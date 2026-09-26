package controller

import (
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// commandFakeHost records every CommandHost callback and returns canned
// values. Session-returning methods hand back real *session.Session values the
// test builds with commandRegisteredSession; everything else is inert.
type commandFakeHost struct {
	networkID       string
	statusNetworkID string
	hasNetworks     bool

	sessionForSurface  *session.Session
	sessionByNetwork   map[string]*session.Session
	selectedSession    *session.Session
	selectedTarget     string
	selectedIsChannel  bool
	selectedCloseable  bool
	selectedKey        irc.ConversationKey
	hasSelectedKey     bool
	sendSelectedResult irc.CommandOutcome
	dismissResult      bool
	clearSurfaceResult irc.CommandOutcome
	delegatedResult    irc.CommandOutcome

	mutes   *MuteStore
	mapping irc.CaseMapping
	prefs   map[irc.PrefName]bool
	prefSet map[irc.PrefName]bool

	now time.Time

	delegated          []string
	echoLocalKind      irc.MessageKind
	echoLocalBody      string
	echoLocalCalls     int
	openChannel        string
	openChannelCalls   int
	dismissCalls       int
	dropCalls          int
	clearSurfaces      []irc.ComposerSurface
	remembered         []string
	nickDeliveries     []string
	localActivityCalls int
	unawayCalls        []*session.Session
	manualAway         []string
	awayCleared        []string
	clearTypingCalls   int
	sentMessages       []string
	echoPresent        []string
	echoPresentWires   []QuietWire
	muteCalls          []commandMuteCall
	syncHighlights     []string
	reloadCalls        int
	selectedConvs      []string
	whoisEvents        []irc.WhoisTranscriptEvent
	statusTexts        []string
}

type commandMuteCall struct {
	networkID string
	target    string
	muted     bool
}

var _ CommandHost = (*commandFakeHost)(nil)

func commandPair(left, right string) string { return left + "\x00" + right }

func (h *commandFakeHost) QueryNetworkID(irc.ComposerSurface) string { return h.networkID }

func (h *commandFakeHost) SessionFor(irc.ComposerSurface) *session.Session {
	return h.sessionForSurface
}

func (h *commandFakeHost) SessionForNetwork(networkID string) *session.Session {
	return h.sessionByNetwork[networkID]
}

func (h *commandFakeHost) SelectedSession() *session.Session { return h.selectedSession }

func (h *commandFakeHost) SelectedTarget() string { return h.selectedTarget }

func (h *commandFakeHost) SelectedIsChannel() bool { return h.selectedIsChannel }

func (h *commandFakeHost) SelectedIsCloseableDirect() bool { return h.selectedCloseable }

func (h *commandFakeHost) SelectedKey() (irc.ConversationKey, bool) {
	return h.selectedKey, h.hasSelectedKey
}

func (h *commandFakeHost) HasNetworks() bool { return h.hasNetworks }

func (h *commandFakeHost) StatusNetworkID() string { return h.statusNetworkID }

func (h *commandFakeHost) EchoLocal(kind irc.MessageKind, body string) {
	h.echoLocalKind = kind
	h.echoLocalBody = body
	h.echoLocalCalls++
}

func (h *commandFakeHost) OpenJoinedChannel(networkID, channel string) {
	h.openChannel = channel
	h.openChannelCalls++
}

func (h *commandFakeHost) DismissChannel(networkID, channel string) bool {
	h.dismissCalls++
	return h.dismissResult
}

func (h *commandFakeHost) DropSelectedDirectAndReselect() { h.dropCalls++ }

func (h *commandFakeHost) ClearSurface(surface irc.ComposerSurface) irc.CommandOutcome {
	h.clearSurfaces = append(h.clearSurfaces, surface)
	return h.clearSurfaceResult
}

func (h *commandFakeHost) RememberOpenDirect(networkID, target string) {
	h.remembered = append(h.remembered, commandPair(networkID, target))
}

func (h *commandFakeHost) NoteNickDelivery(networkID, target string) {
	h.nickDeliveries = append(h.nickDeliveries, commandPair(networkID, target))
}

func (h *commandFakeHost) NoteLocalActivity() { h.localActivityCalls++ }

func (h *commandFakeHost) UnawayAfterChat(s *session.Session) {
	h.unawayCalls = append(h.unawayCalls, s)
}

func (h *commandFakeHost) NoteManualAway(networkID string) {
	h.manualAway = append(h.manualAway, networkID)
}

func (h *commandFakeHost) NoteAwayCleared(networkID string) {
	h.awayCleared = append(h.awayCleared, networkID)
}

func (h *commandFakeHost) ClearTypingTarget() { h.clearTypingCalls++ }

func (h *commandFakeHost) SendSelectedMessage(body string) irc.CommandOutcome {
	h.sentMessages = append(h.sentMessages, body)
	return h.sendSelectedResult
}

func (h *commandFakeHost) EchoIfPresent(s *session.Session, target, body string, wire QuietWire) {
	h.echoPresent = append(h.echoPresent, commandPair(target, body))
	h.echoPresentWires = append(h.echoPresentWires, wire)
}

func (h *commandFakeHost) ApplyMute(networkID, target string, muted bool) bool {
	h.muteCalls = append(h.muteCalls, commandMuteCall{networkID: networkID, target: target, muted: muted})
	if muted {
		return h.mutes.Add(networkID, target, h.mapping)
	}
	return h.mutes.Remove(networkID, target, h.mapping)
}

func (h *commandFakeHost) SyncHighlightWords(networkID string) {
	h.syncHighlights = append(h.syncHighlights, networkID)
}

func (h *commandFakeHost) ReloadConversations() { h.reloadCalls++ }

func (h *commandFakeHost) SelectConversation(networkID, target string) {
	h.selectedConvs = append(h.selectedConvs, commandPair(networkID, target))
}

func (h *commandFakeHost) ApplyWhoisTranscript(event irc.WhoisTranscriptEvent) {
	h.whoisEvents = append(h.whoisEvents, event)
}

func (h *commandFakeHost) RecordStatus(entry irc.StatusEntry) {
	h.statusTexts = append(h.statusTexts, entry.Text())
}

func (h *commandFakeHost) delegatedOutcome(name string, result irc.CommandOutcome) irc.CommandOutcome {
	h.delegated = append(h.delegated, name)
	return result
}

func (h *commandFakeHost) DispatchList(irc.Command, irc.ComposerSurface) irc.CommandOutcome {
	return h.delegatedOutcome("list", h.delegatedResult)
}

func (h *commandFakeHost) DispatchAutoaway(irc.Command, irc.ComposerSurface) irc.CommandOutcome {
	return h.delegatedOutcome("autoaway", h.delegatedResult)
}

func (h *commandFakeHost) DispatchWhois(irc.Command, irc.ComposerSurface) irc.CommandOutcome {
	return h.delegatedOutcome("whois", h.delegatedResult)
}

func (h *commandFakeHost) DispatchCtcp(irc.Command, irc.ComposerSurface) irc.CommandOutcome {
	return h.delegatedOutcome("ctcp", h.delegatedResult)
}

func (h *commandFakeHost) DispatchStatus(irc.Command, irc.ComposerSurface) irc.CommandOutcome {
	return h.delegatedOutcome("status", h.delegatedResult)
}

func (h *commandFakeHost) DispatchAvatar(irc.Command, irc.ComposerSurface) irc.CommandOutcome {
	return h.delegatedOutcome("avatar", h.delegatedResult)
}

func (h *commandFakeHost) DispatchMonitor(irc.Command, irc.ComposerSurface) irc.CommandOutcome {
	return h.delegatedOutcome("monitor", h.delegatedResult)
}

func (h *commandFakeHost) PrefEnabled(name irc.PrefName) bool {
	if h.prefSet[name] {
		return h.prefs[name]
	}
	return true
}

func (h *commandFakeHost) PrefApply(name irc.PrefName, enabled bool) {
	h.prefs[name] = enabled
	h.prefSet[name] = true
}

func (h *commandFakeHost) Now() time.Time { return h.now }

// commandFixture bundles a dispatcher over a real reducer and real stores, plus
// the recording host.
type commandFixture struct {
	dispatcher *CommandDispatcher
	host       *commandFakeHost
	reducer    *irc.EventReducer
	ignores    *IgnoreStore
	mutes      *MuteStore
	highlights *HighlightStore
}

func newCommandFixture() *commandFixture {
	reducer := irc.NewEventReducer()
	ignores := NewIgnoreStore()
	mutes := NewMuteStore()
	highlights := NewHighlightStore()
	features := irc.NewServerFeatures()
	host := &commandFakeHost{
		networkID:          "libera",
		statusNetworkID:    "libera",
		delegatedResult:    irc.OutcomeSent,
		sendSelectedResult: irc.OutcomeSent,
		clearSurfaceResult: irc.OutcomeSent,
		mutes:              mutes,
		mapping:            features.CaseMapping(),
		prefs:              map[irc.PrefName]bool{},
		prefSet:            map[irc.PrefName]bool{},
		sessionByNetwork:   map[string]*session.Session{},
		now:                time.Unix(1000, 0).UTC(),
	}
	return &commandFixture{
		dispatcher: NewCommandDispatcher(reducer, ignores, mutes, highlights, host),
		host:       host,
		reducer:    reducer,
		ignores:    ignores,
		mutes:      mutes,
		highlights: highlights,
	}
}

// commandRegisteredSession builds a real session driven to Registered over a
// loopback transport, so wire-path assertions see actual frames.
func commandRegisteredSession(t *testing.T, networkID string) (*session.Session, *session.LoopbackTransport) {
	t.Helper()
	transport := session.NewLoopbackTransport()
	config := session.DefaultSessionConfig(networkID, networkID, "irc.example.test", "me")
	clock := session.NewFakeClock(time.Unix(0, 0))
	active := session.NewSession(config, transport, clock)
	active.Start()
	transport.CompleteConnect()
	transport.InjectBytes([]byte(":server CAP me LS :multi-prefix\r\n:server 001 me :Welcome\r\n"))
	if state := active.State(); state != session.StateRegistered {
		t.Fatalf("session state = %v, want Registered", state)
	}
	return active, transport
}

func commandFrames(transport *session.LoopbackTransport) []string {
	raw := transport.WrittenFrames()
	frames := make([]string, len(raw))
	for index, frame := range raw {
		frames[index] = string(frame)
	}
	return frames
}

func commandWrote(t *testing.T, transport *session.LoopbackTransport, frame string) {
	t.Helper()
	for _, candidate := range commandFrames(transport) {
		if candidate == frame {
			return
		}
	}
	t.Fatalf("did not write %q; wrote %q", frame, commandFrames(transport))
}

func commandRequireOutcome(t *testing.T, got, want irc.CommandOutcome, label string) {
	t.Helper()
	if got != want {
		t.Fatalf("%s outcome = %v, want %v", label, got, want)
	}
}

func commandRequireText(t *testing.T, got, want, label string) {
	t.Helper()
	if got != want {
		t.Fatalf("%s = %q, want %q", label, got, want)
	}
}

func TestCommandDispatcherEmptyUnknownAndScope(t *testing.T) {
	fixture := newCommandFixture()
	dispatcher := fixture.dispatcher

	commandRequireOutcome(t, dispatcher.Dispatch(irc.Command{Verb: irc.VerbEmpty}, irc.SurfaceConversation), irc.OutcomeSent, "empty")
	commandRequireOutcome(t, dispatcher.Dispatch(irc.Command{Verb: irc.VerbUnknown}, irc.SurfaceConversation), irc.OutcomeUnsupported, "unknown")
	commandRequireOutcome(t, dispatcher.Dispatch(irc.Command{Verb: irc.VerbAction}, irc.SurfaceStatus), irc.OutcomeWrongScope, "action on status")
	commandRequireOutcome(t, dispatcher.Dispatch(irc.Command{Verb: irc.VerbClose}, irc.SurfaceStatus), irc.OutcomeWrongScope, "close on status")
}

func TestCommandDispatcherSay(t *testing.T) {
	fixture := newCommandFixture()
	fixture.host.sendSelectedResult = irc.OutcomeSent
	outcome := fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbSay, Argument: "hello world"}, irc.SurfaceConversation)
	commandRequireOutcome(t, outcome, irc.OutcomeSent, "say")
	if len(fixture.host.sentMessages) != 1 || fixture.host.sentMessages[0] != "hello world" {
		t.Fatalf("sent messages = %q, want [hello world]", fixture.host.sentMessages)
	}
}

