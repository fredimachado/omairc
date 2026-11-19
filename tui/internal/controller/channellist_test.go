package controller

import (
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/session"
)

func channelListClock() *session.FakeClock {
	return session.NewFakeClock(time.Date(2026, 9, 26, 12, 0, 0, 0, time.UTC))
}

func TestChannelListRequestMissing(t *testing.T) {
	r := NewChannelListRequest(channelListClock())
	if _, ok := r.State("n"); ok {
		t.Fatal("State(missing) reported present")
	}
	if res := r.Finish("n"); res.Completed || res.HasPending || res.PendingMask != "" {
		t.Fatalf("Finish(missing) = %+v, want zero", res)
	}
	if r.Fail("n", "x", false) {
		t.Fatal("Fail(missing) = true, want false")
	}
	if r.FireIdleTimeout("n") {
		t.Fatal("FireIdleTimeout(missing) = true, want false")
	}
}

func TestChannelListRequestStartCachedLoading(t *testing.T) {
	r := NewChannelListRequest(channelListClock())

	if kind := r.Request("n", "#a", false); kind != ChannelListStart {
		t.Fatalf("first Request = %v, want Start", kind)
	}
	state, ok := r.State("n")
	if !ok || !state.Loading || state.Complete || state.Mask != "#a" {
		t.Fatalf("state after start = %+v, want loading mask #a", state)
	}

	// A same-mask request while loading stays loading and clears the pending mask.
	if kind := r.Request("n", "#A", false); kind != ChannelListLoading {
		t.Fatalf("same-mask Request = %v, want Loading", kind)
	}
	state, _ = r.State("n")
	if state.HasPendingMask {
		t.Fatal("same-mask request left a pending mask")
	}

	// A different mask while loading becomes the pending mask.
	if kind := r.Request("n", "#b", false); kind != ChannelListLoading {
		t.Fatalf("different-mask Request = %v, want Loading", kind)
	}
	state, _ = r.State("n")
	if !state.HasPendingMask || state.PendingMask != "#b" {
		t.Fatalf("pending after different mask = %+v, want #b", state)
	}
	// A same-mask request again clears that pending mask.
	if kind := r.Request("n", "#A", false); kind != ChannelListLoading {
		t.Fatalf("same-mask Request = %v, want Loading", kind)
	}
	state, _ = r.State("n")
	if state.HasPendingMask {
		t.Fatal("same-mask request did not clear pending")
	}

	if res := r.Finish("n"); !res.Completed {
		t.Fatalf("Finish = %+v, want Completed", res)
	}
	state, _ = r.State("n")
	if state.Loading || !state.Complete {
		t.Fatalf("state after finish = %+v, want complete", state)
	}

	// Cache reuse: the same mask without refresh is Cached.
	if kind := r.Request("n", "#A", false); kind != ChannelListCached {
		t.Fatalf("cached Request = %v, want Cached", kind)
	}
	// A refresh forces a reload even for the same mask.
	if kind := r.Request("n", "#a", true); kind != ChannelListStart {
		t.Fatalf("refresh Request = %v, want Start", kind)
	}
	state, _ = r.State("n")
	if !state.Loading || state.Complete {
		t.Fatalf("state after refresh = %+v, want loading", state)
	}
}

func TestChannelListRequestStateCopiesRows(t *testing.T) {
	r := NewChannelListRequest(channelListClock())
	r.Request("n", "*", false)
	r.Row("n", ChannelListRow{Channel: "#a", Users: 1})
	state, _ := r.State("n")
	state.Rows[0].Users = 99
	again, _ := r.State("n")
	if again.Rows[0].Users != 1 {
		t.Fatalf("State exposed the cache: %+v", again.Rows[0])
	}
}

