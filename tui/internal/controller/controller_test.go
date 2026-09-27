package controller

// This file ports the Phase 2 subset of tests/session/tst_controller.cpp. It
// keeps the C++ slot names and expectations, dropping only the parts that
// belong to later phases (slash-command dispatch, persistence, playback,
// inbox, monitors, ignore/mute, avatars, preferences, channel list). Where a
// C++ slot drove the controller through a slash command, the Go port drives
// the same underlying seam directly (SendToTarget for the QuietSend cause, the
// session's SendAction/SendNotice for the frame-split contract, the reducer's
// ClearMessages for /clear) and says so in a comment.

import (
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// --- Fixtures and assertion helpers ---------------------------------------

// newController returns a controller on a deterministic FakeClock. No test
// sleeps or reads the wall clock.
func newController(t *testing.T) (*Controller, *session.FakeClock) {
	t.Helper()
	c := New()
	clock := session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC))
	c.SetClock(clock)
	return c, clock
}

// baseConfig mirrors the C++ config() helper: TLS on, reconnect off.
func baseConfig(networkID, nick string) session.SessionConfig {
	config := session.DefaultSessionConfig(networkID, networkID, "irc.example", nick)
	config.TLSEnabled = true
	config.ReconnectEnabled = false
	return config
}

// addSession registers a session without starting it.
func addSession(t *testing.T, c *Controller, clock *session.FakeClock, config session.SessionConfig) *session.LoopbackTransport {
	t.Helper()
	transport := session.NewLoopbackTransport()
	if _, err := c.AddSession(config, transport, clock); err != nil {
		t.Fatalf("AddSession(%q): %v", config.NetworkID, err)
	}
	return transport
}

// startNetwork starts a registered session and completes the socket.
func startNetwork(t *testing.T, c *Controller, transport *session.LoopbackTransport, networkID string) {
	t.Helper()
	if !c.Start(networkID) {
		t.Fatalf("Start(%q) = false", networkID)
	}
	transport.CompleteConnect()
}

// addAndStart is addSession plus startNetwork.
func addAndStart(t *testing.T, c *Controller, clock *session.FakeClock, config session.SessionConfig) *session.LoopbackTransport {
	t.Helper()
	transport := addSession(t, c, clock, config)
	startNetwork(t, c, transport, config.NetworkID)
	return transport
}

// registerNetwork mirrors the C++ registerSession helper: CAP LS, an optional
// CAP ACK for the given capabilities, then the welcome.
func registerNetwork(t *testing.T, transport *session.LoopbackTransport, nick string, caps ...string) {
	t.Helper()
	var builder strings.Builder
	joined := strings.Join(caps, " ")
	if joined == "" {
		fmt.Fprintf(&builder, ":server CAP %s LS :multi-prefix\r\n", nick)
	} else {
		fmt.Fprintf(&builder, ":server CAP %s LS :%s\r\n", nick, joined)
		fmt.Fprintf(&builder, ":server CAP %s ACK :%s\r\n", nick, joined)
	}
	fmt.Fprintf(&builder, ":server 001 %s :Welcome\r\n", nick)
	transport.InjectBytes([]byte(builder.String()))
}

func inject(t *testing.T, transport *session.LoopbackTransport, text string) {
	t.Helper()
	transport.InjectBytes([]byte(text))
}

func lastWrittenFrame(t *testing.T, transport *session.LoopbackTransport) string {
	t.Helper()
	frames := transport.WrittenFrames()
	if len(frames) == 0 {
		t.Fatalf("no frames written")
	}
	return string(frames[len(frames)-1])
}

func writtenFramesContain(transport *session.LoopbackTransport, needle string) bool {
	for _, frame := range transport.WrittenFrames() {
		if strings.Contains(string(frame), needle) {
			return true
		}
	}
	return false
}

func hasConversation(c *Controller, networkID, target string) bool {
	for _, row := range c.Conversations() {
		if row.NetworkID == networkID && row.Conversation == target {
			return true
		}
	}
	return false
}

func conversationTargets(c *Controller) []string {
	targets := make([]string, 0, len(c.Conversations()))
	for _, row := range c.Conversations() {
		targets = append(targets, row.Conversation)
	}
	return targets
}

func messageBodies(c *Controller) []string {
	bodies := make([]string, 0, len(c.Messages()))
	for _, row := range c.Messages() {
		bodies = append(bodies, row.Body)
	}
	return bodies
}

func consoleContains(c *Controller, networkID, needle string) bool {
	for _, entry := range c.Console().Lines(networkID) {
		if strings.Contains(entry.Text(), needle) {
			return true
		}
	}
	return false
}

// --- reducesTrafficAndRoutesOutboundByNetwork -----------------------------