func TestCommandDispatcherClose(t *testing.T) {
	fixture := newCommandFixture()
	fixture.host.selectedIsChannel = true
	fixture.host.hasSelectedKey = true
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbClose}, irc.SurfaceConversation), irc.OutcomeWrongScope, "close channel")
	if fixture.host.dropCalls != 0 {
		t.Fatalf("dropCalls = %d, want 0", fixture.host.dropCalls)
	}

	fixture.host.selectedIsChannel = false
	fixture.host.selectedCloseable = true
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbClose}, irc.SurfaceConversation), irc.OutcomeSent, "close direct")
	if fixture.host.dropCalls != 1 {
		t.Fatalf("dropCalls = %d, want 1", fixture.host.dropCalls)
	}
}

func TestCommandDispatcherQuery(t *testing.T) {
	fixture := newCommandFixture()
	outcome := fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbQuery, Argument: "Lena"}, irc.SurfaceConversation)
	commandRequireOutcome(t, outcome, irc.OutcomeSent, "query")

	key := fixture.reducer.ConversationKey("libera", "Lena")
	inserted := fixture.reducer.Find(key)
	if inserted == nil || inserted.Target != "Lena" {
		t.Fatalf("conversation not inserted for %v", key)
	}
	if len(fixture.host.remembered) != 1 || fixture.host.remembered[0] != commandPair("libera", "Lena") {
		t.Fatalf("remembered = %q", fixture.host.remembered)
	}
	if len(fixture.host.selectedConvs) != 1 || fixture.host.selectedConvs[0] != commandPair("libera", "Lena") {
		t.Fatalf("selectConversation = %q", fixture.host.selectedConvs)
	}

	// Text with a registered session sends into the opened conversation.
	active, _ := commandRegisteredSession(t, "libera")
	fixture.host.sessionByNetwork["libera"] = active
	outcome = fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbQuery, Argument: "lena hello"}, irc.SurfaceConversation)
	commandRequireOutcome(t, outcome, irc.OutcomeSent, "query with text")
	if len(fixture.host.sentMessages) != 1 || fixture.host.sentMessages[0] != "hello" {
		t.Fatalf("query text = %q, want [hello]", fixture.host.sentMessages)
	}

	// Text without a session is NotConnected.
	fixture.host.sessionByNetwork = map[string]*session.Session{}
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbQuery, Argument: "lena hi"}, irc.SurfaceConversation), irc.OutcomeNotConnected, "query text no session")

	// A channel target is refused, and an empty network id splits by surface.
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbQuery, Argument: "#chan"}, irc.SurfaceConversation), irc.OutcomeRefused, "query channel")
	fixture.host.networkID = ""
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbQuery, Argument: "lena"}, irc.SurfaceConversation), irc.OutcomeWrongScope, "query no network conversation")
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbQuery, Argument: "lena"}, irc.SurfaceStatus), irc.OutcomeRefused, "query no network status")
}