func TestChannelListRequestRowReplace(t *testing.T) {
	r := NewChannelListRequest(channelListClock())
	if r.Row("n", ChannelListRow{Channel: "#a"}) {
		t.Fatal("Row without a request reported a replace")
	}
	r.Request("n", "*", false)
	if replaced := r.Row("n", ChannelListRow{Channel: "#Chan", Users: 1}); replaced {
		t.Fatal("first Row reported a replace")
	}
	if replaced := r.Row("n", ChannelListRow{Channel: "#chan", Users: 7, Topic: "t"}); !replaced {
		t.Fatal("case-insensitive duplicate did not replace")
	}
	state, _ := r.State("n")
	if len(state.Rows) != 1 || state.Rows[0].Users != 7 || state.Rows[0].Topic != "t" {
		t.Fatalf("rows after replace = %+v", state.Rows)
	}

	// An empty channel is ignored.
	if r.Row("n", ChannelListRow{Users: 3}) {
		t.Fatal("empty channel reported a replace")
	}
	state, _ = r.State("n")
	if len(state.Rows) != 1 {
		t.Fatalf("empty channel row count = %d, want 1", len(state.Rows))
	}

	// Rows for a finished request are ignored.
	r.Finish("n")
	r.Row("n", ChannelListRow{Channel: "#later"})
	state, _ = r.State("n")
	if len(state.Rows) != 1 {
		t.Fatalf("post-finish row count = %d, want 1", len(state.Rows))
	}
}

func TestChannelListRequestRowCap(t *testing.T) {
	r := NewChannelListRequest(channelListClock())
	r.Request("n", "*", false)
	for index := 0; index < ChannelListMaxRows; index++ {
		r.Row("n", ChannelListRow{Channel: fmt.Sprintf("#c%d", index), Users: 1})
	}
	if replaced := r.Row("n", ChannelListRow{Channel: "#overflow", Users: 2}); replaced {
		t.Fatal("overflow row reported a replace")
	}
	state, _ := r.State("n")
	if len(state.Rows) != ChannelListMaxRows {
		t.Fatalf("row count = %d, want %d", len(state.Rows), ChannelListMaxRows)
	}
	// A replace still works at the cap.
	if replaced := r.Row("n", ChannelListRow{Channel: "#C0", Users: 9}); !replaced {
		t.Fatal("replace at cap failed")
	}
	state, _ = r.State("n")
	if state.Rows[0].Users != 9 {
		t.Fatalf("rows[0] = %+v, want users 9", state.Rows[0])
	}
}

func TestChannelListRequestIdleTimerAndActivity(t *testing.T) {
	clock := channelListClock()
	r := NewChannelListRequest(clock)
	r.Request("n", "*", false)

	// The default 30s deadline is re-armed by Activity.
	clock.Advance(20 * time.Second)
	r.Activity("n")
	clock.Advance(20 * time.Second)
	state, _ := r.State("n")
	if !state.Loading || state.Error != "" {
		t.Fatalf("state after activity = %+v, want still loading", state)
	}
	clock.Advance(10 * time.Second)
	state, _ = r.State("n")
	if state.Loading {
		t.Fatal("idle timer did not fire after the activity window")
	}
	if state.Error != channelListTimeoutText {
		t.Fatalf("timeout error = %q, want %q", state.Error, channelListTimeoutText)
	}
	if !state.DrainTimedOutEnd {
		t.Fatalf("timeout state = %+v, want drain flag", state)
	}
}

func TestChannelListRequestActivityOnlyWhileLoading(t *testing.T) {
	clock := channelListClock()
	r := NewChannelListRequest(clock)
	r.Request("n", "*", false)
	r.Fail("n", "boom", false)
	r.Activity("n")
	if clock.Pending() != 0 {
		t.Fatalf("Activity re-armed a non-loading request: pending = %d", clock.Pending())
	}
}

func TestChannelListRequestSetIdleTimeoutClamp(t *testing.T) {
	clock := channelListClock()
	r := NewChannelListRequest(clock)
	r.SetIdleTimeout(0)
	r.Request("n", "*", false)
	clock.Advance(time.Millisecond)
	state, _ := r.State("n")
	if state.Loading {
		t.Fatal("zero timeout was not clamped to 1ms")
	}
	if state.Error != channelListTimeoutText {
		t.Fatalf("error = %q, want %q", state.Error, channelListTimeoutText)
	}
}