func TestReducesTrafficAndRoutesOutboundByNetwork(t *testing.T) {
	c, clock := newController(t)
	transportA := addAndStart(t, c, clock, baseConfig("network-a", "omairc"))
	transportB := addAndStart(t, c, clock, baseConfig("network-b", "omairc"))

	inject(t, transportA, ":server CAP omairc LS :multi-prefix\r\n"+
		":server 001 omairc :Welcome\r\n"+
		":server 005 omairc CHANTYPES=#& PREFIX=(ov)@+ :are supported by this server\r\n"+
		":omairc!u@h JOIN :#chan\r\n"+
		":server 353 omairc = #chan :@omairc +Alice Bob\r\n"+
		":server 366 omairc #chan :End of NAMES\r\n"+
		":Alice!u@h PRIVMSG #chan :hello\r\n")

	conversations := c.Conversations()
	if len(conversations) != 1 || conversations[0].Conversation != "#chan" {
		t.Fatalf("Conversations = %+v, want one #chan row", conversations)
	}
	messages := c.Messages()
	if len(messages) != 2 {
		t.Fatalf("Messages = %+v, want 2 rows", messages)
	}
	if messages[1].Body != "hello" {
		t.Fatalf("messages[1].Body = %q, want hello", messages[1].Body)
	}
	members := c.Members()
	if len(members) != 3 {
		t.Fatalf("Members = %+v, want 3 rows", members)
	}
	if members[0].Nick != "omairc" || members[0].Label != "@omairc" {
		t.Fatalf("members[0] = %+v", members[0])
	}
	if members[1].Nick != "Alice" || members[1].Label != "+Alice" {
		t.Fatalf("members[1] = %+v", members[1])
	}
	if members[2].Nick != "Bob" || members[2].Label != "Bob" {
		t.Fatalf("members[2] = %+v", members[2])
	}
	if c.SelectedTarget() != "#chan" {
		t.Fatalf("SelectedTarget = %q", c.SelectedTarget())
	}
	if !c.IsChannel() {
		t.Fatalf("IsChannel = false")
	}
	if c.PeopleCount() != 3 {
		t.Fatalf("PeopleCount = %d, want 3", c.PeopleCount())
	}

	if !c.SendMessage("hello") {
		t.Fatalf("SendMessage(hello) = false")
	}
	if got := lastWrittenFrame(t, transportA); got != "PRIVMSG #chan :hello\r\n" {
		t.Fatalf("last frame = %q", got)
	}
	messages = c.Messages()
	if len(messages) != 3 {
		t.Fatalf("Messages after send = %+v, want 3 rows", messages)
	}
	if messages[2].Body != "hello" || messages[2].Author != "omairc" {
		t.Fatalf("own echo = %+v", messages[2])
	}
	if c.CurrentNick() != "omairc" {
		t.Fatalf("CurrentNick = %q", c.CurrentNick())
	}

	// The /me ACTION branch is a Phase 7 command; Phase 2 only routes it
	// literally, so the port keeps the plain-PRIVMSG routing contract.
	inject(t, transportA, ":op!u@h MODE #chan +o Alice\r\n"+
		":op!u@h MODE #chan -o Alice\r\n")
	members = c.Members()
	if members[1].Nick != "Alice" || members[1].Label != "+Alice" {
		t.Fatalf("members[1] after MODE = %+v", members[1])
	}

	registerNetwork(t, transportB, "omairc")
	inject(t, transportB, ":server 005 omairc CHANTYPES=# :are supported by this server\r\n"+
		":omairc!u@h JOIN :#chan\r\n")
	c.SelectConversation("network-b", "#chan")
	if !c.SendMessage("second") {
		t.Fatalf("SendMessage(second) = false")
	}
	if got := lastWrittenFrame(t, transportB); got != "PRIVMSG #chan :second\r\n" {
		t.Fatalf("network-b last frame = %q", got)
	}
	if got := lastWrittenFrame(t, transportA); got == "PRIVMSG #chan :second\r\n" {
		t.Fatalf("network-a received network-b's frame")
	}
}

// --- conversationCreateMatrix ---------------------------------------------

func TestConversationCreateMatrix(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "echo-message")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n")
	if !c.Session("libera").Capabilities().Contains(irc.CapabilityEchoMessage) {
		t.Fatalf("echo-message not acked")
	}
	c.SelectConversation("libera", "#omarchy")

	// QuietSend writes the frame but never invents a conversation.
	if !c.SendToTarget("libera", "lena", "hello") {
		t.Fatalf("SendToTarget = false")
	}
	if got := lastWrittenFrame(t, transport); got != "PRIVMSG lena :hello\r\n" {
		t.Fatalf("frame = %q", got)
	}
	if c.SelectedTarget() != "#omarchy" {
		t.Fatalf("SelectedTarget = %q after QuietSend", c.SelectedTarget())
	}
	if hasConversation(c, "libera", "lena") {
		t.Fatalf("QuietSend invented lena: %v", conversationTargets(c))
	}

	// A self-authored inbound line never invents either.
	inject(t, transport, ":omairc!u@h PRIVMSG lena :hello\r\n")
	if c.SelectedTarget() != "#omarchy" {
		t.Fatalf("SelectedTarget = %q after self-authored line", c.SelectedTarget())
	}
	if hasConversation(c, "libera", "lena") {
		t.Fatalf("InboundSelf invented lena: %v", conversationTargets(c))
	}

	// A service direct never invents, and still reaches the Status console.
	inject(t, transport, ":NickServ!NickServ@services PRIVMSG omairc :This nickname is registered.\r\n")
	if hasConversation(c, "libera", "NickServ") || hasConversation(c, "libera", "nickserv") {
		t.Fatalf("service direct invented: %v", conversationTargets(c))
	}
	if !consoleContains(c, "libera", "This nickname is registered.") {
		t.Fatalf("Status console missing the NickServ line")
	}

	// An inbound human direct invents without stealing the selection.
	inject(t, transport, ":alice!u@h PRIVMSG omairc :hi\r\n")
	if !hasConversation(c, "libera", "alice") {
		t.Fatalf("InboundOther did not invent alice: %v", conversationTargets(c))
	}
	if c.SelectedTarget() != "#omarchy" {
		t.Fatalf("SelectedTarget = %q after inbound direct", c.SelectedTarget())
	}

	// UserOpen invents and selects, without a PRIVMSG.
	framesBefore := len(transport.WrittenFrames())
	if !c.OpenDirectMessage("bob") {
		t.Fatalf("OpenDirectMessage(bob) = false")
	}
	if c.SelectedTarget() != "bob" {
		t.Fatalf("SelectedTarget = %q after OpenDirectMessage", c.SelectedTarget())
	}
	if !hasConversation(c, "libera", "bob") {
		t.Fatalf("UserOpen did not invent bob: %v", conversationTargets(c))
	}
	if len(transport.WrittenFrames()) != framesBefore {
		t.Fatalf("OpenDirectMessage wrote a frame")
	}
	if writtenFramesContain(transport, "PRIVMSG bob") {
		t.Fatalf("OpenDirectMessage sent PRIVMSG bob")
	}
}