func TestCommandDispatcherQuietSend(t *testing.T) {
	fixture := newCommandFixture()
	active, transport := commandRegisteredSession(t, "libera")
	fixture.host.sessionByNetwork["libera"] = active

	outcome := fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbMsg, Argument: "lena hello there"}, irc.SurfaceConversation)
	commandRequireOutcome(t, outcome, irc.OutcomeSent, "msg")
	commandWrote(t, transport, "PRIVMSG lena :hello there\r\n")
	if len(fixture.host.nickDeliveries) != 1 || fixture.host.nickDeliveries[0] != commandPair("libera", "lena") {
		t.Fatalf("nick deliveries = %q", fixture.host.nickDeliveries)
	}
	if len(fixture.host.echoPresentWires) != 1 || fixture.host.echoPresentWires[0] != QuietWirePrivmsg {
		t.Fatalf("echo wires = %v", fixture.host.echoPresentWires)
	}
	if fixture.host.localActivityCalls != 1 || len(fixture.host.unawayCalls) != 1 {
		t.Fatalf("msg activity = %d/%d", fixture.host.localActivityCalls, len(fixture.host.unawayCalls))
	}

	outcome = fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbNotice, Argument: "lena hi"}, irc.SurfaceConversation)
	commandRequireOutcome(t, outcome, irc.OutcomeSent, "notice")
	commandWrote(t, transport, "NOTICE lena :hi\r\n")
	if fixture.host.echoPresentWires[len(fixture.host.echoPresentWires)-1] != QuietWireNotice {
		t.Fatalf("notice wire = %v", fixture.host.echoPresentWires)
	}
	if fixture.host.localActivityCalls != 1 {
		t.Fatalf("notice counted local activity")
	}

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbMsg, Argument: "#chan hi"}, irc.SurfaceConversation), irc.OutcomeRefused, "msg channel")
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbMsg, Argument: "lena"}, irc.SurfaceConversation), irc.OutcomeRefused, "msg no body")
}

