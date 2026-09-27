package session

import (
	"strconv"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// historyConfig is the sessionTestConfig with no autojoin, mirroring the
// per-test sessionConfig overrides in tst_session.cpp.
func historyConfig() SessionConfig {
	config := sessionTestConfig(sessionTestNetworkID)
	config.AutojoinChannels = nil
	return config
}

// registerWithHistory enables batch + chathistory, welcomes, and optionally
// self-joins #omarchy (which requests CHATHISTORY).
func registerWithHistory(t *testing.T, fixture *sessionFixture, selfJoin bool) {
	t.Helper()
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch chathistory\r\n" +
		":server CAP omairc ACK :batch chathistory\r\n" +
		":server 001 omairc :Welcome\r\n")
	if selfJoin {
		fixture.inject(":omairc!u@h JOIN :#omarchy\r\n")
	}
}

// messageCommands returns the command of every recorded message.
func messageCommands(messages []irc.Message) []string {
	commands := make([]string, len(messages))
	for index, message := range messages {
		commands[index] = message.Command
	}
	return commands
}

// messageBodies returns the trailing parameter of every recorded PRIVMSG.
func messageBodies(messages []irc.Message) []string {
	var bodies []string
	for _, message := range messages {
		if message.Command == "PRIVMSG" && len(message.Params) > 0 {
			bodies = append(bodies, message.Params[len(message.Params)-1])
		}
	}
	return bodies
}

func assertStringsEqual(t *testing.T, got, want []string) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("got = %q, want %q", got, want)
	}
	for index := range want {
		if got[index] != want[index] {
			t.Fatalf("value %d = %q, want %q", index, got[index], want[index])
		}
	}
}

// TestSessionBatchOpenAndCloseDoNotEmitBatchLines ports
// SessionTest::batchOpenAndCloseDoNotEmitBatchLines.
func TestSessionBatchOpenAndCloseDoNotEmitBatchLines(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.registerWithWelcome()
	start := len(fixture.handler.messages)

	fixture.inject(":irc.host BATCH +ns netsplit irc.example irc.other\r\n" +
		"@batch=ns :alice!u@h PRIVMSG #omarchy :still here\r\n" +
		":irc.host BATCH -ns\r\n")

	assertStringsEqual(t, messageCommands(fixture.handler.messages[start:]), []string{"PRIVMSG"})
	if fixture.handler.hasLabel("BATCH") {
		t.Fatal("BATCH must not reach Status")
	}
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
}

// TestSessionNestedBatchesDoNotFailTheSession ports
// SessionTest::nestedBatchesDoNotFailTheSession.
func TestSessionNestedBatchesDoNotFailTheSession(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.registerWithWelcome()
	start := len(fixture.handler.messages)

	fixture.inject(":irc.host BATCH +outer netsplit irc.a irc.b\r\n" +
		"@batch=outer :irc.host BATCH +inner netjoin irc.a irc.b\r\n" +
		"@batch=inner :alice!u@h PRIVMSG #omarchy :Hi\r\n" +
		"@batch=outer :irc.host BATCH -inner\r\n" +
		":irc.host BATCH -outer\r\n")

	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
	assertStringsEqual(t, messageCommands(fixture.handler.messages[start:]), []string{"PRIVMSG"})
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
}

// TestSessionUnknownBatchTypeAndCloseStayRegistered ports
// SessionTest::unknownBatchTypeAndCloseStayRegistered.
func TestSessionUnknownBatchTypeAndCloseStayRegistered(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.registerWithWelcome()
	start := len(fixture.handler.messages)

	fixture.inject(":irc.host BATCH +x unknown.example/foo\r\n" +
		"@batch=x :alice!u@h PRIVMSG #omarchy :delivered\r\n" +
		":irc.host BATCH -x\r\n" +
		":irc.host BATCH -missing\r\n" +
		":bob!u@h PRIVMSG #omarchy :after\r\n")

	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
	assertStringsEqual(t, messageCommands(fixture.handler.messages[start:]),
		[]string{"PRIVMSG", "PRIVMSG"})
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
}