// --- selectedPrivmsgInsertsMessageRow -------------------------------------

func TestSelectedPrivmsgInsertsMessageRow(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n"+
		":server 353 omairc = #omarchy :omairc Alice\r\n"+
		":server 366 omairc #omarchy :End of NAMES\r\n")

	rowsAfterJoin := len(c.Messages())
	if rowsAfterJoin == 0 {
		t.Fatalf("Messages after join is empty")
	}
	before := c.Messages()

	inject(t, transport, ":Alice!u@h PRIVMSG #omarchy :hello\r\n")
	if len(c.Messages()) != rowsAfterJoin+1 {
		t.Fatalf("Messages = %d, want %d", len(c.Messages()), rowsAfterJoin+1)
	}
	if c.Messages()[rowsAfterJoin].Body != "hello" {
		t.Fatalf("appended row = %+v", c.Messages()[rowsAfterJoin])
	}
	for index := range before {
		if before[index] != c.Messages()[index] {
			t.Fatalf("existing row %d changed: %+v -> %+v", index, before[index], c.Messages()[index])
		}
	}

	if !c.SendMessage("own line") {
		t.Fatalf("SendMessage = false")
	}
	if len(c.Messages()) != rowsAfterJoin+2 {
		t.Fatalf("Messages = %d, want %d", len(c.Messages()), rowsAfterJoin+2)
	}
	if c.Messages()[rowsAfterJoin+1].Body != "own line" {
		t.Fatalf("own echo = %+v", c.Messages()[rowsAfterJoin+1])
	}
}

// --- isupportBurstDoesNotResetModels --------------------------------------

func TestIsupportBurstDoesNotResetModels(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n"+
		":server 353 omairc = #omarchy :omairc Alice\r\n"+
		":server 366 omairc #omarchy :End of NAMES\r\n")
	if c.SelectedTarget() != "#omarchy" || !c.IsChannel() {
		t.Fatalf("selection = %q isChannel=%v", c.SelectedTarget(), c.IsChannel())
	}

	epoch := c.ConversationEpoch()
	messagesBefore := append([]MessageSnapshot(nil), c.Messages()...)
	membersBefore := append([]MemberSnapshot(nil), c.Members()...)

	inject(t, transport, ":server 005 omairc CHANTYPES=# PREFIX=(v)+ :are supported by this server\r\n"+
		":server 005 omairc CHANMODES=eIbq,k,flj,CFL :are supported by this server\r\n"+
		":server 005 omairc NICKLEN=16 :are supported by this server\r\n")
	if c.ConversationEpoch() != epoch {
		t.Fatalf("ConversationEpoch moved on an ISUPPORT burst")
	}
	if len(c.Messages()) != len(messagesBefore) {
		t.Fatalf("Messages changed on an ISUPPORT burst")
	}
	if !memberSnapshotsEqual(c.Members(), membersBefore) {
		t.Fatalf("Members changed on an ISUPPORT burst: %+v", c.Members())
	}

	// With PREFIX=(v)+ a +o MODE is not a membership change and paints no rank.
	// The Go conversation epoch tracks "snapshot rebuilt", which a MODE does
	// touch, so the C++ "no conversation reset" contract is asserted on the
	// row set instead.
	targetsBeforeMode := conversationTargets(c)
	inject(t, transport, ":op!u@h MODE #omarchy +o Alice\r\n")
	if !stringSlicesEqual(conversationTargets(c), targetsBeforeMode) {
		t.Fatalf("conversation rows changed on MODE: %v -> %v", targetsBeforeMode, conversationTargets(c))
	}
	if c.Members()[0].Nick != "Alice" || c.Members()[0].Label != "Alice" {
		t.Fatalf("members[0] = %+v", c.Members()[0])
	}
	if !memberSnapshotsEqual(c.Members(), []MemberSnapshot{
		{Nick: "Alice", Label: "Alice"},
		{Nick: "omairc", Label: "omairc"},
	}) {
		t.Fatalf("members = %+v", c.Members())
	}

	// A later ISUPPORT update also reloads nothing on its own.
	epochAfterMode := c.ConversationEpoch()
	inject(t, transport, ":server 005 omairc CHANTYPES=$ :are supported by this server\r\n")
	if c.ConversationEpoch() != epochAfterMode {
		t.Fatalf("ConversationEpoch moved on a CHANTYPES update")
	}
	if c.IsChannel() {
		t.Fatalf("IsChannel = true after CHANTYPES=$")
	}

	inject(t, transport, ":omairc!u@h JOIN :$odd\r\n")
	c.SelectConversation("libera", "$odd")
	if c.SelectedTarget() != "$odd" {
		t.Fatalf("SelectedTarget = %q", c.SelectedTarget())
	}
	if !c.IsChannel() {
		t.Fatalf("IsChannel($odd) = false")
	}
}

func memberSnapshotsEqual(left, right []MemberSnapshot) bool {
	if len(left) != len(right) {
		return false
	}
	for index := range left {
		if left[index] != right[index] {
			return false
		}
	}
	return true
}