func TestChannelListRequestFinishPending(t *testing.T) {
	r := NewChannelListRequest(channelListClock())
	r.Request("n", "#a", false)
	r.Request("n", "#b", false)
	res := r.Finish("n")
	if !res.Completed || !res.HasPending || res.PendingMask != "#b" {
		t.Fatalf("Finish = %+v, want completed pending #b", res)
	}
	if res = r.Finish("n"); res.Completed || res.HasPending {
		t.Fatalf("second Finish = %+v, want zero", res)
	}

	// A pending mask equal to the current mask is dropped.
	r.Request("m", "#a", false)
	r.Request("m", "#b", false)
	r.Request("m", "#A", false)
	res = r.Finish("m")
	if !res.Completed || res.HasPending || res.PendingMask != "" {
		t.Fatalf("equal-mask Finish = %+v, want completed with no pending", res)
	}
}

func TestChannelListRequestFinishDrainTimedOutEnd(t *testing.T) {
	r := NewChannelListRequest(channelListClock())
	r.Request("n", "*", false)
	if !r.Fail("n", channelListTimeoutText, true) {
		t.Fatal("Fail(timedOut) = false")
	}
	state, _ := r.State("n")
	if state.Loading || state.Complete || !state.DrainTimedOutEnd {
		t.Fatalf("state after timeout = %+v", state)
	}

	// Restarting preserves the drain flag.
	if kind := r.Request("n", "*", false); kind != ChannelListStart {
		t.Fatalf("restart = %v, want Start", kind)
	}
	state, _ = r.State("n")
	if !state.Loading || !state.DrainTimedOutEnd {
		t.Fatalf("state after restart = %+v, want loading with drain", state)
	}

	// The first Finish swallows the stale END and keeps loading.
	if res := r.Finish("n"); res.Completed || res.HasPending {
		t.Fatalf("drain Finish = %+v, want zero", res)
	}
	state, _ = r.State("n")
	if !state.Loading || state.DrainTimedOutEnd {
		t.Fatalf("state after drain = %+v, want loading without drain", state)
	}

	// The next Finish completes.
	if res := r.Finish("n"); !res.Completed {
		t.Fatalf("post-drain Finish = %+v, want completed", res)
	}
}

func TestChannelListRequestFail(t *testing.T) {
	r := NewChannelListRequest(channelListClock())
	r.Request("n", "*", false)
	if !r.Fail("n", "boom", false) {
		t.Fatal("Fail = false, want true")
	}
	state, _ := r.State("n")
	if state.Loading || state.Complete || state.Error != "boom" || state.DrainTimedOutEnd {
		t.Fatalf("state after Fail = %+v", state)
	}
	if r.Fail("n", "again", false) {
		t.Fatal("second Fail = true, want false")
	}
}

func TestChannelListRequestForgetStopsTimer(t *testing.T) {
	clock := channelListClock()
	r := NewChannelListRequest(clock)
	r.Request("n", "*", false)
	if clock.Pending() != 1 {
		t.Fatalf("pending after Start = %d, want 1", clock.Pending())
	}
	r.Forget("n")
	if _, ok := r.State("n"); ok {
		t.Fatal("Forget left state behind")
	}
	if clock.Pending() != 0 {
		t.Fatalf("pending after Forget = %d, want 0", clock.Pending())
	}
	r.Request("m", "*", false)
	r.CancelStart("m")
	if _, ok := r.State("m"); ok {
		t.Fatal("CancelStart left state behind")
	}
}

func TestSameChannelListMask(t *testing.T) {
	cases := []struct {
		left, right string
		want        bool
	}{
		{"#a", "#A", true},
		{" #a ", "#A", true},
		{"#a", "#b", false},
		{"", "  ", true},
	}
	for _, c := range cases {
		if got := SameChannelListMask(c.left, c.right); got != c.want {
			t.Fatalf("SameChannelListMask(%q, %q) = %v, want %v", c.left, c.right, got, c.want)
		}
	}
}

