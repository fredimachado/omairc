package controller

import (
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// This file ports the coordinator half of the MONITOR behavior from
// tests/session/tst_controller.cpp. It drives the coordinator through a
// recording MonitorHost and real MonitorStore/MuteStore/reducer instances; the
// session is a real registered session on a LoopbackTransport so the MONITOR
// frames the coordinator writes are observable.

// monitorNotice is one recorded NotifyMonitor call.
type monitorNotice struct {
	networkID   string
	display     string
	body        string
	newlyOnline bool
}

// monitorFakeHost records every callback the coordinator makes.
type monitorFakeHost struct {
	now           time.Time
	networkID     string
	session       *session.Session
	sessions      map[string]*session.Session
	selected      irc.ConversationKey
	hasSelected   bool
	entries       []irc.StatusEntry
	notifications []monitorNotice
}

func (h *monitorFakeHost) RecordStatus(entry irc.StatusEntry) {
	h.entries = append(h.entries, entry)
}

func (h *monitorFakeHost) NotifyMonitor(networkID, display, body string, newlyOnline bool) {
	h.notifications = append(h.notifications, monitorNotice{networkID, display, body, newlyOnline})
}

func (h *monitorFakeHost) QueryNetworkID(surface irc.ComposerSurface) string {
	return h.networkID
}

func (h *monitorFakeHost) SessionFor(surface irc.ComposerSurface) *session.Session {
	return h.session
}

func (h *monitorFakeHost) SessionForNetwork(networkID string) *session.Session {
	return h.sessions[networkID]
}

func (h *monitorFakeHost) SelectedKey() (irc.ConversationKey, bool) {
	return h.selected, h.hasSelected
}

func (h *monitorFakeHost) Now() time.Time { return h.now }

// monitorFixture bundles the coordinator and the stores it reads.
type monitorFixture struct {
	reducer     *irc.EventReducer
	monitors    *MonitorStore
	mutes       *MuteStore
	host        *monitorFakeHost
	coordinator *MonitorCoordinator
}

func newMonitorFixture(t *testing.T, featureToken string) *monitorFixture {
	t.Helper()
	reducer := irc.NewEventReducer()
	features := irc.NewServerFeatures()
	if featureToken != "" {
		features.ApplyToken(featureToken)
	}
	reducer.SetServerFeatures("net", features)
	host := &monitorFakeHost{
		now:       time.Date(2026, 9, 26, 10, 0, 0, 0, time.UTC),
		networkID: "net",
		sessions:  map[string]*session.Session{},
	}
	monitors := NewMonitorStore()
	mutes := NewMuteStore()
	return &monitorFixture{
		reducer:     reducer,
		monitors:    monitors,
		mutes:       mutes,
		host:        host,
		coordinator: NewMonitorCoordinator(reducer, monitors, mutes, host),
	}
}

// mapping returns the network's active case mapping.
func (f *monitorFixture) mapping() irc.CaseMapping {
	features := f.reducer.ServerFeatures("net")
	return features.CaseMapping()
}

// texts returns every recorded Status line in order.
func (f *monitorFixture) texts() []string {
	texts := make([]string, 0, len(f.host.entries))
	for _, entry := range f.host.entries {
		texts = append(texts, entry.Text())
	}
	return texts
}

// monitorRegisteredSession builds a real session driven to Registered on a
// loopback transport, so SendMonitor has a live wire to write to.
func monitorRegisteredSession(t *testing.T, networkID, nick string) (*session.Session, *session.LoopbackTransport) {
	t.Helper()
	clock := session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC))
	transport := session.NewLoopbackTransport()
	s := session.NewSession(baseConfig(networkID, nick), transport, clock)
	s.Start()
	transport.CompleteConnect()
	registerNetwork(t, transport, nick)
	if s.State() != session.StateRegistered {
		t.Fatalf("session %q state = %v, want Registered", networkID, s.State())
	}
	return s, transport
}