func TestCommandDispatcherServiceMsg(t *testing.T) {
	fixture := newCommandFixture()
	active, transport := commandRegisteredSession(t, "libera")
	fixture.host.sessionByNetwork["libera"] = active

	cases := []struct {
		verb irc.Verb
		want string
	}{
		{irc.VerbNs, "PRIVMSG NickServ :identify hunter2\r\n"},
		{irc.VerbCs, "PRIVMSG ChanServ :op\r\n"},
		{irc.VerbZnc, "PRIVMSG *status :listclients\r\n"},
	}
	arguments := map[irc.Verb]string{
		irc.VerbNs:  "identify hunter2",
		irc.VerbCs:  "op",
		irc.VerbZnc: "listclients",
	}
	for _, testCase := range cases {
		outcome := fixture.dispatcher.Dispatch(irc.Command{Verb: testCase.verb, Argument: arguments[testCase.verb]}, irc.SurfaceConversation)
		commandRequireOutcome(t, outcome, irc.OutcomeSent, "service msg")
		commandWrote(t, transport, testCase.want)
	}
}

func TestCommandDispatcherJoin(t *testing.T) {
	fixture := newCommandFixture()
	active, transport := commandRegisteredSession(t, "libera")
	fixture.host.sessionForSurface = active

	cancelledKey := fixture.reducer.ConversationKey("libera", "#help")
	fixture.dispatcher.NoteCancelled(cancelledKey)
	outcome := fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbJoin, Argument: "#help"}, irc.SurfaceConversation)
	commandRequireOutcome(t, outcome, irc.OutcomeSent, "join")
	commandWrote(t, transport, "JOIN #help\r\n")
	commandRequireText(t, fixture.host.openChannel, "#help", "open channel")
	if fixture.dispatcher.TakeCancelledSelfJoin(cancelledKey) {
		t.Fatalf("successful join did not erase the pending cancellation")
	}

	// No argument and no invite is refused.
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbJoin}, irc.SurfaceConversation), irc.OutcomeRefused, "join no invite")

	// A pending INVITE seeds the empty /join.
	transport.InjectBytes([]byte(":inviter INVITE me :#invited\r\n"))
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbJoin}, irc.SurfaceConversation), irc.OutcomeSent, "join invite")
	commandWrote(t, transport, "JOIN #invited\r\n")
}