func TestChannelListModelEmptyState(t *testing.T) {
	m := NewChannelListModel()
	state := m.State()
	if state.NetworkID != "" || state.Mask != "" || state.Sources != 0 || len(state.Rows) != 0 {
		t.Fatalf("empty State = %+v", state)
	}
	if state.Loading || state.Complete || state.Cached || state.Error != "" || state.Filter != "" {
		t.Fatalf("empty State flags = %+v", state)
	}
}

func TestChannelListModelShowSorts(t *testing.T) {
	m := NewChannelListModel()
	rows := []ChannelListRow{
		{Channel: "#b", Users: 1, Topic: "b"},
		{Channel: "#a", Users: 5, Topic: "a"},
		{Channel: "#c", Users: 5, Topic: "c"},
	}
	m.Show("net", "*", rows, true, false, true, "")
	state := m.State()
	if state.NetworkID != "net" || state.Mask != "*" || state.Sources != 3 {
		t.Fatalf("State = %+v", state)
	}
	if !state.Complete || state.Loading || !state.Cached || state.Error != "" {
		t.Fatalf("State flags = %+v", state)
	}
	want := []string{"#a", "#c", "#b"}
	if len(state.Rows) != len(want) {
		t.Fatalf("rows = %+v, want %v", state.Rows, want)
	}
	for index, channel := range want {
		if state.Rows[index].Channel != channel {
			t.Fatalf("rows[%d] = %q, want %q", index, state.Rows[index].Channel, channel)
		}
	}
	// Show must not mutate the caller's slice even though it sorts.
	if rows[0].Channel != "#b" {
		t.Fatalf("Show mutated caller rows: %+v", rows)
	}
}

func TestChannelListModelShowCap(t *testing.T) {
	m := NewChannelListModel()
	rows := make([]ChannelListRow, ChannelListMaxRows+10)
	for index := range rows {
		rows[index] = ChannelListRow{Channel: fmt.Sprintf("#c%d", index), Users: index}
	}
	m.Show("n", "*", rows, true, false, false, "")
	state := m.State()
	if state.Sources != ChannelListMaxRows || len(state.Rows) != ChannelListMaxRows {
		t.Fatalf("sources = %d rows = %d, want %d", state.Sources, len(state.Rows), ChannelListMaxRows)
	}
	// The cap keeps the first ChannelListMaxRows before sorting, so the highest
	// surviving user count is the last kept row.
	if state.Rows[0].Users != ChannelListMaxRows-1 {
		t.Fatalf("rows[0].Users = %d, want %d", state.Rows[0].Users, ChannelListMaxRows-1)
	}
}

func TestChannelListModelFilter(t *testing.T) {
	m := NewChannelListModel()
	m.Show("n", "*", []ChannelListRow{
		{Channel: "#go", Users: 3, Topic: "\x0304rust\x03 language"},
		{Channel: "#cpp", Users: 2, Topic: "C++ \x04FF0000stuff\x04"},
		{Channel: "#idle", Users: 1, Topic: "chat"},
	}, true, false, false, "")

	m.SetFilter("rust")
	state := m.State()
	if len(state.Rows) != 1 || state.Rows[0].Channel != "#go" {
		t.Fatalf("filter rust = %+v", state.Rows)
	}
	if state.Filter != "rust" {
		t.Fatalf("Filter = %q, want rust", state.Filter)
	}

	m.SetFilter("c++")
	state = m.State()
	if len(state.Rows) != 1 || state.Rows[0].Channel != "#cpp" {
		t.Fatalf("filter c++ = %+v", state.Rows)
	}

	m.SetFilter("#IDLE")
	state = m.State()
	if len(state.Rows) != 1 || state.Rows[0].Channel != "#idle" {
		t.Fatalf("filter #IDLE = %+v", state.Rows)
	}

	m.SetFilter("  ")
	state = m.State()
	if len(state.Rows) != 3 {
		t.Fatalf("blank filter = %+v, want all rows", state.Rows)
	}
	if state.Filter != "  " {
		t.Fatalf("Filter = %q, want the raw filter", state.Filter)
	}
	if m.Filter() != "  " {
		t.Fatalf("Filter() = %q, want the raw filter", m.Filter())
	}
}