func TestMonitorSubscribeTruncatesToLimit(t *testing.T) {
	fixture := newMonitorFixture(t, "MONITOR=2")
	for _, nick := range []string{"one", "two", "three"} {
		fixture.monitors.Add("net", nick, fixture.mapping())
	}
	s, transport := monitorRegisteredSession(t, "net", "tester")
	fixture.host.sessions["net"] = s

	fixture.coordinator.SubscribeMonitors("net")
	if got := lastWrittenFrame(t, transport); got != "MONITOR + one,two\r\n" {
		t.Fatalf("subscribe frame = %q, want %q", got, "MONITOR + one,two\r\n")
	}

	before := len(transport.WrittenFrames())
	fixture.coordinator.SubscribeMonitors("net")
	if after := len(transport.WrittenFrames()); after != before {
		t.Fatalf("second subscribe wrote %d extra frames, want 0", after-before)
	}
}

func TestMonitorSubscribeRequiresAdvertisedMonitor(t *testing.T) {
	fixture := newMonitorFixture(t, "")
	fixture.monitors.Add("net", "one", fixture.mapping())
	s, transport := monitorRegisteredSession(t, "net", "tester")
	fixture.host.sessions["net"] = s

	before := len(transport.WrittenFrames())
	fixture.coordinator.SubscribeMonitors("net")
	if after := len(transport.WrittenFrames()); after != before {
		t.Fatalf("subscribe on a non-MONITOR network wrote %d frames, want 0", after-before)
	}
}

func TestMonitorPresenceNotifiesOnlyOnChange(t *testing.T) {
	fixture := newMonitorFixture(t, "MONITOR")
	fixture.monitors.Add("net", "Foo", fixture.mapping())
	message := irc.Message{Command: "730", Params: []string{"tester", "foo!user@host"}}

	// Unknown -> Online only stores the state; it records and notifies nothing.
	fixture.coordinator.HandleMonitorPresence("net", message, true)
	if got := fixture.texts(); len(got) != 0 {
		t.Fatalf("first presence texts = %v, want none", got)
	}
	if len(fixture.host.notifications) != 0 {
		t.Fatalf("first presence notified %d times, want 0", len(fixture.host.notifications))
	}

	// Online -> Offline records and notifies.
	fixture.coordinator.HandleMonitorPresence("net", message, false)
	if got := fixture.texts(); len(got) != 1 || got[0] != "Foo is offline" {
		t.Fatalf("offline presence texts = %v", got)
	}
	if len(fixture.host.notifications) != 1 {
		t.Fatalf("offline presence notifications = %d, want 1", len(fixture.host.notifications))
	}
	notice := fixture.host.notifications[0]
	if notice.display != "Foo" || notice.body != "is offline" || notice.newlyOnline {
		t.Fatalf("offline notice = %+v, want display Foo body is offline newlyOnline false", notice)
	}

	// Repeated offline is a no-op.
	fixture.coordinator.HandleMonitorPresence("net", message, false)
	if len(fixture.host.entries) != 1 || len(fixture.host.notifications) != 1 {
		t.Fatalf("repeat presence changed state: entries=%d notifications=%d",
			len(fixture.host.entries), len(fixture.host.notifications))
	}

	// Offline -> Online records and notifies as newly online.
	fixture.coordinator.HandleMonitorPresence("net", message, true)
	if got := fixture.texts(); len(got) != 2 || got[1] != "Foo is online" {
		t.Fatalf("online presence texts = %v", got)
	}
	if len(fixture.host.notifications) != 2 {
		t.Fatalf("online presence notifications = %d, want 2", len(fixture.host.notifications))
	}
	if !fixture.host.notifications[1].newlyOnline {
		t.Fatalf("online notice newlyOnline = false, want true")
	}
}