func TestCommandDispatcherPart(t *testing.T) {
	fixture := newCommandFixture()
	active, transport := commandRegisteredSession(t, "libera")
	fixture.host.sessionForSurface = active

	// An unjoined channel dismissed from the sidebar records a cancellation and
	// never writes PART.
	channelKey := fixture.reducer.ConversationKey("libera", "#room")
	fixture.reducer.EnsureConversation(channelKey, "#room", irc.CauseChannelState)
	fixture.host.dismissResult = true
	outcome := fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbPart, Argument: "#room"}, irc.SurfaceConversation)
	commandRequireOutcome(t, outcome, irc.OutcomeSent, "part unjoined")
	if fixture.host.dismissCalls != 1 {
		t.Fatalf("dismiss calls = %d, want 1", fixture.host.dismissCalls)
	}
	if !fixture.dispatcher.TakeCancelledSelfJoin(channelKey) {
		t.Fatalf("unjoined dismiss did not record a cancellation")
	}

	// A channel the host does not dismiss is parted on the wire.
	fixture.host.dismissResult = false
	outcome = fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbPart, Argument: "#other"}, irc.SurfaceConversation)
	commandRequireOutcome(t, outcome, irc.OutcomeSent, "part wire")
	commandWrote(t, transport, "PART #other\r\n")

	// An empty argument with no selected channel is the wrong scope.
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbPart}, irc.SurfaceConversation), irc.OutcomeWrongScope, "part no selection")
}

func TestCommandDispatcherKickInvite(t *testing.T) {
	fixture := newCommandFixture()
	active, transport := commandRegisteredSession(t, "libera")
	fixture.host.sessionForSurface = active

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbKick, Argument: "#room bob spam"}, irc.SurfaceConversation), irc.OutcomeSent, "kick")
	commandWrote(t, transport, "KICK #room bob :spam\r\n")

	fixture.host.selectedKey = fixture.reducer.ConversationKey("libera", "#room")
	fixture.host.hasSelectedKey = true
	fixture.host.selectedIsChannel = true
	fixture.host.selectedTarget = "#room"
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbKick, Argument: "bob"}, irc.SurfaceConversation), irc.OutcomeSent, "kick selected")
	commandWrote(t, transport, "KICK #room bob\r\n")

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbInvite, Argument: "bob #room"}, irc.SurfaceConversation), irc.OutcomeSent, "invite")
	commandWrote(t, transport, "INVITE bob #room\r\n")
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbInvite, Argument: "bob"}, irc.SurfaceConversation), irc.OutcomeSent, "invite selected")
	commandWrote(t, transport, "INVITE bob #room\r\n")
}

func TestCommandDispatcherMode(t *testing.T) {
	fixture := newCommandFixture()
	active, transport := commandRegisteredSession(t, "libera")
	fixture.host.sessionByNetwork["libera"] = active

	fixture.host.selectedKey = fixture.reducer.ConversationKey("libera", "#room")
	fixture.host.hasSelectedKey = true
	fixture.host.selectedIsChannel = true
	fixture.host.selectedTarget = "#room"

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbMode, Argument: "#room +o bob"}, irc.SurfaceConversation), irc.OutcomeSent, "mode")
	commandWrote(t, transport, "MODE #room +o bob\r\n")

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbOp, Argument: "bob"}, irc.SurfaceConversation), irc.OutcomeSent, "op")
	commandWrote(t, transport, "MODE #room +o bob\r\n")

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbBan, Argument: "bob"}, irc.SurfaceConversation), irc.OutcomeSent, "ban")
	commandWrote(t, transport, "MODE #room +b bob!*@*\r\n")

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbBan, Argument: "bob!*@*"}, irc.SurfaceConversation), irc.OutcomeSent, "ban mask")
	commandWrote(t, transport, "MODE #room +b bob!*@*\r\n")

	// A wrapper outside a channel is the wrong scope.
	fixture.host.selectedIsChannel = false
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbOp, Argument: "bob"}, irc.SurfaceConversation), irc.OutcomeWrongScope, "op outside channel")
}

func TestCommandDispatcherTopicAwayBack(t *testing.T) {
	fixture := newCommandFixture()
	active, transport := commandRegisteredSession(t, "libera")
	fixture.host.sessionForSurface = active
	fixture.host.selectedSession = active
	fixture.host.selectedKey = fixture.reducer.ConversationKey("libera", "#room")
	fixture.host.hasSelectedKey = true
	fixture.host.selectedIsChannel = true
	fixture.host.selectedTarget = "#room"

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbTopic, Argument: "hello world"}, irc.SurfaceConversation), irc.OutcomeSent, "topic")
	commandWrote(t, transport, "TOPIC #room :hello world\r\n")
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbTopic}, irc.SurfaceConversation), irc.OutcomeSent, "topic empty")

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbAway, Argument: "brb"}, irc.SurfaceConversation), irc.OutcomeSent, "away")
	commandWrote(t, transport, "AWAY :brb\r\n")
	if len(fixture.host.manualAway) != 1 || fixture.host.manualAway[0] != "libera" {
		t.Fatalf("manual away = %q", fixture.host.manualAway)
	}
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbAway}, irc.SurfaceConversation), irc.OutcomeSent, "away clear")
	commandWrote(t, transport, "AWAY\r\n")
	if len(fixture.host.awayCleared) != 1 {
		t.Fatalf("away cleared = %q", fixture.host.awayCleared)
	}
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbBack}, irc.SurfaceConversation), irc.OutcomeSent, "back")
	if len(fixture.host.awayCleared) != 2 {
		t.Fatalf("away cleared after back = %q", fixture.host.awayCleared)
	}
}