// TestSessionHistoryBatchSwallowsInnerPrivmsg ports
// SessionTest::historyBatchSwallowsInnerPrivmsg.
func TestSessionHistoryBatchSwallowsInnerPrivmsg(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerWithHistory(t, fixture, true)
	start := len(fixture.handler.messages)

	fixture.inject(":irc.host BATCH +hx chathistory #omarchy\r\n" +
		"@batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n" +
		":irc.host BATCH -hx\r\n")

	if len(fixture.handler.messages) != start {
		t.Fatalf("commands = %q, want none", messageCommands(fixture.handler.messages[start:]))
	}
	if len(fixture.handler.batches) != 1 {
		t.Fatalf("batches = %d, want 1", len(fixture.handler.batches))
	}
	batch := fixture.handler.batches[0]
	if batch.Target != "#omarchy" || len(batch.Lines) != 1 || batch.Lines[0].Command != "PRIVMSG" {
		t.Fatalf("batch = %+v, want #omarchy with one PRIVMSG", batch)
	}
	if fixture.handler.hasLabel("BATCH") || fixture.handler.anyFieldContains("older") {
		t.Fatal("batch plumbing must not reach Status")
	}
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}

	fixture.inject(":irc.host BATCH +empty chathistory #omarchy\r\n" +
		":irc.host BATCH -empty\r\n")
	if len(fixture.handler.batches) != 2 {
		t.Fatalf("batches = %d, want 2", len(fixture.handler.batches))
	}
	if len(fixture.handler.batches[1].Lines) != 0 ||
		fixture.handler.batches[1].Target != "#omarchy" {
		t.Fatalf("empty batch = %+v, want #omarchy with no lines", fixture.handler.batches[1])
	}
}

// TestSessionDraftHistoryBatchSwallowsInnerPrivmsg ports
// SessionTest::draftHistoryBatchSwallowsInnerPrivmsg.
func TestSessionDraftHistoryBatchSwallowsInnerPrivmsg(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch draft/chathistory\r\n" +
		":server CAP omairc ACK :batch draft/chathistory\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n")
	start := len(fixture.handler.messages)

	fixture.inject(":irc.host BATCH +hx draft/chathistory #omarchy\r\n" +
		"@batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n" +
		":irc.host BATCH -hx\r\n")

	if len(fixture.handler.messages) != start {
		t.Fatalf("commands = %q, want none", messageCommands(fixture.handler.messages[start:]))
	}
	if len(fixture.handler.batches) != 1 || fixture.handler.batches[0].Target != "#omarchy" {
		t.Fatalf("batches = %+v, want one #omarchy batch", fixture.handler.batches)
	}
}

// TestSessionUnsolicitedHistoryFloodKeepsLiveTraffic ports
// SessionTest::unsolicitedHistoryFloodKeepsLiveTraffic.
func TestSessionUnsolicitedHistoryFloodKeepsLiveTraffic(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerWithHistory(t, fixture, false)
	start := len(fixture.handler.messages)

	var fill string
	for index := 0; index < 16; index++ {
		fill += ":irc.host BATCH +d" + strconv.Itoa(index) + " unknown.example/foo\r\n"
	}
	for index := 0; index < 40; index++ {
		fill += ":irc.host BATCH +x" + strconv.Itoa(index) + " chathistory #omarchy\r\n"
	}
	fill += ":irc.host BATCH +hx chathistory #omarchy\r\n" +
		"@batch=x0 :alice!u@h PRIVMSG #omarchy :recorded\r\n" +
		"@batch=x39 :alice!u@h PRIVMSG #omarchy :unrecorded\r\n" +
		"@batch=hx :alice!u@h PRIVMSG #omarchy :overflow\r\n" +
		":irc.host BATCH -hx\r\n" +
		":bob!u@h PRIVMSG #omarchy :after\r\n"
	fixture.inject(fill)

	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]),
		[]string{"unrecorded", "overflow", "after"})
	if len(fixture.handler.batches) != 0 {
		t.Fatalf("batches = %d, want 0", len(fixture.handler.batches))
	}
}