func TestMonitorPresenceSkipsNotifyWhenMuted(t *testing.T) {
	fixture := newMonitorFixture(t, "MONITOR")
	fixture.monitors.Add("net", "Foo", fixture.mapping())
	key := fixture.reducer.ConversationKey("net", "Foo")
	fixture.reducer.EnsureConversation(key, "Foo", irc.CauseUserOpen)
	fixture.reducer.SetMuted(key, true)
	message := irc.Message{Command: "730", Params: []string{"tester", "foo!user@host"}}

	fixture.coordinator.HandleMonitorPresence("net", message, true)
	fixture.coordinator.HandleMonitorPresence("net", message, false)
	if len(fixture.host.notifications) != 0 {
		t.Fatalf("muted presence notified %d times, want 0", len(fixture.host.notifications))
	}
	if got := fixture.texts(); len(got) != 1 || got[0] != "Foo is offline" {
		t.Fatalf("muted presence recorded %v, want [Foo is offline]", got)
	}
}

func TestMonitorHandleListFull(t *testing.T) {
	fixture := newMonitorFixture(t, "MONITOR=2")
	fixture.monitors.Add("net", "one", fixture.mapping())
	fixture.monitors.Add("net", "two", fixture.mapping())
	presence := irc.Message{Command: "730", Params: []string{"tester", "one!user@host"}}
	fixture.coordinator.HandleMonitorPresence("net", presence, true)
	fixture.host.entries = nil

	message := irc.Message{Command: "734", Params: []string{"tester", "2", "one,two"}}
	fixture.coordinator.HandleMonitorListFull("net", message)

	if fixture.monitors.Contains("net", "one", fixture.mapping()) {
		t.Fatalf("one was not dropped on 734")
	}
	if fixture.monitors.Contains("net", "two", fixture.mapping()) {
		t.Fatalf("two was not dropped on 734")
	}
	if got := fixture.texts(); len(got) != 1 || got[0] != "Monitor list is full (2): one, two." {
		t.Fatalf("734 texts = %v", got)
	}
}

func TestMonitorDispatchMonitored(t *testing.T) {
	fixture := newMonitorFixture(t, "MONITOR")
	fixture.monitors.Add("net", "one", fixture.mapping())
	fixture.monitors.Add("net", "two", fixture.mapping())
	presence := irc.Message{Command: "730", Params: []string{"tester", "one!user@host"}}
	fixture.coordinator.HandleMonitorPresence("net", presence, true)
	fixture.host.entries = nil
	s, _ := monitorRegisteredSession(t, "net", "tester")
	fixture.host.session = s

	outcome := fixture.coordinator.DispatchMonitor(irc.Command{Verb: irc.VerbMonitored}, irc.SurfaceStatus)
	if outcome != irc.OutcomeSent {
		t.Fatalf("/monitored outcome = %v, want Sent", outcome)
	}
	if got := fixture.texts(); len(got) != 1 || got[0] != "Watching: one (online), two (unknown)" {
		t.Fatalf("/monitored texts = %v", got)
	}

	if outcome := fixture.coordinator.DispatchMonitor(
		irc.Command{Verb: irc.VerbMonitored, Argument: "extra"}, irc.SurfaceStatus); outcome != irc.OutcomeRefused {
		t.Fatalf("/monitored with an argument = %v, want Refused", outcome)
	}

	empty := newMonitorFixture(t, "MONITOR")
	emptySession, _ := monitorRegisteredSession(t, "net", "tester")
	empty.host.session = emptySession
	empty.coordinator.DispatchMonitor(irc.Command{Verb: irc.VerbMonitored}, irc.SurfaceStatus)
	if got := empty.texts(); len(got) != 1 || got[0] != "Not watching anyone" {
		t.Fatalf("empty /monitored texts = %v", got)
	}
}