func TestCommandDispatcherRawListNick(t *testing.T) {
	fixture := newCommandFixture()
	active, transport := commandRegisteredSession(t, "libera")
	fixture.host.sessionForSurface = active
	fixture.host.sessionByNetwork["libera"] = active

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbRaw, Argument: "PRIVMSG #room :hi"}, irc.SurfaceConversation), irc.OutcomeSent, "raw")
	commandWrote(t, transport, "PRIVMSG #room :hi\r\n")

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbNick, Argument: "newnick"}, irc.SurfaceConversation), irc.OutcomeSent, "nick")
	commandWrote(t, transport, "NICK newnick\r\n")

	// NotConnected when the network has no session.
	fixture.host.sessionForSurface = nil
	fixture.host.sessionByNetwork = map[string]*session.Session{}
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbRaw, Argument: "LIST"}, irc.SurfaceConversation), irc.OutcomeNotConnected, "raw no session")
}

func TestCommandDispatcherIgnore(t *testing.T) {
	fixture := newCommandFixture()
	active, _ := commandRegisteredSession(t, "libera")
	fixture.host.sessionForSurface = active

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbIgnored}, irc.SurfaceConversation), irc.OutcomeSent, "ignored empty")
	commandRequireText(t, fixture.host.statusTexts[0], "Not ignoring anyone", "ignored empty text")

	fixture.host.statusTexts = nil
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbIgnore, Argument: "Lena"}, irc.SurfaceConversation), irc.OutcomeSent, "ignore")
	commandRequireText(t, fixture.host.statusTexts[0], "Ignoring Lena", "ignore text")
	if !fixture.ignores.Contains("libera", "lena", fixture.host.mapping) {
		t.Fatalf("ignore store missing lena")
	}

	fixture.host.statusTexts = nil
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbIgnore, Argument: "lena"}, irc.SurfaceConversation), irc.OutcomeSent, "ignore again")
	commandRequireText(t, fixture.host.statusTexts[0], "Already ignoring lena", "ignore again text")

	fixture.host.statusTexts = nil
	fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbIgnored}, irc.SurfaceConversation)
	commandRequireText(t, fixture.host.statusTexts[0], "Ignoring: Lena", "ignored listed text")

	fixture.host.statusTexts = nil
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbUnignore, Argument: "LENA"}, irc.SurfaceConversation), irc.OutcomeSent, "unignore")
	commandRequireText(t, fixture.host.statusTexts[0], "No longer ignoring LENA", "unignore text")

	fixture.host.statusTexts = nil
	fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbUnignore, Argument: "LENA"}, irc.SurfaceConversation)
	commandRequireText(t, fixture.host.statusTexts[0], "Not ignoring LENA", "unignore again text")

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbIgnore, Argument: "#chan"}, irc.SurfaceConversation), irc.OutcomeRefused, "ignore channel")
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbIgnore, Argument: "bad!nick"}, irc.SurfaceConversation), irc.OutcomeRefused, "ignore mask")
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbIgnored, Argument: "extra"}, irc.SurfaceConversation), irc.OutcomeRefused, "ignored extra")
}

func TestCommandDispatcherMute(t *testing.T) {
	fixture := newCommandFixture()
	active, _ := commandRegisteredSession(t, "libera")
	fixture.host.sessionForSurface = active
	fixture.host.selectedKey = fixture.reducer.ConversationKey("libera", "#room")
	fixture.host.hasSelectedKey = true
	fixture.host.selectedTarget = "#room"

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbMuted}, irc.SurfaceConversation), irc.OutcomeSent, "muted empty")
	commandRequireText(t, fixture.host.statusTexts[0], "Not muting anything", "muted empty text")

	fixture.host.statusTexts = nil
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbMute, Argument: "#room"}, irc.SurfaceConversation), irc.OutcomeSent, "mute")
	commandRequireText(t, fixture.host.statusTexts[0], "Muted #room", "mute text")

	fixture.host.statusTexts = nil
	fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbMute, Argument: "#room"}, irc.SurfaceConversation)
	commandRequireText(t, fixture.host.statusTexts[0], "Already muted #room", "mute again text")

	fixture.host.statusTexts = nil
	fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbMuted}, irc.SurfaceConversation)
	commandRequireText(t, fixture.host.statusTexts[0], "Muted: #room", "muted listed text")

	fixture.host.statusTexts = nil
	fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbUnmute, Argument: "#room"}, irc.SurfaceConversation)
	commandRequireText(t, fixture.host.statusTexts[0], "No longer muted #room", "unmute text")

	fixture.host.statusTexts = nil
	fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbUnmute, Argument: "#room"}, irc.SurfaceConversation)
	commandRequireText(t, fixture.host.statusTexts[0], "Not muted #room", "unmute again text")

	// An empty argument mutes the selected target.
	fixture.host.statusTexts = nil
	fixture.host.selectedTarget = "bob"
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbMute}, irc.SurfaceConversation), irc.OutcomeSent, "mute selected")
	commandRequireText(t, fixture.host.statusTexts[0], "Muted bob", "mute selected text")

	if len(fixture.host.muteCalls) == 0 || fixture.host.muteCalls[0].muted != true {
		t.Fatalf("apply mute calls = %+v", fixture.host.muteCalls)
	}
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbMute, Argument: "#room bob"}, irc.SurfaceConversation), irc.OutcomeRefused, "mute extra")
}