// --- incomingNickRetargetsSelectedDirect ----------------------------------

func TestIncomingNickRetargetsSelectedDirect(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n"+
		":server 353 omairc = #omarchy :omairc Alice\r\n"+
		":server 366 omairc #omarchy :End of NAMES\r\n"+
		":Alice!u@h PRIVMSG omairc :hi\r\n")
	c.SelectConversation("libera", "Alice")
	if c.SelectedTarget() != "Alice" {
		t.Fatalf("SelectedTarget = %q", c.SelectedTarget())
	}
	if c.CurrentNick() != "omairc" {
		t.Fatalf("CurrentNick = %q", c.CurrentNick())
	}

	inject(t, transport, ":Alice!u@h NICK :Alicia\r\n")
	if c.SelectedTarget() != "Alicia" {
		t.Fatalf("SelectedTarget = %q, want Alicia", c.SelectedTarget())
	}
	if hasConversation(c, "libera", "Alice") {
		t.Fatalf("old Alice row survived: %v", conversationTargets(c))
	}
	if !hasConversation(c, "libera", "Alicia") {
		t.Fatalf("Alicia row missing: %v", conversationTargets(c))
	}
	if len(c.Messages()) != 2 {
		t.Fatalf("direct Messages = %+v", c.Messages())
	}
	last := c.Messages()[len(c.Messages())-1]
	if last.Kind != "event" || last.Body != "Alice is now Alicia" {
		t.Fatalf("nick event row = %+v", last)
	}

	c.SelectConversation("libera", "#omarchy")
	if c.Members()[0].Nick != "Alicia" {
		t.Fatalf("channel member = %+v", c.Members()[0])
	}
	if len(c.Messages()) != 2 || c.Messages()[0].Body != "omairc joined" ||
		c.Messages()[1].Body != "Alice is now Alicia" {
		t.Fatalf("channel Messages = %+v", c.Messages())
	}

	c.SelectConversation("libera", "Alicia")
	if !c.SendMessage("hello") {
		t.Fatalf("SendMessage = false")
	}
	if got := lastWrittenFrame(t, transport); got != "PRIVMSG Alicia :hello\r\n" {
		t.Fatalf("frame = %q", got)
	}

	inject(t, transport, ":omairc!u@h NICK :fred\r\n")
	if c.Session("libera").Nick() != "fred" {
		t.Fatalf("session nick = %q", c.Session("libera").Nick())
	}
	if c.CurrentNick() != "fred" {
		t.Fatalf("CurrentNick = %q", c.CurrentNick())
	}
}

// --- incomingNickCaseOnlyRetargetsDirect ----------------------------------

func TestIncomingNickCaseOnlyRetargetsDirect(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc")
	inject(t, transport, ":Alice!u@h PRIVMSG omairc :hi\r\n")
	if c.SelectedTarget() != "Alice" {
		t.Fatalf("SelectedTarget = %q", c.SelectedTarget())
	}

	inject(t, transport, ":Alice!u@h NICK :ALICE\r\n")
	if c.SelectedTarget() != "ALICE" {
		t.Fatalf("SelectedTarget = %q, want ALICE", c.SelectedTarget())
	}
	if !hasConversation(c, "libera", "ALICE") {
		t.Fatalf("ALICE row missing: %v", conversationTargets(c))
	}
	if !c.SendMessage("hello") {
		t.Fatalf("SendMessage = false")
	}
	if got := lastWrittenFrame(t, transport); got != "PRIVMSG ALICE :hello\r\n" {
		t.Fatalf("frame = %q", got)
	}
}

// --- welcomeAssignedNickRoutesDirectMessages ------------------------------

func TestWelcomeAssignedNickRoutesDirectMessages(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc-very-long-name"))
	inject(t, transport, ":server CAP omairc-very-long-name LS :multi-prefix\r\n"+
		":server 001 omairc-truncated :Welcome\r\n"+
		":Alice!u@h PRIVMSG omairc-truncated :hi\r\n"+
		":Bob!u@h PRIVMSG omairc-very-long-name :nope\r\n")

	if c.Session("libera").Nick() != "omairc-truncated" {
		t.Fatalf("session nick = %q", c.Session("libera").Nick())
	}
	if c.CurrentNick() != "omairc-truncated" {
		t.Fatalf("CurrentNick = %q", c.CurrentNick())
	}
	if c.SelectedTarget() != "Alice" {
		t.Fatalf("SelectedTarget = %q", c.SelectedTarget())
	}
	if !hasConversation(c, "libera", "Alice") {
		t.Fatalf("Alice row missing: %v", conversationTargets(c))
	}
	if hasConversation(c, "libera", "Bob") {
		t.Fatalf("Bob row invented for a stale nick: %v", conversationTargets(c))
	}
}

// --- echoMessageAckSkipsLocalPrivmsg --------------------------------------

func TestEchoMessageAckSkipsLocalPrivmsg(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "echo-message")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n"+
		":server 353 omairc = #omarchy :omairc Alice\r\n"+
		":server 366 omairc #omarchy :End of NAMES\r\n")
	if !c.Session("libera").Capabilities().Contains(irc.CapabilityEchoMessage) {
		t.Fatalf("echo-message not acked")
	}
	rowsAfterJoin := len(c.Messages())
	if rowsAfterJoin == 0 {
		t.Fatalf("Messages after join is empty")
	}

	if !c.SendMessage("hello") {
		t.Fatalf("SendMessage = false")
	}
	if got := lastWrittenFrame(t, transport); got != "PRIVMSG #omarchy :hello\r\n" {
		t.Fatalf("frame = %q", got)
	}
	if len(c.Messages()) != rowsAfterJoin {
		t.Fatalf("echo-message echoed locally: %d -> %d", rowsAfterJoin, len(c.Messages()))
	}

	inject(t, transport, ":omairc!u@h PRIVMSG #omarchy :hello\r\n")
	if len(c.Messages()) != rowsAfterJoin+1 {
		t.Fatalf("server echo not admitted: %d", len(c.Messages()))
	}
	if row := c.Messages()[rowsAfterJoin]; row.Body != "hello" || row.Author != "omairc" {
		t.Fatalf("server echo row = %+v", row)
	}
}