// TestSessionOverflowedUnknownBatchPassesInnerPrivmsg ports
// SessionTest::overflowedUnknownBatchPassesInnerPrivmsg.
func TestSessionOverflowedUnknownBatchPassesInnerPrivmsg(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerWithHistory(t, fixture, false)
	start := len(fixture.handler.messages)

	var fill string
	for index := 0; index < 16; index++ {
		fill += ":irc.host BATCH +d" + strconv.Itoa(index) + " unknown.example/foo\r\n"
	}
	fill += ":irc.host BATCH +extra unknown.example/foo\r\n" +
		"@batch=extra :alice!u@h PRIVMSG #omarchy :passed\r\n" +
		":irc.host BATCH +hx chathistory #omarchy\r\n" +
		"@batch=hx :alice!u@h PRIVMSG #omarchy :overflow\r\n" +
		":bob!u@h PRIVMSG #omarchy :after\r\n"
	fixture.inject(fill)

	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]),
		[]string{"passed", "after"})
	if len(fixture.handler.batches) != 0 {
		t.Fatalf("batches = %d, want 0", len(fixture.handler.batches))
	}
}

// TestSessionLeftoverNestedBatchLineAfterRootCloseIsSwallowed ports
// SessionTest::leftoverNestedBatchLineAfterRootCloseIsSwallowed.
func TestSessionLeftoverNestedBatchLineAfterRootCloseIsSwallowed(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerWithHistory(t, fixture, true)
	start := len(fixture.handler.messages)

	fixture.inject(":irc.host BATCH +stale chathistory #omarchy\r\n" +
		"@batch=stale :irc.host BATCH +child chathistory #omarchy\r\n" +
		"@batch=child :alice!u@h PRIVMSG #omarchy :old\r\n" +
		":irc.host BATCH -stale\r\n" +
		"@batch=child :alice!u@h PRIVMSG #omarchy :late\r\n" +
		":irc.host BATCH -child\r\n" +
		":bob!u@h PRIVMSG #omarchy :after\r\n")

	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]), []string{"after"})
	if len(fixture.handler.batches) != 1 || len(fixture.handler.batches[0].Lines) != 1 {
		t.Fatalf("batches = %+v, want one batch with one line", fixture.handler.batches)
	}
}

// TestSessionIgnoredBatchLedgerRecoversAfterClose ports
// SessionTest::ignoredBatchLedgerRecoversAfterClose.
func TestSessionIgnoredBatchLedgerRecoversAfterClose(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerWithHistory(t, fixture, false)
	start := len(fixture.handler.messages)

	var fill string
	for index := 0; index < 16; index++ {
		fill += ":irc.host BATCH +d" + strconv.Itoa(index) + " unknown.example/foo\r\n"
	}
	for index := 0; index < 32; index++ {
		fill += ":irc.host BATCH +x" + strconv.Itoa(index) + " chathistory #omarchy\r\n"
	}
	fill += ":irc.host BATCH -x0\r\n" +
		":irc.host BATCH +hx chathistory #omarchy\r\n" +
		"@batch=hx :alice!u@h PRIVMSG #omarchy :overflow\r\n" +
		":bob!u@h PRIVMSG #omarchy :after\r\n"
	fixture.inject(fill)

	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]), []string{"after"})
	if len(fixture.handler.batches) != 0 {
		t.Fatalf("batches = %d, want 0", len(fixture.handler.batches))
	}
}

// TestSessionUnsolicitedHistoryBatchStillEmits ports
// SessionTest::unsolicitedHistoryBatchStillEmits.
func TestSessionUnsolicitedHistoryBatchStillEmits(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerWithHistory(t, fixture, false)

	fixture.inject(":irc.host BATCH +hx chathistory #omarchy\r\n" +
		"@batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n" +
		":irc.host BATCH -hx\r\n")

	if len(fixture.handler.batches) != 1 || fixture.handler.batches[0].Target != "#omarchy" ||
		len(fixture.handler.batches[0].Lines) != 1 {
		t.Fatalf("batches = %+v, want one #omarchy batch with one line", fixture.handler.batches)
	}
	if got := fixture.handler.batches[0].Lines[0].Params[len(fixture.handler.batches[0].Lines[0].Params)-1]; got != "older" {
		t.Fatalf("line body = %q, want older", got)
	}
}

// TestSessionHistoryBatchWithoutCapabilityIsIgnored ports
// SessionTest::historyBatchWithoutCapabilityIsIgnored.
func TestSessionHistoryBatchWithoutCapabilityIsIgnored(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.registerWithWelcome()
	start := len(fixture.handler.messages)

	fixture.inject(":irc.host BATCH +hx chathistory #omarchy\r\n" +
		"@batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n" +
		":irc.host BATCH -hx\r\n" +
		":bob!u@h PRIVMSG #omarchy :after\r\n")

	if len(fixture.handler.batches) != 0 {
		t.Fatalf("batches = %d, want 0", len(fixture.handler.batches))
	}
	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]), []string{"after"})
}