func TestCommandDispatcherHighlight(t *testing.T) {
	fixture := newCommandFixture()
	active, _ := commandRegisteredSession(t, "libera")
	fixture.host.sessionForSurface = active

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbHighlights}, irc.SurfaceConversation), irc.OutcomeSent, "highlights empty")
	commandRequireText(t, fixture.host.statusTexts[0], "No highlight words", "highlights empty text")

	fixture.host.statusTexts = nil
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbHighlight, Argument: "Alice"}, irc.SurfaceConversation), irc.OutcomeSent, "highlight")
	commandRequireText(t, fixture.host.statusTexts[0], "Highlighting Alice", "highlight text")
	if len(fixture.host.syncHighlights) != 1 || fixture.host.syncHighlights[0] != "libera" {
		t.Fatalf("sync highlights = %q", fixture.host.syncHighlights)
	}

	fixture.host.statusTexts = nil
	fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbHighlight, Argument: "alice"}, irc.SurfaceConversation)
	commandRequireText(t, fixture.host.statusTexts[0], "Already highlighting alice", "highlight again text")

	fixture.host.statusTexts = nil
	fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbHighlights}, irc.SurfaceConversation)
	commandRequireText(t, fixture.host.statusTexts[0], "Highlights: Alice", "highlights listed text")

	fixture.host.statusTexts = nil
	fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbUnhighlight, Argument: "alice"}, irc.SurfaceConversation)
	commandRequireText(t, fixture.host.statusTexts[0], "No longer highlighting alice", "unhighlight text")

	fixture.host.statusTexts = nil
	fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbUnhighlight, Argument: "alice"}, irc.SurfaceConversation)
	commandRequireText(t, fixture.host.statusTexts[0], "Not highlighting alice", "unhighlight again text")

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbHighlight}, irc.SurfaceConversation), irc.OutcomeRefused, "highlight empty")
}

func TestCommandDispatcherPref(t *testing.T) {
	fixture := newCommandFixture()

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbPref}, irc.SurfaceStatus), irc.OutcomeSent, "pref list")
	commandRequireText(t, fixture.host.statusTexts[0], irc.FormatPrefList(true, true, true), "pref list text")

	fixture.host.statusTexts = nil
	fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbPref, Argument: "avatars"}, irc.SurfaceStatus)
	commandRequireText(t, fixture.host.statusTexts[0], irc.FormatPrefQuery(irc.PrefAvatars, true), "pref query text")

	fixture.host.statusTexts = nil
	fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbPref, Argument: "avatars off"}, irc.SurfaceStatus)
	commandRequireText(t, fixture.host.statusTexts[0], irc.FormatPrefState(irc.PrefAvatars, false), "pref set text")
	if fixture.host.PrefEnabled(irc.PrefAvatars) {
		t.Fatalf("pref avatars still on")
	}

	fixture.host.statusTexts = nil
	fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbPref, Argument: "bogus"}, irc.SurfaceStatus)
	commandRequireText(t, fixture.host.statusTexts[0], irc.PrefUsage(), "pref usage text")

	// On a conversation the feedback lands in the transcript.
	fixture.host.selectedKey = fixture.reducer.ConversationKey("libera", "lena")
	fixture.host.hasSelectedKey = true
	fixture.host.statusTexts = nil
	fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbPref}, irc.SurfaceConversation)
	if len(fixture.host.whoisEvents) != 1 || fixture.host.whoisEvents[0].Destination != fixture.host.selectedKey {
		t.Fatalf("pref conversation feedback = %+v", fixture.host.whoisEvents)
	}
}