// --- echoMessageAbsentStillEchoesLocally ----------------------------------

func TestEchoMessageAbsentStillEchoesLocally(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	inject(t, transport, ":server CAP omairc LS :echo-message\r\n"+
		":server 001 omairc :Welcome\r\n"+
		":omairc!u@h JOIN :#omarchy\r\n"+
		":server 353 omairc = #omarchy :omairc Alice\r\n"+
		":server 366 omairc #omarchy :End of NAMES\r\n")
	if c.Session("libera").Capabilities().Contains(irc.CapabilityEchoMessage) {
		t.Fatalf("echo-message enabled without an ACK")
	}
	rowsAfterJoin := len(c.Messages())

	if !c.SendMessage("hello") {
		t.Fatalf("SendMessage = false")
	}
	if len(c.Messages()) != rowsAfterJoin+1 {
		t.Fatalf("local echo missing: %d -> %d", rowsAfterJoin, len(c.Messages()))
	}
	if row := c.Messages()[rowsAfterJoin]; row.Body != "hello" || row.Author != "omairc" {
		t.Fatalf("local echo row = %+v", row)
	}
}

// --- echoMessageAckSkipsMsgEcho (adapted: SendToTarget) -------------------

func TestEchoMessageAckSkipsSendToTargetEcho(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "echo-message")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n"+
		":lena!u@h PRIVMSG omairc :hi\r\n")
	if !c.Session("libera").Capabilities().Contains(irc.CapabilityEchoMessage) {
		t.Fatalf("echo-message not acked")
	}
	c.SelectConversation("libera", "#omarchy")
	c.SelectConversation("libera", "lena")
	rowsBeforeMsg := len(c.Messages())

	// The C++ slot drove /msg; SendToTarget is the same QuietSend + echoIfPresent
	// seam without the Phase 7 slash layer.
	if !c.SendToTarget("libera", "lena", "later") {
		t.Fatalf("SendToTarget = false")
	}
	if got := lastWrittenFrame(t, transport); got != "PRIVMSG lena :later\r\n" {
		t.Fatalf("frame = %q", got)
	}
	if len(c.Messages()) != rowsBeforeMsg {
		t.Fatalf("echo-message echoed SendToTarget: %d -> %d", rowsBeforeMsg, len(c.Messages()))
	}

	inject(t, transport, ":omairc!u@h PRIVMSG lena :later\r\n")
	if len(c.Messages()) != rowsBeforeMsg+1 {
		t.Fatalf("server echo not admitted: %d", len(c.Messages()))
	}
	if row := c.Messages()[len(c.Messages())-1]; row.Body != "later" {
		t.Fatalf("server echo row = %+v", row)
	}
}

// --- echoMessageLongPrivmsgShowsEachChunkOnce -----------------------------

func TestEchoMessageLongPrivmsgShowsEachChunkOnce(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "echo-message")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n"+
		":server 353 omairc = #omarchy :omairc Alice\r\n"+
		":server 366 omairc #omarchy :End of NAMES\r\n")
	if !c.Session("libera").Capabilities().Contains(irc.CapabilityEchoMessage) {
		t.Fatalf("echo-message not acked")
	}
	rowsAfterJoin := len(c.Messages())

	first := strings.Repeat("a", 400)
	second := strings.Repeat("b", 200)
	body := first + " " + second
	if !c.SendMessage(body) {
		t.Fatalf("SendMessage = false")
	}
	if len(c.Messages()) != rowsAfterJoin {
		t.Fatalf("echo-message echoed the long local line: %d", len(c.Messages()))
	}

	inject(t, transport, ":omairc!u@h PRIVMSG #omarchy :"+first+"\r\n"+
		":omairc!u@h PRIVMSG #omarchy :"+second+"\r\n")
	if len(c.Messages()) != rowsAfterJoin+2 {
		t.Fatalf("Messages = %d, want %d", len(c.Messages()), rowsAfterJoin+2)
	}
	if got := c.Messages()[rowsAfterJoin]; got.Body != first || got.Author != "omairc" {
		t.Fatalf("chunk 1 row = %+v", got)
	}
	if got := c.Messages()[rowsAfterJoin+1]; got.Body != second || got.Author != "omairc" {
		t.Fatalf("chunk 2 row = %+v", got)
	}
}

// --- longMeAndNoticeSplitAcrossFrames (adapted: session send seams) -------

