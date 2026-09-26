package controller

import (
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// This file ports the auto-away runtime behavior from
// tests/session/tst_controller.cpp. It drives the runtime with a recording
// AutoawayHost and a FakeClock, and uses real registered sessions on loopback
// transports so AWAY/ClearAway frames are observable.

// autoawayStatus is one recorded RecordStatus call.
type autoawayStatus struct {
	networkID string
	text      string
}

// autoawayFakeHost records every callback the runtime makes.
type autoawayFakeHost struct {
	now              time.Time
	sessions         map[string]*session.Session
	networkIDs       []string
	selfAway         map[string]bool
	unawaySent       []string
	statuses         []autoawayStatus
	queryNetworkID   string
	consoleNetworkID string
	selected         irc.ConversationKey
	hasSelected      bool
	events           []irc.Event
}

func (h *autoawayFakeHost) FindSession(networkID string) *session.Session {
	return h.sessions[networkID]
}

func (h *autoawayFakeHost) NetworkIDs() []string {
	return append([]string(nil), h.networkIDs...)
}

func (h *autoawayFakeHost) SelfAway(networkID string) bool { return h.selfAway[networkID] }

func (h *autoawayFakeHost) MarkUnawaySent(networkID string) {
	h.unawaySent = append(h.unawaySent, networkID)
}

func (h *autoawayFakeHost) RecordStatus(networkID, text string) {
	h.statuses = append(h.statuses, autoawayStatus{networkID, text})
}

func (h *autoawayFakeHost) QueryNetworkID(surface irc.ComposerSurface) string {
	return h.queryNetworkID
}

func (h *autoawayFakeHost) ConsoleNetworkID() string { return h.consoleNetworkID }

func (h *autoawayFakeHost) SelectedKey() (irc.ConversationKey, bool) {
	return h.selected, h.hasSelected
}

func (h *autoawayFakeHost) ApplyEvent(event irc.Event) { h.events = append(h.events, event) }

func (h *autoawayFakeHost) Now() time.Time { return h.now }

// autoawayFixture bundles the runtime, its host, and the driving clock.
type autoawayFixture struct {
	clock   *session.FakeClock
	host    *autoawayFakeHost
	runtime *AutoawayRuntime
}

func newAutoawayFixture(t *testing.T) *autoawayFixture {
	t.Helper()
	start := time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)
	clock := session.NewFakeClock(start)
	host := &autoawayFakeHost{
		now:              start,
		sessions:         map[string]*session.Session{},
		selfAway:         map[string]bool{},
		queryNetworkID:   "net",
		consoleNetworkID: "net",
	}
	runtime := NewAutoawayRuntime(host, clock)
	runtime.LoadStored()
	return &autoawayFixture{clock: clock, host: host, runtime: runtime}
}

// lastStatusText returns the most recent recorded Status text.
func (f *autoawayFixture) lastStatusText(t *testing.T) string {
	t.Helper()
	if len(f.host.statuses) == 0 {
		t.Fatalf("no status recorded")
	}
	return f.host.statuses[len(f.host.statuses)-1].text
}

// enable issues one /autoaway argument on the Status surface.
func (f *autoawayFixture) enable(t *testing.T, argument string) irc.CommandOutcome {
	t.Helper()
	return f.runtime.DispatchAutoaway(irc.Command{Verb: irc.VerbAutoaway, Argument: argument}, irc.SurfaceStatus)
}