// TestSessionBouncerPlaybackBatchNeedsOnlyTheBatchCapability ports
// SessionTest::bouncerPlaybackBatchNeedsOnlyTheBatchCapability.
func TestSessionBouncerPlaybackBatchNeedsOnlyTheBatchCapability(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch\r\n" +
		":server CAP omairc ACK :batch\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n")
	if !fixture.session.Capabilities().Contains(irc.CapabilityBatch) {
		t.Fatal("batch must be enabled")
	}
	if fixture.session.Capabilities().Contains(irc.CapabilityChatHistory) {
		t.Fatal("chathistory must not be enabled")
	}
	start := len(fixture.handler.messages)

	fixture.inject(":znc.in BATCH +pb znc.in/playback #omarchy\r\n" +
		"@batch=pb :lena!u@h PRIVMSG #omarchy :yesterday\r\n" +
		":znc.in BATCH -pb\r\n" +
		":bob!u@h PRIVMSG #omarchy :after\r\n")

	if len(fixture.handler.batches) != 1 || fixture.handler.batches[0].Target != "#omarchy" ||
		len(fixture.handler.batches[0].Lines) != 1 {
		t.Fatalf("batches = %+v, want one #omarchy batch with one line", fixture.handler.batches)
	}
	line := fixture.handler.batches[0].Lines[0]
	if line.Command != "PRIVMSG" || line.Params[len(line.Params)-1] != "yesterday" {
		t.Fatalf("line = %+v, want a PRIVMSG yesterday", line)
	}
	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]), []string{"after"})
}

// TestSessionBouncerQueryPlaybackBatchCollectsUnderThePeerNick ports
// SessionTest::bouncerQueryPlaybackBatchCollectsUnderThePeerNick.
func TestSessionBouncerQueryPlaybackBatchCollectsUnderThePeerNick(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch echo-message\r\n" +
		":server CAP omairc ACK :batch echo-message\r\n" +
		":server 001 omairc :Welcome\r\n")

	fixture.inject(":znc.in BATCH +q znc.in/playback lena\r\n" +
		"@batch=q :lena!u@h PRIVMSG omairc :are you there\r\n" +
		"@batch=q :omairc!u@h PRIVMSG lena :just got back\r\n" +
		":znc.in BATCH -q\r\n")

	if len(fixture.handler.batches) != 1 {
		t.Fatalf("batches = %d, want 1", len(fixture.handler.batches))
	}
	batch := fixture.handler.batches[0]
	if batch.Target != "lena" || len(batch.Lines) != 2 {
		t.Fatalf("batch = %+v, want target lena with two lines", batch)
	}
	if got := batch.Lines[0].Params[len(batch.Lines[0].Params)-1]; got != "are you there" {
		t.Fatalf("first line = %q, want are you there", got)
	}
	if got := batch.Lines[1].Params[len(batch.Lines[1].Params)-1]; got != "just got back" {
		t.Fatalf("second line = %q, want just got back", got)
	}
}

// TestSessionBouncerBatchPlaybackTypeIsNotReplay ports
// SessionTest::bouncerBatchPlaybackTypeIsNotReplay.
func TestSessionBouncerBatchPlaybackTypeIsNotReplay(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch\r\n" +
		":server CAP omairc ACK :batch\r\n" +
		":server 001 omairc :Welcome\r\n")
	start := len(fixture.handler.messages)

	fixture.inject(":znc.in BATCH +q znc.in/batch/playback lena\r\n" +
		"@batch=q :lena!u@h PRIVMSG omairc :are you there\r\n" +
		":znc.in BATCH -q\r\n")

	if len(fixture.handler.batches) != 0 {
		t.Fatalf("batches = %d, want 0", len(fixture.handler.batches))
	}
	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]), []string{"are you there"})
}