func TestLongMeAndNoticeSplitAcrossFrames(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n")
	c.SelectConversation("libera", "#omarchy")
	s := c.Session("libera")

	first := strings.Repeat("a", 400)
	second := strings.Repeat("b", 200)
	body := first + " " + second

	// /me becomes SendAction in the session; the split contract is the same.
	before := len(transport.WrittenFrames())
	if !s.SendAction("#omarchy", body) {
		t.Fatalf("SendAction = false")
	}
	actionFrames := transport.WrittenFrames()[before:]
	if len(actionFrames) != 2 {
		t.Fatalf("action frames = %d, want 2", len(actionFrames))
	}
	if got := string(actionFrames[0]); got != "PRIVMSG #omarchy :\x01ACTION "+first+"\x01\r\n" {
		t.Fatalf("action frame 0 = %q", got)
	}
	if got := string(actionFrames[1]); got != "PRIVMSG #omarchy :\x01ACTION "+second+"\x01\r\n" {
		t.Fatalf("action frame 1 = %q", got)
	}
	for _, frame := range actionFrames {
		assertSplitFrame(t, frame)
	}

	// /notice becomes SendNotice.
	before = len(transport.WrittenFrames())
	if !s.SendNotice("#omarchy", body) {
		t.Fatalf("SendNotice = false")
	}
	noticeFrames := transport.WrittenFrames()[before:]
	if len(noticeFrames) != 2 {
		t.Fatalf("notice frames = %d, want 2", len(noticeFrames))
	}
	if got := string(noticeFrames[0]); got != "NOTICE #omarchy :"+first+"\r\n" {
		t.Fatalf("notice frame 0 = %q", got)
	}
	if got := string(noticeFrames[1]); got != "NOTICE #omarchy :"+second+"\r\n" {
		t.Fatalf("notice frame 1 = %q", got)
	}
	for _, frame := range noticeFrames {
		assertSplitFrame(t, frame)
	}
}

func assertSplitFrame(t *testing.T, frame []byte) {
	t.Helper()
	if !strings.HasSuffix(string(frame), "\r\n") {
		t.Fatalf("frame does not end with CRLF: %q", frame)
	}
	if len(frame) > irc.MaxClassicFrameBytes {
		t.Fatalf("frame is %d bytes, over %d: %q", len(frame), irc.MaxClassicFrameBytes, frame)
	}
}

// --- incomingNickservPrivmsgDoesNotOpenDirect -----------------------------

func TestIncomingNickservPrivmsgDoesNotOpenDirect(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n"+
		":NickServ!NickServ@services PRIVMSG omairc :This nickname is registered.\r\n")

	if hasConversation(c, "libera", "NickServ") || hasConversation(c, "libera", "nickserv") {
		t.Fatalf("NickServ direct invented: %v", conversationTargets(c))
	}
	if !consoleContains(c, "libera", "This nickname is registered.") {
		t.Fatalf("Status console missing the NickServ line")
	}
}

// --- zncQuietSendDoesNotOpenDirect (adapted: SendToTarget) ----------------

func TestQuietSendDoesNotOpenDirect(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "echo-message")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n")
	if !c.Session("libera").Capabilities().Contains(irc.CapabilityEchoMessage) {
		t.Fatalf("echo-message not acked")
	}
	c.SelectConversation("libera", "#omarchy")

	// The C++ slot drove /znc; SendToTarget is the same QuietSend seam.
	if !c.SendToTarget("libera", "*status", "ListMods") {
		t.Fatalf("SendToTarget = false")
	}
	if got := lastWrittenFrame(t, transport); got != "PRIVMSG *status :ListMods\r\n" {
		t.Fatalf("frame = %q", got)
	}
	if c.SelectedTarget() != "#omarchy" {
		t.Fatalf("SelectedTarget = %q", c.SelectedTarget())
	}

	inject(t, transport, ":omairc!u@h PRIVMSG *status :ListMods\r\n"+
		":*status!znc@znc.in PRIVMSG omairc :Modules: playback\r\n")
	if hasConversation(c, "libera", "*status") {
		t.Fatalf("*status direct invented: %v", conversationTargets(c))
	}
	if c.SelectedTarget() != "#omarchy" {
		t.Fatalf("SelectedTarget = %q", c.SelectedTarget())
	}
	if !consoleContains(c, "libera", "Modules: playback") {
		t.Fatalf("Status console missing the *status reply")
	}
}

// --- statusMsgNickservIdentifyDoesNotOpenDirect (adapted) -----------------

func TestStatusMsgNickservIdentifyDoesNotOpenDirect(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "echo-message")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n")
	if !c.Session("libera").Capabilities().Contains(irc.CapabilityEchoMessage) {
		t.Fatalf("echo-message not acked")
	}
	c.SelectConversation("libera", "#omarchy")

	// The C++ slot drove /msg nickserv identify through the console;
	// SendToTarget is the same QuietSend path without the slash layer.
	if !c.SendToTarget("libera", "nickserv", "identify my_nick s3cret") {
		t.Fatalf("SendToTarget = false")
	}
	if got := lastWrittenFrame(t, transport); got != "PRIVMSG nickserv :identify my_nick s3cret\r\n" {
		t.Fatalf("frame = %q", got)
	}
	if c.SelectedTarget() != "#omarchy" {
		t.Fatalf("SelectedTarget = %q", c.SelectedTarget())
	}

	inject(t, transport, ":NickServ!NickServ@services PRIVMSG omairc :You are now identified for my_nick.\r\n"+
		":omairc!u@h PRIVMSG nickserv :identify my_nick s3cret\r\n")

	if c.SelectedTarget() != "#omarchy" {
		t.Fatalf("SelectedTarget = %q after the reply", c.SelectedTarget())
	}
	if hasConversation(c, "libera", "NickServ") || hasConversation(c, "libera", "nickserv") {
		t.Fatalf("NickServ direct invented: %v", conversationTargets(c))
	}
	for _, body := range messageBodies(c) {
		if strings.Contains(body, "s3cret") || strings.Contains(body, "identify my_nick") {
			t.Fatalf("secret leaked into the transcript: %q", body)
		}
	}
}

// --- conversationClearWipesMessages (adapted: reducer seam) ---------------