// autoawayRegisteredSession builds a real registered session on a loopback
// transport.
func autoawayRegisteredSession(t *testing.T, networkID, nick string) (*session.Session, *session.LoopbackTransport) {
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

func TestAutoawayQueryAndChanges(t *testing.T) {
	fixture := newAutoawayFixture(t)

	if got := fixture.enable(t, ""); got != irc.OutcomeSent {
		t.Fatalf("/autoaway query = %v, want Sent", got)
	}
	if got := fixture.lastStatusText(t); got != "Auto-away off" {
		t.Fatalf("query text = %q, want %q", got, "Auto-away off")
	}

	// A duration turns auto-away on.
	if got := fixture.enable(t, "5m"); got != irc.OutcomeSent {
		t.Fatalf("/autoaway 5m = %v, want Sent", got)
	}
	if got := fixture.lastStatusText(t); got != "Auto-away 5 minutes" {
		t.Fatalf("5m text = %q", got)
	}
	if !fixture.runtime.IdleActiveForTest() {
		t.Fatalf("5m did not arm the idle timer")
	}
	if got := fixture.runtime.IdleIntervalForTest(); got != 300*time.Second {
		t.Fatalf("idle interval = %v, want 5m", got)
	}

	if got := fixture.enable(t, "reason lunch"); got != irc.OutcomeSent {
		t.Fatalf("/autoaway reason lunch = %v, want Sent", got)
	}
	if got := fixture.lastStatusText(t); got != "Auto-away 5 minutes, reason: lunch" {
		t.Fatalf("reason text = %q", got)
	}

	if got := fixture.enable(t, "on"); got != irc.OutcomeSent {
		t.Fatalf("/autoaway on = %v, want Sent", got)
	}
	if got := fixture.lastStatusText(t); got != "Auto-away 5 minutes, reason: lunch" {
		t.Fatalf("on text = %q", got)
	}

	if got := fixture.enable(t, "off"); got != irc.OutcomeSent {
		t.Fatalf("/autoaway off = %v, want Sent", got)
	}
	if got := fixture.lastStatusText(t); got != "Auto-away off, reason: lunch" {
		t.Fatalf("off text = %q", got)
	}
	if fixture.runtime.IdleActiveForTest() {
		t.Fatalf("off left the idle timer armed")
	}

	if got := fixture.enable(t, "reason"); got != irc.OutcomeSent {
		t.Fatalf("/autoaway reason = %v, want Sent", got)
	}
	if got := fixture.lastStatusText(t); got != "Auto-away off" {
		t.Fatalf("clear reason text = %q", got)
	}

	// An unparseable argument prints the verb-table usage line.
	if got := fixture.enable(t, "bogus"); got != irc.OutcomeSent {
		t.Fatalf("/autoaway bogus = %v, want Sent", got)
	}
	usage := irc.FindVerb(irc.VerbAutoaway).Usage
	if got := fixture.lastStatusText(t); got != usage {
		t.Fatalf("usage text = %q, want %q", got, usage)
	}
}

func TestAutoawayEnableWithoutTimeoutShowsUsage(t *testing.T) {
	fixture := newAutoawayFixture(t)
	if got := fixture.enable(t, "on"); got != irc.OutcomeSent {
		t.Fatalf("/autoaway on without timeout = %v, want Sent", got)
	}
	usage := irc.FindVerb(irc.VerbAutoaway).Usage
	if got := fixture.lastStatusText(t); got != usage {
		t.Fatalf("text = %q, want usage %q", got, usage)
	}
}

func TestAutoawayFeedbackRouting(t *testing.T) {
	selected := irc.ConversationKey{NetworkID: "net", NormalizedTarget: "#room"}
	fixture := newAutoawayFixture(t)
	fixture.host.hasSelected = true
	fixture.host.selected = selected

	if got := fixture.runtime.DispatchAutoaway(
		irc.Command{Verb: irc.VerbAutoaway}, irc.SurfaceConversation); got != irc.OutcomeSent {
		t.Fatalf("conversation feedback = %v, want Sent", got)
	}
	if len(fixture.host.events) != 1 {
		t.Fatalf("conversation feedback events = %d, want 1", len(fixture.host.events))
	}
	event, ok := fixture.host.events[0].(irc.WhoisTranscriptEvent)
	if !ok {
		t.Fatalf("event type = %T, want WhoisTranscriptEvent", fixture.host.events[0])
	}
	if event.Destination != selected || event.FormattedBody != "Auto-away off" {
		t.Fatalf("event = %+v", event)
	}
	if len(fixture.host.statuses) != 0 {
		t.Fatalf("conversation feedback also recorded a status: %v", fixture.host.statuses)
	}

	// A conversation composer with nothing selected is the wrong scope.
	fixture.host.hasSelected = false
	if got := fixture.runtime.DispatchAutoaway(
		irc.Command{Verb: irc.VerbAutoaway}, irc.SurfaceConversation); got != irc.OutcomeWrongScope {
		t.Fatalf("unselected conversation = %v, want WrongScope", got)
	}

	// With no network at all the feedback cannot be routed.
	fixture.host.queryNetworkID = ""
	fixture.host.consoleNetworkID = ""
	if got := fixture.runtime.DispatchAutoaway(
		irc.Command{Verb: irc.VerbAutoaway}, irc.SurfaceStatus); got != irc.OutcomeRefused {
		t.Fatalf("no network = %v, want Refused", got)
	}
}

func TestAutoawayIdleGraceTripViaClock(t *testing.T) {
	fixture := newAutoawayFixture(t)
	s, transport := autoawayRegisteredSession(t, "net", "tester")
	fixture.host.sessions["net"] = s
	fixture.host.networkIDs = []string{"net"}

	fixture.enable(t, "5m")
	if !fixture.runtime.IdleActiveForTest() || fixture.runtime.IdleIntervalForTest() != 300*time.Second {
		t.Fatalf("idle timer not armed for 5m")
	}

	// Idle elapses into the grace window (300 / 20 = 15, clamped to 10).
	fixture.clock.Advance(300 * time.Second)
	if fixture.runtime.IdleActiveForTest() {
		t.Fatalf("idle timer still active after firing")
	}
	if !fixture.runtime.GraceActiveForTest() {
		t.Fatalf("grace timer not armed after idle")
	}
	if got := fixture.runtime.GraceIntervalForTest(); got != 10*time.Second {
		t.Fatalf("grace interval = %v, want 10s", got)
	}

	// Grace elapses and auto-away trips.
	fixture.clock.Advance(10 * time.Second)
	if fixture.runtime.GraceActiveForTest() {
		t.Fatalf("grace timer still active after firing")
	}
	if !fixture.runtime.tripped {
		t.Fatalf("auto-away did not trip")
	}
	if _, ok := fixture.runtime.autoAwayNetworks["net"]; !ok {
		t.Fatalf("net was not recorded as auto-away")
	}
	if !writtenFramesContain(transport, "AWAY :") {
		t.Fatalf("registered session was not marked away")
	}
	if got := fixture.lastStatusText(t); got != "Auto-away triggered." {
		t.Fatalf("trip status = %q", got)
	}
}

func TestAutoawayGraceIntervalSeam(t *testing.T) {
	fixture := newAutoawayFixture(t)
	fixture.enable(t, "30s")

	// 30 / 20 = 1, clamped to the 5s floor.
	fixture.runtime.FireIdleForTest()
	if got := fixture.runtime.GraceIntervalForTest(); got != 5*time.Second {
		t.Fatalf("grace interval = %v, want 5s", got)
	}
	if !fixture.runtime.GraceActiveForTest() {
		t.Fatalf("grace timer not armed")
	}

	// Firing idle again while grace is already armed is a no-op.
	fixture.runtime.FireIdleForTest()
	if got := fixture.runtime.GraceIntervalForTest(); got != 5*time.Second {
		t.Fatalf("second idle changed grace interval to %v", got)
	}
}

func TestAutoawayTripSkipsManualAndUnregistered(t *testing.T) {
	fixture := newAutoawayFixture(t)
	autoSession, autoTransport := autoawayRegisteredSession(t, "auto", "a")
	manualSession, manualTransport := autoawayRegisteredSession(t, "manual", "m")
	selfSession, selfTransport := autoawayRegisteredSession(t, "self", "s")
	fixture.host.sessions["auto"] = autoSession
	fixture.host.sessions["manual"] = manualSession
	fixture.host.sessions["self"] = selfSession
	fixture.host.networkIDs = []string{"auto", "manual", "gone", "self"}
	fixture.host.selfAway["self"] = true

	fixture.runtime.NoteManualAway("manual")
	fixture.enable(t, "5m")
	fixture.host.statuses = nil
	fixture.runtime.FireIdleForTest()
	fixture.runtime.FireGraceForTest()

	if !writtenFramesContain(autoTransport, "AWAY :") {
		t.Fatalf("registered network was not marked away")
	}
	if writtenFramesContain(manualTransport, "AWAY :") {
		t.Fatalf("manually away network was marked away")
	}
	if writtenFramesContain(selfTransport, "AWAY :") {
		t.Fatalf("already-away network was marked away")
	}
	if len(fixture.host.statuses) != 1 || fixture.host.statuses[0].networkID != "auto" {
		t.Fatalf("trip statuses = %v, want only auto", fixture.host.statuses)
	}
}

func TestAutoawayNoteLocalActivityClears(t *testing.T) {
	fixture := newAutoawayFixture(t)
	s, transport := autoawayRegisteredSession(t, "net", "tester")
	fixture.host.sessions["net"] = s
	fixture.host.networkIDs = []string{"net"}

	fixture.enable(t, "5m")
	fixture.runtime.FireIdleForTest()
	fixture.runtime.FireGraceForTest()
	if !fixture.runtime.tripped {
		t.Fatalf("auto-away did not trip")
	}

	fixture.host.statuses = nil
	fixture.host.unawaySent = nil
	fixture.runtime.NoteLocalActivity()

	if fixture.runtime.tripped {
		t.Fatalf("local activity left auto-away tripped")
	}
	if len(fixture.runtime.autoAwayNetworks) != 0 {
		t.Fatalf("auto-away set not cleared: %v", fixture.runtime.autoAwayNetworks)
	}
	if !writtenFramesContain(transport, "AWAY\r\n") {
		t.Fatalf("local activity did not clear away")
	}
	if len(fixture.host.unawaySent) != 1 || fixture.host.unawaySent[0] != "net" {
		t.Fatalf("unaway bookkeeping = %v, want [net]", fixture.host.unawaySent)
	}
	if got := fixture.lastStatusText(t); got != "Auto-away cleared — back online." {
		t.Fatalf("cleared status = %q", got)
	}
	if !fixture.runtime.IdleActiveForTest() {
		t.Fatalf("local activity did not re-arm the idle timer")
	}
	if got := fixture.runtime.IdleIntervalForTest(); got != 300*time.Second {
		t.Fatalf("re-armed idle interval = %v, want 5m", got)
	}
}

func TestAutoawayRefreshReasonWhileTripped(t *testing.T) {
	fixture := newAutoawayFixture(t)
	s, transport := autoawayRegisteredSession(t, "net", "tester")
	fixture.host.sessions["net"] = s
	fixture.host.networkIDs = []string{"net"}

	fixture.enable(t, "5m")
	fixture.runtime.FireIdleForTest()
	fixture.runtime.FireGraceForTest()

	// A new one-shot reason while tripped re-sends AWAY with it.
	fixture.enable(t, "10m lunch")
	if !writtenFramesContain(transport, "AWAY :lunch\r\n") {
		t.Fatalf("tripped reason change did not refresh AWAY")
	}
}

func TestAutoawayOnSessionRegisteredMarksAway(t *testing.T) {
	fixture := newAutoawayFixture(t)

	// Trip with no networks configured.
	fixture.enable(t, "5m")
	fixture.runtime.FireIdleForTest()
	fixture.runtime.FireGraceForTest()
	if !fixture.runtime.tripped {
		t.Fatalf("auto-away did not trip")
	}

	s, transport := autoawayRegisteredSession(t, "net", "tester")
	fixture.runtime.OnSessionRegistered(s)
	if !writtenFramesContain(transport, "AWAY :") {
		t.Fatalf("newly registered session was not marked away")
	}
	if _, ok := fixture.runtime.autoAwayNetworks["net"]; !ok {
		t.Fatalf("newly registered session was not recorded as auto-away")
	}
}

func TestAutoawayNoteAwayClearedClearsOneShotReason(t *testing.T) {
	fixture := newAutoawayFixture(t)
	s, _ := autoawayRegisteredSession(t, "net", "tester")
	fixture.host.sessions["net"] = s
	fixture.host.networkIDs = []string{"net"}

	fixture.enable(t, "5m lunch")
	fixture.runtime.FireIdleForTest()
	fixture.runtime.FireGraceForTest()
	if fixture.runtime.config.OneShotReason != "lunch" {
		t.Fatalf("one-shot reason = %q", fixture.runtime.config.OneShotReason)
	}

	fixture.runtime.NoteAwayCleared("net")
	if _, ok := fixture.runtime.autoAwayNetworks["net"]; ok {
		t.Fatalf("NoteAwayCleared left the network in the auto set")
	}
	if fixture.runtime.config.OneShotReason != "" {
		t.Fatalf("NoteAwayCleared left one-shot reason = %q", fixture.runtime.config.OneShotReason)
	}
}

func TestAutoawayForgetNetworkDropsAwaySets(t *testing.T) {
	fixture := newAutoawayFixture(t)
	s, _ := autoawayRegisteredSession(t, "net", "tester")
	fixture.host.sessions["net"] = s
	fixture.host.networkIDs = []string{"net"}

	fixture.enable(t, "5m")
	fixture.runtime.FireIdleForTest()
	fixture.runtime.FireGraceForTest()
	fixture.runtime.NoteManualAway("net")

	fixture.runtime.ForgetNetwork("net")
	if len(fixture.runtime.autoAwayNetworks) != 0 || len(fixture.runtime.manualAwayNetworks) != 0 {
		t.Fatalf("ForgetNetwork left sets: auto=%v manual=%v",
			fixture.runtime.autoAwayNetworks, fixture.runtime.manualAwayNetworks)
	}
}

func TestAutoawaySetEphemeralIsStored(t *testing.T) {
	fixture := newAutoawayFixture(t)
	fixture.runtime.SetEphemeral(true)
	if !fixture.runtime.ephemeral {
		t.Fatalf("SetEphemeral did not store the flag")
	}
	// Saving is a no-op this phase, so a change still updates the live config.
	fixture.enable(t, "5m")
	if !fixture.runtime.config.Enabled {
		t.Fatalf("ephemeral runtime did not apply the change")
	}
}