func TestCommandDispatcherHelp(t *testing.T) {
	fixture := newCommandFixture()

	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbHelp}, irc.SurfaceStatus), irc.OutcomeSent, "help status")
	rows := irc.VerbTable()
	names := make([]string, 0, len(rows))
	for _, row := range rows {
		names = append(names, "/"+row.Name)
	}
	want := "Commands: " + strings.Join(names, ", ") + ". Empty /join joins the latest invite."
	commandRequireText(t, fixture.host.statusTexts[0], want, "help text")
	if !strings.HasPrefix(fixture.host.statusTexts[0], "Commands: /me, /join, /part") ||
		!strings.HasSuffix(fixture.host.statusTexts[0], "Empty /join joins the latest invite.") {
		t.Fatalf("help text shape = %q", fixture.host.statusTexts[0])
	}

	fixture.host.selectedKey = fixture.reducer.ConversationKey("libera", "#room")
	fixture.host.hasSelectedKey = true
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbHelp}, irc.SurfaceConversation), irc.OutcomeSent, "help conversation")
	if len(fixture.host.whoisEvents) != 1 || fixture.host.whoisEvents[0].FormattedBody != want {
		t.Fatalf("help conversation feedback = %+v", fixture.host.whoisEvents)
	}

	fixture.host.hasSelectedKey = false
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbHelp}, irc.SurfaceConversation), irc.OutcomeWrongScope, "help no selection")

	fixture.host.networkID = ""
	fixture.host.statusNetworkID = ""
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbHelp}, irc.SurfaceConversation), irc.OutcomeRefused, "help no network")
}

func TestCommandDispatcherDelegatedHosts(t *testing.T) {
	fixture := newCommandFixture()
	fixture.host.delegatedResult = irc.OutcomeSent

	cases := []struct {
		verb irc.Verb
		name string
	}{
		{irc.VerbWhois, "whois"},
		{irc.VerbPing, "ctcp"},
		{irc.VerbTime, "ctcp"},
		{irc.VerbVersion, "ctcp"},
		{irc.VerbMonitor, "monitor"},
		{irc.VerbUnmonitor, "monitor"},
		{irc.VerbMonitored, "monitor"},
		{irc.VerbAutoaway, "autoaway"},
		{irc.VerbList, "list"},
		{irc.VerbStatus, "status"},
		{irc.VerbAvatar, "avatar"},
	}
	for _, testCase := range cases {
		fixture.host.delegated = nil
		outcome := fixture.dispatcher.Dispatch(irc.Command{Verb: testCase.verb}, irc.SurfaceConversation)
		commandRequireOutcome(t, outcome, irc.OutcomeSent, "delegated "+testCase.name)
		if len(fixture.host.delegated) != 1 || fixture.host.delegated[0] != testCase.name {
			t.Fatalf("delegated = %q, want [%s]", fixture.host.delegated, testCase.name)
		}
	}

	fixture.host.clearSurfaceResult = irc.OutcomeSent
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbClear}, irc.SurfaceConversation), irc.OutcomeSent, "clear")
	if len(fixture.host.clearSurfaces) != 1 || fixture.host.clearSurfaces[0] != irc.SurfaceConversation {
		t.Fatalf("clear surfaces = %v", fixture.host.clearSurfaces)
	}
}

func TestCommandDispatcherWireLifecycle(t *testing.T) {
	fixture := newCommandFixture()

	// No session, no selection, but networks exist -> Refused.
	fixture.host.hasNetworks = true
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbNick, Argument: "x"}, irc.SurfaceConversation), irc.OutcomeRefused, "no session with networks")

	// No session and no networks -> NotConnected.
	fixture.host.hasNetworks = false
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbNick, Argument: "x"}, irc.SurfaceConversation), irc.OutcomeNotConnected, "no session")

	// A live but unregistered session refuses every non-quit verb.
	transport := session.NewLoopbackTransport()
	config := session.DefaultSessionConfig("libera", "libera", "irc.example.test", "me")
	connecting := session.NewSession(config, transport, session.NewFakeClock(time.Unix(0, 0)))
	connecting.Start()
	fixture.host.sessionForSurface = connecting
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbNick, Argument: "x"}, irc.SurfaceConversation), irc.OutcomeNotConnected, "unregistered session")

	// An unsupported wire verb on a registered session.
	active, _ := commandRegisteredSession(t, "libera")
	fixture.host.sessionForSurface = active
	commandRequireOutcome(t, fixture.dispatcher.Dispatch(irc.Command{Verb: irc.VerbDeop}, irc.SurfaceConversation), irc.OutcomeWrongScope, "deop without channel")
}

func TestCommandDispatcherCancelled(t *testing.T) {
	fixture := newCommandFixture()
	dispatcher := fixture.dispatcher

	key := irc.ConversationKey{NetworkID: "libera", NormalizedTarget: "#room"}
	other := irc.ConversationKey{NetworkID: "oftc", NormalizedTarget: "#room"}

	dispatcher.NoteCancelled(key)
	dispatcher.NoteCancelled(other)
	if !dispatcher.TakeCancelledSelfJoin(key) {
		t.Fatalf("key was not cancelled")
	}
	if dispatcher.TakeCancelledSelfJoin(key) {
		t.Fatalf("key cancellation was consumed twice")
	}
	if !dispatcher.TakeCancelledSelfJoin(other) {
		t.Fatalf("other key was not cancelled")
	}

	dispatcher.NoteCancelled(key)
	dispatcher.NoteCancelled(other)
	dispatcher.ForgetNetwork("libera")
	if dispatcher.TakeCancelledSelfJoin(key) {
		t.Fatalf("ForgetNetwork did not clear libera")
	}
	if !dispatcher.TakeCancelledSelfJoin(other) {
		t.Fatalf("ForgetNetwork cleared the wrong network")
	}
}
