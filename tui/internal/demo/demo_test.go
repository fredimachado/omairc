package demo

import (
	"sort"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// attachController seeds a fresh controller and returns it with the demo
// server. Tests are in-package so they can drive the unexported tryAnswer*
// handlers directly, matching tst_demoserver.cpp.
func attachController(t *testing.T, autoEcho bool) (*controller.Controller, *DemoServer) {
	t.Helper()
	c := controller.New()
	d := New()
	if !d.Attach(c, autoEcho) {
		t.Fatalf("Attach failed: %q", d.LastError())
	}
	return c, d
}

func wireLines(payload []byte) []string {
	text := strings.TrimSuffix(string(payload), "\r\n")
	if text == "" {
		return nil
	}
	return strings.Split(text, "\r\n")
}

func TestAnswersClientPing(t *testing.T) {
	_, d := attachController(t, true)
	got := d.tryAnswerPing([]byte("PING :omairc-watchdog\r\n"))
	want := ":server PONG irc.example :omairc-watchdog\r\n"
	if string(got) != want {
		t.Fatalf("PONG = %q, want %q", got, want)
	}
}

func TestAnswersMonitorAdd(t *testing.T) {
	_, d := attachController(t, true)
	got := d.tryAnswerMonitor("omarchy", "fred",
		[]byte("MONITOR + anna,ghost\r\n"), demoOnlineNicks(omarchyWorld()))
	want := ":server 730 fred :anna!u@h\r\n" + ":server 731 fred :ghost\r\n"
	if string(got) != want {
		t.Fatalf("MONITOR + = %q, want %q", got, want)
	}
}

func TestAnswersList(t *testing.T) {
	_, d := attachController(t, true)

	reply := d.tryAnswerList(omarchyWorld(), []byte("LIST\r\n"))
	if reply == nil {
		t.Fatal("LIST was not answered")
	}
	lines := wireLines(reply)
	if len(lines) == 0 {
		t.Fatal("LIST reply was empty")
	}
	if lines[0] != ":server 321 fred Channel :Users  Name" {
		t.Fatalf("321 header = %q", lines[0])
	}
	if lines[len(lines)-1] != ":server 323 fred :End of /LIST" {
		t.Fatalf("323 footer = %q", lines[len(lines)-1])
	}

	var rows []SeedListed
	for _, line := range lines {
		if !strings.HasPrefix(line, ":server 322 ") {
			continue
		}
		message, err := irc.Parse(line)
		if err != nil || len(message.Params) != 4 {
			t.Fatalf("bad 322 line %q: %v %+v", line, err, message.Params)
		}
		users, err := strconv.Atoi(message.Params[2])
		if err != nil {
			t.Fatalf("bad 322 users %q: %v", message.Params[2], err)
		}
		rows = append(rows, SeedListed{Name: message.Params[1], Users: users, Topic: message.Params[3]})
	}
	if len(rows) != 6 {
		t.Fatalf("322 row count = %d, want 6", len(rows))
	}

	// The C++ test observes ChannelListModel's users-descending sort.
	sort.SliceStable(rows, func(i, j int) bool { return rows[i].Users > rows[j].Users })
	if rows[0].Name != "#linux" || rows[0].Users != 42 {
		t.Fatalf("row 0 = %+v, want #linux/42", rows[0])
	}
	// The directory topic carries literal STX and mIRC color bytes.
	wantLinuxTopic := "\x02Kernel\x02 discussion and \x03" + "04distro" + "\x03 help."
	if rows[0].Topic != wantLinuxTopic {
		t.Fatalf("row 0 topic = %q, want %q", rows[0].Topic, wantLinuxTopic)
	}
	if rows[1].Name != "#omarchy" || rows[1].Users != 12 {
		t.Fatalf("row 1 = %+v, want #omarchy/12", rows[1])
	}

	filtered := d.tryAnswerList(omarchyWorld(), []byte("LIST #l*\r\n"))
	var filteredRows []string
	for _, line := range wireLines(filtered) {
		if strings.HasPrefix(line, ":server 322 ") {
			filteredRows = append(filteredRows, line)
		}
	}
	if len(filteredRows) != 1 || !strings.Contains(filteredRows[0], " #linux ") {
		t.Fatalf("LIST #l* = %v, want one #linux row", filteredRows)
	}

	if got := d.tryAnswerList(omarchyWorld(), []byte("MODE #omarchy\r\n")); got != nil {
		t.Fatalf("non-LIST frame answered %q", got)
	}
}

func TestSeedsServiceAccounts(t *testing.T) {
	c, _ := attachController(t, false)
	want := map[string]string{
		"lena": "pinkieval",
		"fred": "fredm",
		"sol":  "solarius",
		"teo":  "teoval",
		"kai":  "kaidev",
	}
	for nick, account := range want {
		if got := c.Reducer().NickPresence("omarchy", nick).Account; got != account {
			t.Fatalf("account(%s) = %q, want %q", nick, got, account)
		}
	}
}

func TestSkipsRedundantAccountTag(t *testing.T) {
	c, d := attachController(t, false)
	epoch := c.PeerAccountEpoch()
	d.omarchy.InjectBytes([]byte("@account=kaidev :kai!u@h PRIVMSG #omarchy :still here\r\n"))
	if got := c.Reducer().NickPresence("omarchy", "kai").Account; got != "kaidev" {
		t.Fatalf("account(kai) = %q, want kaidev", got)
	}
	if got := c.PeerAccountEpoch(); got != epoch {
		t.Fatalf("PeerAccountEpoch moved from %d to %d", epoch, got)
	}
}

func TestSeedsParityWorld(t *testing.T) {
	c := controller.New()
	// A fake clock keeps the typing hints from expiring and stops every
	// session timer. Do not Advance it.
	clock := session.NewFakeClock(time.Date(2026, 9, 12, 12, 0, 0, 0, time.UTC))
	c.SetClock(clock)
	d := New()
	if !d.Attach(c, false) {
		t.Fatalf("Attach failed: %q", d.LastError())
	}

	// Manager.NetworkIDs sorts; the C++ insertion order is not preserved.
	networkIDs := c.NetworkIDs()
	sort.Strings(networkIDs)
	if strings.Join(networkIDs, ",") != "oftc,omarchy" {
		t.Fatalf("NetworkIDs = %v, want oftc,omarchy", c.NetworkIDs())
	}

	type row struct {
		network   string
		target    string
		unread    int
		mention   bool
		typing    bool
		isChannel bool
	}
	var wantRows = []row{
		{"omarchy", "#desktop", 3, false, false, true},
		{"omarchy", "#help", 0, false, false, true},
		{"omarchy", "#omarchy", 0, false, false, true},
		{"omarchy", "#ricing", 12, true, false, true},
		{"omarchy", "anna", 1, true, true, false},
		{"omarchy", "dax", 0, false, false, false},
		{"oftc", "#build", 2, false, false, true},
		{"oftc", "#lab", 0, false, false, true},
		{"oftc", "#omarchy", 0, false, false, true},
		{"oftc", "rio", 0, false, false, false},
	}
	gotRows := c.Conversations()
	if len(gotRows) != len(wantRows) {
		t.Fatalf("sidebar row count = %d, want %d: %+v", len(gotRows), len(wantRows), gotRows)
	}
	for index, want := range wantRows {
		got := gotRows[index]
		if got.NetworkID != want.network || got.Conversation != want.target ||
			got.Unread != want.unread || got.Mention != want.mention ||
			got.Typing != want.typing || got.Direct != !want.isChannel {
			t.Fatalf("row[%d] = %+v, want %+v", index, got, want)
		}
	}

	key := c.Reducer().ConversationKey("omarchy", "#omarchy")
	var nicks []string
	for _, member := range c.Reducer().OrderedMembers(key) {
		nicks = append(nicks, member.Nick)
	}
	wantNicks := []string{"fred", "anna", "dax", "mira", "kai", "teo", "ivy", "lena", "max", "nora", "sam", "sol"}
	if strings.Join(nicks, " ") != strings.Join(wantNicks, " ") {
		t.Fatalf("ordered members = %v, want %v", nicks, wantNicks)
	}

	wantLabels := map[string]string{
		"fred": "~fred",
		"anna": "&anna",
		"dax":  "@dax",
		"mira": "@mira",
		"kai":  "%kai",
		"teo":  "+teo",
	}
	for nick, label := range wantLabels {
		view, ok := c.Reducer().MemberView(key, nick)
		if !ok {
			t.Fatalf("MemberView(%s) missing", nick)
		}
		if view.Label != label {
			t.Fatalf("label(%s) = %q, want %q", nick, view.Label, label)
		}
	}

	for _, nick := range []string{"teo", "lena", "sam", "ivy", "max"} {
		if c.Reducer().NickPresence("omarchy", nick).Away == nil {
			t.Fatalf("presence(%s).Away = nil, want away", nick)
		}
	}
	for _, nick := range []string{"anna", "dax"} {
		if c.Reducer().NickPresence("omarchy", nick).Away != nil {
			t.Fatalf("presence(%s).Away set, want nil", nick)
		}
	}

	wantStatus := map[string]string{
		"anna": "writing docs",
		"dax":  "on #desktop",
		"mira": "making tea",
		"sol":  "new here",
		"fred": "building Omairc",
	}
	for nick, status := range wantStatus {
		view, ok := c.Reducer().MemberView(key, nick)
		if !ok {
			t.Fatalf("MemberView(%s) missing", nick)
		}
		if view.Status != status {
			t.Fatalf("status(%s) = %q, want %q", nick, view.Status, status)
		}
	}

	c.SelectConversation("omarchy", "#omarchy")
	if !containsString(c.TypingNicks(), "anna") {
		t.Fatalf("typing in #omarchy = %v, want anna", c.TypingNicks())
	}
	c.SelectConversation("omarchy", "anna")
	if !containsString(c.TypingNicks(), "anna") {
		t.Fatalf("typing in anna = %v, want anna", c.TypingNicks())
	}
}

func TestEchoLastPrivmsg(t *testing.T) {
	c, d := attachController(t, true)
	c.SelectConversation("omarchy", "#omarchy")
	if !c.SendMessage("hello") {
		t.Fatal("SendMessage failed")
	}
	found := false
	for _, frame := range d.omarchy.WrittenFrames() {
		if strings.HasPrefix(string(frame), "PRIVMSG #omarchy :hello") {
			found = true
		}
	}
	if !found {
		t.Fatal("transport did not record the client PRIVMSG")
	}
	if !d.EchoLastOmarchyPrivmsg("fred") {
		t.Fatal("EchoLastOmarchyPrivmsg = false, want true")
	}
}

func containsString(values []string, wanted string) bool {
	for _, value := range values {
		if value == wanted {
			return true
		}
	}
	return false
}