func TestChannelListModelBeginLoadAndAppend(t *testing.T) {
	m := NewChannelListModel()
	m.BeginLoad("n", "*")
	state := m.State()
	if state.NetworkID != "n" || state.Mask != "*" {
		t.Fatalf("BeginLoad state = %+v", state)
	}
	if !state.Loading || state.Complete || state.Cached || state.Sources != 0 || len(state.Rows) != 0 {
		t.Fatalf("BeginLoad flags = %+v", state)
	}

	m.SetFilter("rust")
	m.AppendRow(ChannelListRow{Channel: "#a", Users: 2, Topic: "rust"})
	m.AppendRow(ChannelListRow{Channel: "#b", Users: 9, Topic: "go"})
	state = m.State()
	if state.Sources != 2 {
		t.Fatalf("sources = %d, want 2", state.Sources)
	}
	if len(state.Rows) != 1 || state.Rows[0].Channel != "#a" {
		t.Fatalf("filtered append rows = %+v", state.Rows)
	}
}

func TestChannelListModelAppendStateSorted(t *testing.T) {
	m := NewChannelListModel()
	m.BeginLoad("n", "*")
	m.AppendRow(ChannelListRow{Channel: "#low", Users: 1})
	m.AppendRow(ChannelListRow{Channel: "#high", Users: 5})
	m.AppendRow(ChannelListRow{Channel: "#mid", Users: 3})
	state := m.State()
	want := []string{"#high", "#mid", "#low"}
	for index, channel := range want {
		if state.Rows[index].Channel != channel {
			t.Fatalf("rows[%d] = %q, want %q", index, state.Rows[index].Channel, channel)
		}
	}
}

func TestChannelListModelAppendCap(t *testing.T) {
	m := NewChannelListModel()
	m.BeginLoad("n", "*")
	for index := 0; index < ChannelListMaxRows; index++ {
		m.AppendRow(ChannelListRow{Channel: fmt.Sprintf("#c%d", index), Users: 1})
	}
	m.AppendRow(ChannelListRow{Channel: "#overflow", Users: 99})
	state := m.State()
	if state.Sources != ChannelListMaxRows || len(state.Rows) != ChannelListMaxRows {
		t.Fatalf("sources = %d rows = %d, want %d", state.Sources, len(state.Rows), ChannelListMaxRows)
	}
}

func TestChannelListModelFailAndClear(t *testing.T) {
	m := NewChannelListModel()
	m.Show("n", "*", []ChannelListRow{{Channel: "#a", Users: 1}}, true, false, true, "")
	m.Fail("boom")
	state := m.State()
	if state.Loading || state.Complete || state.Cached || state.Error != "boom" {
		t.Fatalf("Fail state = %+v", state)
	}
	if state.Sources != 1 || len(state.Rows) != 1 {
		t.Fatalf("Fail dropped cached rows: %+v", state)
	}

	m.Clear()
	state = m.State()
	if state.NetworkID != "" || state.Mask != "" || state.Sources != 0 || len(state.Rows) != 0 ||
		state.Loading || state.Complete || state.Cached || state.Error != "" {
		t.Fatalf("Clear state = %+v", state)
	}

	// Clear is a no-op on an already empty model.
	m.Clear()
	if state = m.State(); state.NetworkID != "" || state.Sources != 0 {
		t.Fatalf("second Clear changed state = %+v", state)
	}
}