func TestConversationClearWipesMessages(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n")
	c.SelectConversation("libera", "#omarchy")
	if !c.SendMessage("hello") {
		t.Fatalf("SendMessage = false")
	}
	if len(c.Messages()) == 0 || !hasConversation(c, "libera", "#omarchy") {
		t.Fatalf("precondition failed: messages=%d rows=%v", len(c.Messages()), conversationTargets(c))
	}
	framesBefore := len(transport.WrittenFrames())

	// The C++ slot drove /clear; the Phase 2 equivalent is the reducer's
	// ClearMessages followed by a republish. Nothing is sent on the wire.
	key := c.Reducer().ConversationKey("libera", "#omarchy")
	c.Reducer().ClearMessages(key)
	c.Publish(irc.ViewNotify{Messages: true, Conversations: true})

	if c.LastError() != "" {
		t.Fatalf("LastError = %q", c.LastError())
	}
	if len(c.Messages()) != 0 {
		t.Fatalf("Messages = %+v, want empty", c.Messages())
	}
	if !hasConversation(c, "libera", "#omarchy") {
		t.Fatalf("channel row dropped: %v", conversationTargets(c))
	}
	if c.SelectedTarget() != "#omarchy" {
		t.Fatalf("SelectedTarget = %q", c.SelectedTarget())
	}
	if len(transport.WrittenFrames()) != framesBefore {
		t.Fatalf("clear wrote a frame")
	}
	if writtenFramesContain(transport, "PART") || writtenFramesContain(transport, "QUIT") {
		t.Fatalf("clear sent a PART or QUIT")
	}

	// Clearing an already empty transcript is a no-op.
	c.Reducer().ClearMessages(key)
	c.Publish(irc.ViewNotify{Messages: true, Conversations: true})
	if c.LastError() != "" || c.SelectedTarget() != "#omarchy" || len(c.Messages()) != 0 {
		t.Fatalf("second clear changed state: err=%q target=%q messages=%d",
			c.LastError(), c.SelectedTarget(), len(c.Messages()))
	}
}

// --- twoSessionsStartTogether ---------------------------------------------

func TestTwoSessionsStartTogether(t *testing.T) {
	c, clock := newController(t)
	transportA := addSession(t, c, clock, baseConfig("network-a", "omairc"))
	transportB := addSession(t, c, clock, baseConfig("network-b", "omairc"))
	if !c.Start("network-a") || !c.Start("network-b") {
		t.Fatalf("Start returned false")
	}
	if transportA.State() != session.ConnectionConnecting {
		t.Fatalf("transport A state = %v", transportA.State())
	}
	if transportB.State() != session.ConnectionConnecting {
		t.Fatalf("transport B state = %v", transportB.State())
	}

	transportA.CompleteConnect()
	transportB.CompleteConnect()
	registerNetwork(t, transportA, "omairc")
	registerNetwork(t, transportB, "omairc")
	inject(t, transportA, ":omairc!u@h JOIN :#alpha\r\n")
	inject(t, transportB, ":omairc!u@h JOIN :#lab\r\n")

	if len(c.Conversations()) != 2 {
		t.Fatalf("Conversations = %v, want 2 rows", conversationTargets(c))
	}
	if c.Session("network-a").State() != session.StateRegistered {
		t.Fatalf("network-a state = %v", c.Session("network-a").State())
	}
	if c.Session("network-b").State() != session.StateRegistered {
		t.Fatalf("network-b state = %v", c.Session("network-b").State())
	}
}

// --- quitWhileReconnecting ------------------------------------------------

func TestQuitWhileReconnecting(t *testing.T) {
	c, clock := newController(t)
	config := baseConfig("libera", "omairc")
	config.ReconnectEnabled = true
	transport := addAndStart(t, c, clock, config)
	registerNetwork(t, transport, "omairc")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n")
	c.SelectConversation("libera", "#omarchy")

	transport.RemoteClose()
	if c.Session("libera").State() != session.StateReconnecting {
		t.Fatalf("state = %v, want Reconnecting", c.Session("libera").State())
	}
	if clock.Pending() == 0 {
		t.Fatalf("reconnect timer not armed")
	}

	// The C++ slot drove /quit; Phase 7 wires that command to Session.Stop.
	c.Session("libera").Stop()
	if c.Session("libera").State() != session.StateIdle {
		t.Fatalf("state = %v, want Idle", c.Session("libera").State())
	}
	if clock.Pending() != 0 {
		t.Fatalf("timers still pending after Stop: %d", clock.Pending())
	}
}

// --- disconnectLeavesOtherNetworkLive -------------------------------------

func TestDisconnectLeavesOtherNetworkLive(t *testing.T) {
	c, clock := newController(t)
	transportA := addAndStart(t, c, clock, baseConfig("network-a", "omairc"))
	transportB := addAndStart(t, c, clock, baseConfig("network-b", "omairc"))
	registerNetwork(t, transportA, "omairc")
	registerNetwork(t, transportB, "omairc")
	inject(t, transportA, ":omairc!u@h JOIN :#alpha\r\n")
	inject(t, transportB, ":omairc!u@h JOIN :#lab\r\n")
	c.SelectConversation("network-a", "#alpha")

	// The C++ slot drove /disconnect; Phase 7 wires that command to
	// Session.Stop. Phase 2's session Stop does not write QUIT, so that one
	// C++ frame assertion is deferred with the command layer.
	c.Session("network-a").Stop()
	if c.Session("network-a").State() != session.StateIdle {
		t.Fatalf("network-a state = %v", c.Session("network-a").State())
	}
	if c.Session("network-b").State() != session.StateRegistered {
		t.Fatalf("network-b state = %v", c.Session("network-b").State())
	}
	if c.ConnectionStatusFor("network-a") != "Offline" {
		t.Fatalf("network-a status = %q", c.ConnectionStatusFor("network-a"))
	}
	if len(c.Conversations()) != 2 || !hasConversation(c, "network-a", "#alpha") ||
		!hasConversation(c, "network-b", "#lab") {
		t.Fatalf("Conversations = %v", conversationTargets(c))
	}

	c.OpenStatus("network-a")
	if c.FocusedNetworkID() != "network-a" {
		t.Fatalf("Status not focused on network-a: %q", c.FocusedNetworkID())
	}

	c.SelectConversation("network-b", "#lab")
	inject(t, transportB, ":zed!u@h PRIVMSG #lab :still here\r\n")
	found := false
	for _, body := range messageBodies(c) {
		if body == "still here" {
			found = true
		}
	}
	if !found {
		t.Fatalf("network-b transcript = %v", messageBodies(c))
	}
	if c.Session("network-b").State() != session.StateRegistered {
		t.Fatalf("network-b state = %v", c.Session("network-b").State())
	}
}