// TestSessionUnsolicitedBouncerPlaybackSharesTheOpenBatchBudget ports
// SessionTest::unsolicitedBouncerPlaybackSharesTheOpenBatchBudget.
func TestSessionUnsolicitedBouncerPlaybackSharesTheOpenBatchBudget(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerWithHistory(t, fixture, true)
	if !fixture.session.HistoryPending() {
		t.Fatal("history must be pending after the self join")
	}

	var fill string
	for index := 0; index < 16; index++ {
		fill += ":irc.host BATCH +d" + strconv.Itoa(index) + " unknown.example/foo\r\n"
	}
	fill += ":znc.in BATCH +pb znc.in/playback #omarchy\r\n" +
		"@batch=pb :lena!u@h PRIVMSG #omarchy :crowded out\r\n" +
		":znc.in BATCH -pb\r\n" +
		":irc.host BATCH +hx chathistory #omarchy\r\n" +
		"@batch=hx :alice!u@h PRIVMSG #omarchy :asked for\r\n" +
		":irc.host BATCH -hx\r\n"
	fixture.inject(fill)

	if len(fixture.handler.batches) != 1 {
		t.Fatalf("batches = %d, want 1", len(fixture.handler.batches))
	}
	batch := fixture.handler.batches[0]
	if batch.Target != "#omarchy" || len(batch.Lines) != 1 {
		t.Fatalf("batch = %+v, want one #omarchy line", batch)
	}
	if got := batch.Lines[0].Params[len(batch.Lines[0].Params)-1]; got != "asked for" {
		t.Fatalf("line = %q, want asked for", got)
	}
	if fixture.session.HistoryPending() {
		t.Fatal("history must no longer be pending")
	}
}

// TestSessionSolicitedHistoryBatchSurvivesOpenBatchFlood ports
// SessionTest::solicitedHistoryBatchSurvivesOpenBatchFlood.
func TestSessionSolicitedHistoryBatchSurvivesOpenBatchFlood(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerWithHistory(t, fixture, true)

	var fill string
	for index := 0; index < 16; index++ {
		fill += ":irc.host BATCH +d" + strconv.Itoa(index) + " unknown.example/foo\r\n"
	}
	fill += ":irc.host BATCH +hx chathistory #omarchy\r\n" +
		"@batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n" +
		":irc.host BATCH -hx\r\n"
	fixture.inject(fill)

	if len(fixture.handler.batches) != 1 || fixture.handler.batches[0].Target != "#omarchy" ||
		len(fixture.handler.batches[0].Lines) != 1 {
		t.Fatalf("batches = %+v, want one #omarchy batch with one line", fixture.handler.batches)
	}
	if fixture.session.HistoryPending() {
		t.Fatal("history must no longer be pending")
	}
}

// TestSessionOpenPlaybackDoesNotMakePendingHistoryUnsolicited ports
// SessionTest::openPlaybackDoesNotMakePendingHistoryUnsolicited.
func TestSessionOpenPlaybackDoesNotMakePendingHistoryUnsolicited(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerWithHistory(t, fixture, true)

	fill := ":znc.in BATCH +pb znc.in/playback #omarchy\r\n" +
		"@batch=pb :lena!u@h PRIVMSG #omarchy :from the bouncer\r\n"
	for index := 0; index < 16; index++ {
		fill += ":irc.host BATCH +d" + strconv.Itoa(index) + " unknown.example/foo\r\n"
	}
	fill += ":irc.host BATCH +hx chathistory #omarchy\r\n" +
		"@batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n" +
		":irc.host BATCH -hx\r\n" +
		":znc.in BATCH -pb\r\n"
	fixture.inject(fill)

	if len(fixture.handler.batches) != 2 {
		t.Fatalf("batches = %d, want 2", len(fixture.handler.batches))
	}
	first := fixture.handler.batches[0]
	if first.Target != "#omarchy" || len(first.Lines) != 1 ||
		first.Lines[0].Params[len(first.Lines[0].Params)-1] != "older" {
		t.Fatalf("first batch = %+v, want the solicited older line", first)
	}
	second := fixture.handler.batches[1]
	if second.Target != "#omarchy" || len(second.Lines) != 1 ||
		second.Lines[0].Params[len(second.Lines[0].Params)-1] != "from the bouncer" {
		t.Fatalf("second batch = %+v, want the bouncer line", second)
	}
	if fixture.session.HistoryPending() {
		t.Fatal("history must no longer be pending")
	}
}