func TestChannelListModelPlainTopic(t *testing.T) {
	m := NewChannelListModel()
	cases := []struct {
		in   string
		want string
	}{
		{"\x0304red\x03 plain", "red plain"},
		{"\x0304,02both\x03!", "both!"},
		{"\x04FF0000hex\x04 end", "hex end"},
		{"\x04FF0000,00FF00both\x04!", "both!"},
		{
			"\x02bold\x02 \x1ditalic\x1d \x1funder\x1f \x16rev\x16 " +
				"\x11mono\x11 \x1estrike\x1e \x0freset",
			"bold italic under rev mono strike reset",
		},
		{"no codes", "no codes"},
	}
	for _, c := range cases {
		if got := m.PlainTopic(c.in); got != c.want {
			t.Fatalf("PlainTopic(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}

// requireListPhase checks the loading and complete flags of network "n".
func requireListPhase(t *testing.T, r *ChannelListRequest, when string, loading, complete bool) {
	t.Helper()
	state, _ := r.State("n")
	if state.Loading != loading || state.Complete != complete {
		t.Fatalf("%s: loading=%v complete=%v, want loading=%v complete=%v (%+v)",
			when, state.Loading, state.Complete, loading, complete, state)
	}
}

// TestChannelListRequestPhasesAreExclusive pins that a load is loading or complete
// and never both, and that a failed load is neither.
func TestChannelListRequestPhasesAreExclusive(t *testing.T) {
	r := NewChannelListRequest(channelListClock())
	r.Request("n", "*", false)
	requireListPhase(t, r, "started", true, false)

	r.Finish("n")
	requireListPhase(t, r, "finished", false, true)

	r.Request("n", "*", true)
	requireListPhase(t, r, "refreshed", true, false)

	r.Fail("n", "boom", false)
	requireListPhase(t, r, "failed", false, false)
	state, _ := r.State("n")
	if state.Error != "boom" {
		t.Fatalf("failed state error = %q, want boom", state.Error)
	}
}

// TestChannelListRequestQueuesOneMask pins that a second mask while loading takes the
// single pending slot without arming another timer, and that the in-flight mask in any
// case clears it.
func TestChannelListRequestQueuesOneMask(t *testing.T) {
	clock := channelListClock()
	r := NewChannelListRequest(clock)
	r.Request("n", "#a", false)
	pendingBefore := clock.Pending()

	r.Request("n", "#b", false)
	r.Request("n", "#c", false)
	state, _ := r.State("n")
	if !state.HasPendingMask || state.PendingMask != "#c" {
		t.Fatalf("pending after #b then #c = %+v, want the one slot to hold #c", state)
	}
	if clock.Pending() != pendingBefore {
		t.Fatalf("a queued mask armed a timer: pending %d -> %d", pendingBefore, clock.Pending())
	}

	if kind := r.Request("n", "#A", false); kind != ChannelListLoading {
		t.Fatalf("same-mask Request = %v, want Loading", kind)
	}
	state, _ = r.State("n")
	if state.HasPendingMask || !state.Loading || state.Mask != "#a" {
		t.Fatalf("same mask did not clear the queue: %+v", state)
	}
}

// TestChannelListRequestCompletedNewerMaskCarriesNoDrain pins that a newer mask
// drains the timed-out load's stale END once, completes on the real END, and carries
// no drain flag into a later load.
func TestChannelListRequestCompletedNewerMaskCarriesNoDrain(t *testing.T) {
	r := NewChannelListRequest(channelListClock())
	r.Request("n", "*", false)
	r.Fail("n", channelListTimeoutText, true)

	r.Request("n", "#x", false)
	if res := r.Finish("n"); res.Completed {
		t.Fatalf("stale END completed #x: %+v", res)
	}
	if res := r.Finish("n"); !res.Completed {
		t.Fatalf("real END did not complete #x: %+v", res)
	}
	state, _ := r.State("n")
	if state.DrainTimedOutEnd || !state.Complete {
		t.Fatalf("completed #x = %+v, want complete without a drain flag", state)
	}

	r.Request("n", "#y", false)
	if res := r.Finish("n"); !res.Completed {
		t.Fatalf("first END of #y = %+v, want it to complete (not swallowed)", res)
	}
}

// TestChannelListRequestFailedKeepsRowsAndError pins that a timed-out load keeps the
// rows it streamed and carries the timeout text.
func TestChannelListRequestFailedKeepsRowsAndError(t *testing.T) {
	clock := channelListClock()
	r := NewChannelListRequest(clock)
	r.Request("n", "*", false)
	r.Row("n", ChannelListRow{Channel: "#kept", Users: 4})
	clock.Advance(31 * time.Second)

	state, _ := r.State("n")
	requireListPhase(t, r, "timed out", false, false)
	if state.Error != channelListTimeoutText {
		t.Fatalf("error = %q, want %q", state.Error, channelListTimeoutText)
	}
	if len(state.Rows) != 1 || state.Rows[0].Channel != "#kept" {
		t.Fatalf("rows after timeout = %+v, want #kept", state.Rows)
	}
}

// TestChannelListRequestNonTimeoutFailKeepsDrain pins today's behavior: a failure that
// is not a timeout keeps the drain of the timed-out load it replaced, and the stale END
// is then absorbed by the failed state without completing anything.
func TestChannelListRequestNonTimeoutFailKeepsDrain(t *testing.T) {
	r := NewChannelListRequest(channelListClock())
	r.Request("n", "*", false)
	r.Fail("n", channelListTimeoutText, true)
	r.Request("n", "#x", false)
	if !r.Fail("n", "Too many channel matches", false) {
		t.Fatal("Fail(non-timeout) = false, want true")
	}
	state, _ := r.State("n")
	if state.Loading || state.Complete || !state.DrainTimedOutEnd {
		t.Fatalf("state after non-timeout fail = %+v, want failed with the drain kept", state)
	}

	if res := r.Finish("n"); res.Completed {
		t.Fatalf("END on a failed request completed: %+v", res)
	}
	state, _ = r.State("n")
	if state.Loading || state.Complete || state.DrainTimedOutEnd {
		t.Fatalf("state after the END = %+v, want failed with no drain", state)
	}
}

// recordingClock keeps each AfterFunc callback unfired, so a test can run one late the
// way a real timer that fired just before it was superseded would.
type recordingClock struct {
	callbacks []func()
}

func (c *recordingClock) Now() time.Time { return time.Time{} }

func (c *recordingClock) AfterFunc(_ time.Duration, f func()) session.Timer {
	c.callbacks = append(c.callbacks, f)
	return recordingTimer{}
}

type recordingTimer struct{}

func (recordingTimer) Stop() bool { return true }

// TestChannelListRequestStaleTimerCallbackIgnored pins that the idle callback of a
// superseded load cannot fail the newer load.
func TestChannelListRequestStaleTimerCallbackIgnored(t *testing.T) {
	clock := &recordingClock{}
	r := NewChannelListRequest(clock)
	r.Request("n", "*", false)
	stale := clock.callbacks[0]
	r.Finish("n")
	r.Request("n", "#new", false)

	stale()

	state, _ := r.State("n")
	if !state.Loading || state.Error != "" {
		t.Fatalf("a stale timer callback failed the newer load: %+v", state)
	}
}

// TestChannelListStaleEndAfterTimeoutDoesNotCompleteTheNewerLoad drives the timeout and
// the newer load through the controller: the timed-out LIST's late 323 is drained and
// the newer load completes only on its own end.
func TestChannelListStaleEndAfterTimeoutDoesNotCompleteTheNewerLoad(t *testing.T) {
	c, transport := phase7Setup(t)
	clock := c.clock.(*session.FakeClock)

	if !c.SendMessage("/list *") {
		t.Fatal("SendMessage(/list *) = false")
	}
	clock.Advance(31 * time.Second)

	if !c.SendMessage("/list #new") {
		t.Fatal("SendMessage(/list #new) = false")
	}
	if !writtenFramesContain(transport, "LIST #new") {
		t.Fatalf("no LIST #new frame: %v", transport.WrittenFrames())
	}
	inject(t, transport, ":server 323 omairc :End of /LIST\r\n")
	state, _ := c.channelLists.State("freenode")
	if !state.Loading || state.Complete {
		t.Fatalf("state after the stale 323 = %+v, want #new still loading", state)
	}

	inject(t, transport, ":server 322 omairc #new 3 :new channel\r\n"+
		":server 323 omairc :End of /LIST\r\n")
	state, _ = c.channelLists.State("freenode")
	if state.Loading || !state.Complete || state.Mask != "#new" || len(state.Rows) != 1 {
		t.Fatalf("state after the real end = %+v, want #new complete with one row", state)
	}
}

// answerPings plays a server that answers every PING written from frame index from
// on, in order, and returns the next index to read from.
func answerPings(t *testing.T, transport *session.LoopbackTransport, from int) int {
	t.Helper()
	frames := transport.WrittenFrames()
	for _, frame := range frames[from:] {
		line := strings.TrimRight(string(frame), "\r\n")
		if token, ok := strings.CutPrefix(line, "PING :"); ok {
			inject(t, transport, ":server PONG irc.example :"+token+"\r\n")
		}
	}
	return len(frames)
}

// TestChannelListAfterTimeoutCompletesOnItsOwnPromptEnd pins that a timed-out LIST
// whose 323 never comes does not make later loads swallow their own prompt 323. The
// server answers every PING it gets, the way a live one does.
func TestChannelListAfterTimeoutCompletesOnItsOwnPromptEnd(t *testing.T) {
	c, transport := phase7Setup(t)
	clock := c.clock.(*session.FakeClock)

	if !c.SendMessage("/list *") {
		t.Fatal("SendMessage(/list *) = false")
	}
	clock.Advance(31 * time.Second)
	read := answerPings(t, transport, 0)

	for attempt := 1; attempt <= 3; attempt++ {
		if !c.SendMessage("/list *") {
			t.Fatalf("attempt %d: SendMessage(/list *) = false", attempt)
		}
		read = answerPings(t, transport, read)
		inject(t, transport, ":server 321 omairc Channel :Users  Name\r\n"+
			":server 322 omairc #omarchy 12 :cozy\r\n"+
			":server 323 omairc :End of /LIST\r\n")
		state, _ := c.channelLists.State("freenode")
		if state.Loading || !state.Complete {
			t.Fatalf("attempt %d: request = %+v, want the prompt 323 to complete it", attempt, state)
		}
		snapshot := c.ChannelListSnapshot()
		if snapshot.Loading || !snapshot.Complete || snapshot.Error != "" || len(snapshot.Rows) != 1 {
			t.Fatalf("attempt %d: overlay = %+v, want complete with one row", attempt, snapshot)
		}
	}
}

// TestChannelListTimeoutFailsTheOverlay pins that the idle timeout leaves "Loading"
// on the overlay, not only on the request, and wakes the shell.
func TestChannelListTimeoutFailsTheOverlay(t *testing.T) {
	c, _ := phase7Setup(t)
	clock := c.clock.(*session.FakeClock)
	if !c.SendMessage("/list *") {
		t.Fatal("SendMessage(/list *) = false")
	}
	woken := 0
	c.OnViewChanged = func() { woken++ }

	clock.Advance(31 * time.Second)

	snapshot := c.ChannelListSnapshot()
	if snapshot.Loading || snapshot.Complete || snapshot.Error != channelListTimeoutText {
		t.Fatalf("overlay after timeout = %+v, want the timeout error", snapshot)
	}
	if woken == 0 {
		t.Fatal("the timeout did not wake the shell")
	}
}

// TestChannelListStreamsRowsIntoTheOverlay pins that 322 rows show on the overlay
// while the load is still in flight, the way the desktop overlay streams them.
func TestChannelListStreamsRowsIntoTheOverlay(t *testing.T) {
	c, transport := phase7Setup(t)
	if !c.SendMessage("/list *") {
		t.Fatal("SendMessage(/list *) = false")
	}
	woken := 0
	c.OnViewChanged = func() { woken++ }

	inject(t, transport, ":server 322 omairc #low 2 :low\r\n"+
		":server 322 omairc #high 9 :high\r\n"+
		":server 322 omairc #LOW 5 :low again\r\n")

	snapshot := c.ChannelListSnapshot()
	if !snapshot.Loading || snapshot.Complete {
		t.Fatalf("overlay = %+v, want still loading", snapshot)
	}
	if len(snapshot.Rows) != 2 || snapshot.Rows[0].Channel != "#high" ||
		snapshot.Rows[1].Channel != "#LOW" || snapshot.Rows[1].Users != 5 {
		t.Fatalf("streamed rows = %+v, want #high then the replaced #LOW", snapshot.Rows)
	}
	if woken == 0 {
		t.Fatal("a streamed row did not wake the shell")
	}
}