// --- backgroundChatBumpsConversationEpoch ---------------------------------

func TestBackgroundChatBumpsConversationEpoch(t *testing.T) {
	c, clock := newController(t)
	transportA := addAndStart(t, c, clock, baseConfig("network-a", "omairc"))
	transportB := addAndStart(t, c, clock, baseConfig("network-b", "omairc"))
	registerNetwork(t, transportA, "omairc")
	inject(t, transportA, ":omairc!u@h JOIN :#alpha\r\n")
	c.SelectConversation("network-a", "#alpha")

	registerNetwork(t, transportB, "omairc")
	inject(t, transportB, ":omairc!u@h JOIN :#lab\r\n"+
		":server 353 omairc = #lab :@omairc\r\n"+
		":server 366 omairc #lab :End of NAMES\r\n")

	epoch := c.ConversationEpoch()
	if got := c.UnreadCountFor("network-b"); got != 0 {
		t.Fatalf("UnreadCountFor(network-b) = %d, want 0", got)
	}

	inject(t, transportB, ":zed!u@h PRIVMSG #lab :ping\r\n")

	if c.ConversationEpoch() <= epoch {
		t.Fatalf("ConversationEpoch did not bump")
	}
	if got := c.UnreadCountFor("network-b"); got != 1 {
		t.Fatalf("UnreadCountFor(network-b) = %d, want 1", got)
	}
	if c.MentionFor("network-b") {
		t.Fatalf("MentionFor(network-b) = true")
	}
}

// --- selectConversationRefreshesSidebarUnread -----------------------------

// TestSelectConversationRefreshesSidebarUnread pins that selecting a
// conversation republishes the sidebar snapshot. ConversationListModel::select
// calls reload(), which repaints every row's roles, so a selection consumes its
// unread and mention badge immediately instead of leaving the cached snapshot
// stale until the next conversation-dirtying event.
func TestSelectConversationRefreshesSidebarUnread(t *testing.T) {
	c, clock := newController(t)
	transportA := addAndStart(t, c, clock, baseConfig("network-a", "omairc"))
	transportB := addAndStart(t, c, clock, baseConfig("network-b", "omairc"))

	registerNetwork(t, transportA, "omairc")
	inject(t, transportA, ":omairc!u@h JOIN :#alpha\r\n")
	c.SelectConversation("network-a", "#alpha")

	registerNetwork(t, transportB, "omairc")
	inject(t, transportB, ":omairc!u@h JOIN :#lab\r\n")
	inject(t, transportB, ":zed!u@h PRIVMSG #lab :ping\r\n")

	unreadFor := func(networkID, target string) (int, bool) {
		for _, row := range c.Conversations() {
			if row.NetworkID == networkID && row.Conversation == target {
				return row.Unread, true
			}
		}
		return 0, false
	}

	unread, found := unreadFor("network-b", "#lab")
	if !found || unread != 1 {
		t.Fatalf("network-b #lab unread = %d (found=%v), want 1", unread, found)
	}

	c.SelectConversation("network-b", "#lab")
	if unread, _ := unreadFor("network-b", "#lab"); unread != 0 {
		t.Fatalf("network-b #lab unread after select = %d, want 0", unread)
	}
	if got := c.UnreadCountFor("network-b"); got != 0 {
		t.Fatalf("UnreadCountFor(network-b) after select = %d, want 0", got)
	}
}

// --- typingEventRefreshesSidebarTyping ------------------------------------

// TestTypingEventRefreshesSidebarTyping pins that a typing notification
// repaints the sidebar's typing role. Qt emits typingChanged() and calls
// ConversationListModel::invalidateTyping() when the conversation list was not
// already reloaded; the Go snapshot materializes that role, so the direct row
// must repaint for the hint to show.
func TestTypingEventRefreshesSidebarTyping(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n")
	// The peer's line opens the direct message the typing hint belongs to.
	inject(t, transport, ":lena!u@h PRIVMSG omairc :hey\r\n")
	c.SelectConversation("libera", "lena")

	typingFor := func(target string) (bool, bool) {
		for _, row := range c.Conversations() {
			if row.NetworkID == "libera" && row.Conversation == target {
				return row.Typing, true
			}
		}
		return false, false
	}
	if typing, found := typingFor("lena"); !found || typing {
		t.Fatalf("lena sidebar typing before TAGMSG = %v (found=%v), want false", typing, found)
	}

	inject(t, transport, "@+typing=active :lena!u@h TAGMSG omairc\r\n")
	if typing, found := typingFor("lena"); !found || !typing {
		t.Fatalf("lena sidebar typing after TAGMSG = %v (found=%v), want true", typing, found)
	}
}