func TestMonitorDispatchAdd(t *testing.T) {
	fixture := newMonitorFixture(t, "MONITOR")
	s, transport := monitorRegisteredSession(t, "net", "tester")
	fixture.host.session = s

	outcome := fixture.coordinator.DispatchMonitor(irc.Command{Verb: irc.VerbMonitor, Argument: "one"}, irc.SurfaceStatus)
	if outcome != irc.OutcomeSent {
		t.Fatalf("/monitor outcome = %v, want Sent", outcome)
	}
	if !fixture.monitors.Contains("net", "one", fixture.mapping()) {
		t.Fatalf("/monitor did not store the nick")
	}
	if got := lastWrittenFrame(t, transport); got != "MONITOR + one\r\n" {
		t.Fatalf("/monitor frame = %q", got)
	}
	if got := fixture.texts(); len(got) != 1 || got[0] != "Watching one" {
		t.Fatalf("/monitor texts = %v", got)
	}

	fixture.coordinator.DispatchMonitor(irc.Command{Verb: irc.VerbMonitor, Argument: "one"}, irc.SurfaceStatus)
	if got := fixture.texts(); len(got) != 2 || got[1] != "Already watching one" {
		t.Fatalf("duplicate /monitor texts = %v", got)
	}
}

func TestMonitorDispatchAtLimit(t *testing.T) {
	fixture := newMonitorFixture(t, "MONITOR=2")
	fixture.monitors.Add("net", "one", fixture.mapping())
	fixture.monitors.Add("net", "two", fixture.mapping())
	s, transport := monitorRegisteredSession(t, "net", "tester")
	fixture.host.session = s
	before := len(transport.WrittenFrames())

	outcome := fixture.coordinator.DispatchMonitor(irc.Command{Verb: irc.VerbMonitor, Argument: "three"}, irc.SurfaceStatus)
	if outcome != irc.OutcomeSent {
		t.Fatalf("/monitor at limit outcome = %v, want Sent", outcome)
	}
	if fixture.monitors.Contains("net", "three", fixture.mapping()) {
		t.Fatalf("/monitor at limit stored a third nick")
	}
	if after := len(transport.WrittenFrames()); after != before {
		t.Fatalf("/monitor at limit wrote %d frames, want 0", after-before)
	}
	if got := fixture.texts(); len(got) != 1 || got[0] != "Monitor list is full (2): three." {
		t.Fatalf("/monitor at limit texts = %v", got)
	}
}

func TestMonitorDispatchRollsBackOnSendFailure(t *testing.T) {
	fixture := newMonitorFixture(t, "MONITOR")
	s, _ := monitorRegisteredSession(t, "net", "tester")
	fixture.host.session = s

	// A newline makes irc.Line reject the MONITOR command, so the session's
	// SendMonitor reports failure after the store already accepted the nick.
	outcome := fixture.coordinator.DispatchMonitor(
		irc.Command{Verb: irc.VerbMonitor, Argument: "bad\nnick"}, irc.SurfaceStatus)
	if outcome != irc.OutcomeRefused {
		t.Fatalf("/monitor with a failing send = %v, want Refused", outcome)
	}
	if fixture.monitors.Contains("net", "bad\nnick", fixture.mapping()) {
		t.Fatalf("failed send left the nick in the store")
	}
	if len(fixture.host.entries) != 0 {
		t.Fatalf("failed send recorded %v, want no status", fixture.texts())
	}
}

func TestMonitorDispatchUnmonitor(t *testing.T) {
	fixture := newMonitorFixture(t, "MONITOR")
	fixture.monitors.Add("net", "one", fixture.mapping())
	s, transport := monitorRegisteredSession(t, "net", "tester")
	fixture.host.session = s

	outcome := fixture.coordinator.DispatchMonitor(irc.Command{Verb: irc.VerbUnmonitor, Argument: "one"}, irc.SurfaceStatus)
	if outcome != irc.OutcomeSent {
		t.Fatalf("/unmonitor outcome = %v, want Sent", outcome)
	}
	if fixture.monitors.Contains("net", "one", fixture.mapping()) {
		t.Fatalf("/unmonitor left the nick in the store")
	}
	if got := lastWrittenFrame(t, transport); got != "MONITOR - one\r\n" {
		t.Fatalf("/unmonitor frame = %q", got)
	}
	if got := fixture.texts(); len(got) != 1 || got[0] != "No longer watching one" {
		t.Fatalf("/unmonitor texts = %v", got)
	}

	fixture.coordinator.DispatchMonitor(irc.Command{Verb: irc.VerbUnmonitor, Argument: "one"}, irc.SurfaceStatus)
	if got := fixture.texts(); len(got) != 2 || got[1] != "Not watching one" {
		t.Fatalf("repeat /unmonitor texts = %v", got)
	}
}

func TestMonitorDispatchUnsupportedNetwork(t *testing.T) {
	fixture := newMonitorFixture(t, "")
	s, transport := monitorRegisteredSession(t, "net", "tester")
	fixture.host.session = s
	before := len(transport.WrittenFrames())

	outcome := fixture.coordinator.DispatchMonitor(irc.Command{Verb: irc.VerbMonitor, Argument: "one"}, irc.SurfaceStatus)
	if outcome != irc.OutcomeSent {
		t.Fatalf("/monitor on unsupported network = %v, want Sent", outcome)
	}
	if got := fixture.texts(); len(got) != 1 || got[0] != "This network does not support MONITOR." {
		t.Fatalf("unsupported texts = %v", got)
	}
	if after := len(transport.WrittenFrames()); after != before {
		t.Fatalf("unsupported /monitor wrote %d frames, want 0", after-before)
	}
}

func TestMonitorDispatchScopeAndConnection(t *testing.T) {
	fixture := newMonitorFixture(t, "MONITOR")

	// Nothing resolved for a conversation composer: wrong scope.
	fixture.host.networkID = ""
	if got := fixture.coordinator.DispatchMonitor(
		irc.Command{Verb: irc.VerbMonitored}, irc.SurfaceConversation); got != irc.OutcomeWrongScope {
		t.Fatalf("unselected conversation = %v, want WrongScope", got)
	}
	if got := fixture.coordinator.DispatchMonitor(
		irc.Command{Verb: irc.VerbMonitored}, irc.SurfaceStatus); got != irc.OutcomeRefused {
		t.Fatalf("empty status network = %v, want Refused", got)
	}
	fixture.host.hasSelected = true
	fixture.host.selected = irc.ConversationKey{NetworkID: "net", NormalizedTarget: "#room"}
	if got := fixture.coordinator.DispatchMonitor(
		irc.Command{Verb: irc.VerbMonitored}, irc.SurfaceConversation); got != irc.OutcomeRefused {
		t.Fatalf("selected conversation with no network = %v, want Refused", got)
	}

	// A resolved network without a registered session is not connected.
	fixture.host.networkID = "net"
	if got := fixture.coordinator.DispatchMonitor(
		irc.Command{Verb: irc.VerbMonitored}, irc.SurfaceStatus); got != irc.OutcomeNotConnected {
		t.Fatalf("missing session = %v, want NotConnected", got)
	}
}

func TestMonitorForgetPresenceClearsSubscription(t *testing.T) {
	fixture := newMonitorFixture(t, "MONITOR")
	fixture.monitors.Add("net", "one", fixture.mapping())
	s, transport := monitorRegisteredSession(t, "net", "tester")
	fixture.host.sessions["net"] = s

	fixture.coordinator.SubscribeMonitors("net")
	before := len(transport.WrittenFrames())
	fixture.coordinator.ForgetPresence("net")
	fixture.coordinator.SubscribeMonitors("net")
	if after := len(transport.WrittenFrames()); after != before+1 {
		t.Fatalf("subscribe after forget wrote %d frames, want 1", after-before)
	}
}
