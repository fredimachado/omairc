package session

import (
	"fmt"
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// draft/multiline receive (#478). The Rust twins are in
// `client/src/session/tests/multiline.rs`.

const multilineCaps = "batch message-tags draft/multiline chathistory"

func registerMultilineWith(t *testing.T, fixture *sessionFixture, caps string) {
	t.Helper()
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :" + caps + "\r\n" +
		":server CAP omairc ACK :" + caps + "\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n")
}

func registerMultiline(t *testing.T, fixture *sessionFixture) {
	t.Helper()
	registerMultilineWith(t, fixture, multilineCaps)
}

// multilineBatch is one closed multiline batch of mira's lines.
func multilineBatch(reference string, lines ...string) string {
	text := ":mira!u@h BATCH +" + reference + " draft/multiline #omarchy\r\n"
	for _, line := range lines {
		text += "@batch=" + reference + " :mira!u@h PRIVMSG #omarchy :" + line + "\r\n"
	}
	return text + ":mira!u@h BATCH -" + reference + "\r\n"
}

func messageBody(message irc.Message) string {
	if len(message.Params) < 2 {
		return ""
	}
	return message.Params[1]
}

func messageTag(message irc.Message, name string) (string, bool) {
	for _, tag := range message.Tags {
		if tag.Name == name && tag.Value != nil {
			return *tag.Value, true
		}
	}
	return "", false
}

// openUnknownBatches opens count batches of an unknown type, which fill the
// open-batch budget.
func openUnknownBatches(count int) string {
	var text string
	for index := 0; index < count; index++ {
		text += fmt.Sprintf(":irc.host BATCH +d%d unknown.example/foo\r\n", index)
	}
	return text
}

func numberedLines(count int) []string {
	lines := make([]string, count)
	for index := range lines {
		lines[index] = fmt.Sprintf("l%d", index)
	}
	return lines
}

func TestSessionMultilineThreeLineBatchIsOneMessage(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(multilineBatch("ml", "one", "two", "three"))
	messages := fixture.handler.messages[start:]
	if len(messages) != 1 {
		t.Fatalf("messages = %d, want 1", len(messages))
	}
	if messageBody(messages[0]) != "one\ntwo\nthree" || messages[0].Command != "PRIVMSG" ||
		messages[0].Params[0] != "#omarchy" {
		t.Fatalf("message = %+v", messages[0])
	}
	if len(fixture.handler.batches) != 0 {
		t.Fatal("a live multiline must not be a history batch")
	}
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v", got)
	}
}

func TestSessionMultilineLinesAreHeldUntilTheBatchCloses(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(":mira!u@h BATCH +ml draft/multiline #omarchy\r\n" +
		"@batch=ml :mira!u@h PRIVMSG #omarchy :one\r\n")
	if len(fixture.handler.messages) != start {
		t.Fatal("a line was delivered before BATCH -ml")
	}
	fixture.inject(":mira!u@h BATCH -ml\r\n")
	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]), []string{"one"})
}

func TestSessionMultilineConcatLineJoinsWithNoSeparator(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(":mira!u@h BATCH +ml draft/multiline #omarchy\r\n" +
		"@batch=ml :mira!u@h PRIVMSG #omarchy :first half, \r\n" +
		"@batch=ml;draft/multiline-concat :mira!u@h PRIVMSG #omarchy :second half\r\n" +
		"@batch=ml :mira!u@h PRIVMSG #omarchy :next line\r\n" +
		":mira!u@h BATCH -ml\r\n")
	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]),
		[]string{"first half, second half\nnext line"})
}

func TestSessionMultilineBatchLineSuppliesMsgidTimeAndAccount(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject("@msgid=open1;time=2026-01-02T03:04:05.000Z;account=mira :mira!u@h BATCH +ml draft/multiline #omarchy\r\n" +
		"@batch=ml;msgid=line1;time=1999-01-01T00:00:00.000Z :mira!u@h PRIVMSG #omarchy :one\r\n" +
		"@batch=ml;msgid=line2 :mira!u@h PRIVMSG #omarchy :two\r\n" +
		":mira!u@h BATCH -ml\r\n")
	messages := fixture.handler.messages[start:]
	if len(messages) != 1 {
		t.Fatalf("messages = %d, want 1", len(messages))
	}
	if got, _ := messageTag(messages[0], "msgid"); got != "open1" {
		t.Fatalf("msgid = %q", got)
	}
	if got, _ := messageTag(messages[0], "time"); got != "2026-01-02T03:04:05.000Z" {
		t.Fatalf("time = %q", got)
	}
	if got, _ := messageTag(messages[0], "account"); got != "mira" {
		t.Fatalf("account = %q", got)
	}
	if _, found := messageTag(messages[0], "batch"); found {
		t.Fatal("the batch tag stayed on the message")
	}
}

func TestSessionMultilineNoticeBatchIsOneNotice(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(":mira!u@h BATCH +ml draft/multiline #omarchy\r\n" +
		"@batch=ml :mira!u@h NOTICE #omarchy :one\r\n" +
		"@batch=ml :mira!u@h NOTICE #omarchy :two\r\n" +
		":mira!u@h BATCH -ml\r\n")
	messages := fixture.handler.messages[start:]
	if len(messages) != 1 || messages[0].Command != "NOTICE" || messageBody(messages[0]) != "one\ntwo" {
		t.Fatalf("messages = %+v", messages)
	}
}

func TestSessionMultilineCtcpBatchFallsBackToItsLines(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(":mira!u@h BATCH +ml draft/multiline #omarchy\r\n" +
		"@batch=ml :mira!u@h PRIVMSG #omarchy :one\r\n" +
		"@batch=ml :mira!u@h PRIVMSG #omarchy :\x01ACTION waves\x01\r\n" +
		":mira!u@h BATCH -ml\r\n")
	messages := fixture.handler.messages[start:]
	if len(messages) != 2 {
		t.Fatalf("messages = %d, want 2", len(messages))
	}
	if _, found := messageTag(messages[0], "batch"); found {
		t.Fatal("the batch tag stayed on a line")
	}
}

func TestSessionMultilineTwoBatchesInterleaveWithoutMixing(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(":mira!u@h BATCH +a draft/multiline #omarchy\r\n" +
		":bob!u@h BATCH +b draft/multiline #omarchy\r\n" +
		"@batch=a :mira!u@h PRIVMSG #omarchy :a1\r\n" +
		"@batch=b :bob!u@h PRIVMSG #omarchy :b1\r\n" +
		"@batch=a :mira!u@h PRIVMSG #omarchy :a2\r\n" +
		"@batch=b :bob!u@h PRIVMSG #omarchy :b2\r\n" +
		":bob!u@h BATCH -b\r\n" +
		":mira!u@h BATCH -a\r\n")
	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]),
		[]string{"b1\nb2", "a1\na2"})
}

// Nested in CHATHISTORY.

func multilineHistoryWith(inner string) string {
	return ":irc.host BATCH +hx chathistory #omarchy\r\n" +
		"@batch=hx;msgid=m0 :alice!u@h PRIVMSG #omarchy :before\r\n" +
		inner +
		"@batch=hx;msgid=m2 :alice!u@h PRIVMSG #omarchy :after\r\n" +
		":irc.host BATCH -hx\r\n"
}

func nestedMultilineBatch(lines ...string) string {
	text := "@batch=hx;msgid=ml1;time=2026-01-02T03:04:05.000Z :mira!u@h BATCH +ml draft/multiline #omarchy\r\n"
	for _, line := range lines {
		text += "@batch=ml :mira!u@h PRIVMSG #omarchy :" + line + "\r\n"
	}
	return text + "@batch=hx :mira!u@h BATCH -ml\r\n"
}

func TestSessionMultilineNestedInChathistoryIsOneLineOfTheReplay(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(multilineHistoryWith(nestedMultilineBatch("one", "two", "three")))
	if len(fixture.handler.messages) != start {
		t.Fatal("a nested multiline leaked as live traffic")
	}
	if len(fixture.handler.batches) != 1 {
		t.Fatalf("batches = %d, want 1", len(fixture.handler.batches))
	}
	lines := fixture.handler.batches[0].Lines
	if len(lines) != 3 {
		t.Fatalf("lines = %d, want before, the message, after", len(lines))
	}
	if messageBody(lines[0]) != "before" || messageBody(lines[1]) != "one\ntwo\nthree" ||
		messageBody(lines[2]) != "after" {
		t.Fatalf("lines = %+v", lines)
	}
	if got, _ := messageTag(lines[1], "msgid"); got != "ml1" {
		t.Fatalf("msgid = %q", got)
	}
	if _, found := messageTag(lines[1], "batch"); found {
		t.Fatal("the batch tag stayed on the assembled message")
	}
}

func TestSessionMultilineNestedWithoutTheCapabilityIsCollectedLineByLine(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultilineWith(t, fixture, "batch message-tags chathistory")
	fixture.inject(multilineHistoryWith(nestedMultilineBatch("one", "two")))
	if len(fixture.handler.batches) != 1 {
		t.Fatalf("batches = %d, want 1", len(fixture.handler.batches))
	}
	var bodies []string
	for _, line := range fixture.handler.batches[0].Lines {
		bodies = append(bodies, messageBody(line))
	}
	assertStringsEqual(t, bodies, []string{"before", "one", "two", "after"})
}

func TestSessionMultilineReplayCeilingHoldsForAssembledMessages(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	var text strings.Builder
	text.WriteString(":irc.host BATCH +hx chathistory #omarchy\r\n")
	for index := 0; index < 257; index++ {
		fmt.Fprintf(&text, "@batch=hx :mira!u@h BATCH +m%[1]d draft/multiline #omarchy\r\n"+
			"@batch=m%[1]d :mira!u@h PRIVMSG #omarchy :a%[1]d\r\n"+
			"@batch=m%[1]d :mira!u@h PRIVMSG #omarchy :b%[1]d\r\n"+
			"@batch=hx :mira!u@h BATCH -m%[1]d\r\n", index)
	}
	text.WriteString(":irc.host BATCH -hx\r\n")
	fixture.inject(text.String())
	if len(fixture.handler.batches) != 1 {
		t.Fatalf("batches = %d, want 1", len(fixture.handler.batches))
	}
	lines := fixture.handler.batches[0].Lines
	if len(lines) != 256 {
		t.Fatalf("lines = %d, want the newest 256", len(lines))
	}
	if messageBody(lines[0]) != "a1\nb1" || messageBody(lines[255]) != "a256\nb256" {
		t.Fatalf("first = %q, last = %q", messageBody(lines[0]), messageBody(lines[255]))
	}
}

func TestSessionMultilineStaleReplayDropsItsAssembledMessageWithTheRest(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	fixture.inject(":irc.host BATCH +hx chathistory #omarchy\r\n" +
		"@batch=hx :alice!u@h PRIVMSG #omarchy :before\r\n")
	fixture.inject(nestedMultilineBatch("one", "two"))
	fixture.inject(":omairc!u@h JOIN :#omarchy\r\n:irc.host BATCH -hx\r\n")
	if len(fixture.handler.batches) != 0 {
		t.Fatal("a stale replay was delivered")
	}
}

// A batch that never closes.

func openUnclosedMultiline(fixture *sessionFixture) {
	fixture.inject(":mira!u@h BATCH +ml draft/multiline #omarchy\r\n" +
		"@batch=ml :mira!u@h PRIVMSG #omarchy :one\r\n" +
		"@batch=ml :mira!u@h PRIVMSG #omarchy :two\r\n")
}

func TestSessionMultilineUnclosedBatchFlushesOnDisconnectAndNotAgainOnReconnect(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	openUnclosedMultiline(fixture)
	if len(fixture.handler.messages) != start {
		t.Fatal("delivered before the connection ended")
	}
	fixture.transport.RemoteClose()
	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]), []string{"one\ntwo"})

	fixture.fireReconnect()
	fixture.transport.CompleteConnect()
	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]), []string{"one\ntwo"})
}

func TestSessionMultilineUnclosedBatchFlushesWhenTheConnectionStateResets(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	openUnclosedMultiline(fixture)
	fixture.session.locked(fixture.session.resetForConnectionLocked)
	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]), []string{"one\ntwo"})
	fixture.session.locked(fixture.session.resetForConnectionLocked)
	if len(fixture.handler.messages)-start != 1 {
		t.Fatal("flushing twice delivered twice")
	}
}

func TestSessionMultilineManualStopStillShowsTheUnclosedMessage(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	openUnclosedMultiline(fixture)
	fixture.session.Stop()
	fixture.transport.RemoteClose()
	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]), []string{"one\ntwo"})
}

func TestSessionMultilineUnclosedBatchInAnUnclosedReplayIsDroppedWithIt(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(":irc.host BATCH +hx chathistory #omarchy\r\n" +
		"@batch=hx :mira!u@h BATCH +ml draft/multiline #omarchy\r\n" +
		"@batch=ml :mira!u@h PRIVMSG #omarchy :one\r\n")
	fixture.transport.RemoteClose()
	if len(fixture.handler.messages) != start || len(fixture.handler.batches) != 0 {
		t.Fatal("history was replayed as live traffic")
	}
}

// Caps on what one batch buffers.

func TestSessionMultilineExactlyTheLineCapIsStillOneMessage(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(multilineBatch("ml", numberedLines(100)...))
	messages := fixture.handler.messages[start:]
	if len(messages) != 1 || strings.Count(messageBody(messages[0]), "\n") != 99 {
		t.Fatalf("messages = %d", len(messages))
	}
}

func TestSessionMultilinePastTheLineCapTheRestArrivesOneByOne(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(multilineBatch("ml", numberedLines(103)...))
	messages := fixture.handler.messages[start:]
	if len(messages) != 4 {
		t.Fatalf("messages = %d, want the assembled one and lines 101 to 103", len(messages))
	}
	first := messageBody(messages[0])
	if strings.Count(first, "\n") != 99 || !strings.HasPrefix(first, "l0\n") ||
		!strings.HasSuffix(first, "\nl99") {
		t.Fatalf("first = %q", first)
	}
	assertStringsEqual(t, messageBodies(messages[1:]), []string{"l100", "l101", "l102"})
}

func TestSessionMultilinePastTheByteCapTheRestArrivesOneByOne(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	big := strings.Repeat("x", 4000)
	lines := make([]string, 18)
	for index := range lines {
		lines[index] = fmt.Sprintf("%d%s", index, big)
	}
	fixture.inject(multilineBatch("ml", lines...))
	messages := fixture.handler.messages[start:]
	if len(messages) != 3 {
		t.Fatalf("messages = %d, want the assembled one and two lines", len(messages))
	}
	if strings.Count(messageBody(messages[0]), "\n") != 15 {
		t.Fatalf("assembled lines = %d, want 16", strings.Count(messageBody(messages[0]), "\n")+1)
	}
	if !strings.HasPrefix(messageBody(messages[1]), "16x") || !strings.HasPrefix(messageBody(messages[2]), "17x") {
		t.Fatal("the lines past the cap changed order")
	}
}

func TestSessionMultilineNestedPastTheCapFlushesItsPiecesIntoTheReplay(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(multilineHistoryWith(nestedMultilineBatch(numberedLines(101)...)))
	if len(fixture.handler.messages) != start {
		t.Fatal("a nested multiline leaked as live traffic")
	}
	if len(fixture.handler.batches) != 1 {
		t.Fatalf("batches = %d, want 1", len(fixture.handler.batches))
	}
	lines := fixture.handler.batches[0].Lines
	if len(lines) != 4 {
		t.Fatalf("lines = %d, want before, 100 lines, line 101, after", len(lines))
	}
	if strings.Count(messageBody(lines[1]), "\n") != 99 || messageBody(lines[2]) != "l100" {
		t.Fatalf("lines = %+v", lines)
	}
}

// Not tracked.

func TestSessionMultilineWithoutAPrerequisiteTheLinesArriveOneByOne(t *testing.T) {
	for name, caps := range map[string]string{
		"message-tags": "batch draft/multiline",
		"batch":        "message-tags draft/multiline",
		"multiline":    "batch message-tags",
	} {
		t.Run("without "+name, func(t *testing.T) {
			fixture := newSessionFixture(t, historyConfig())
			registerMultilineWith(t, fixture, caps)
			start := len(fixture.handler.messages)
			fixture.inject(multilineBatch("ml", "one", "two"))
			assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]), []string{"one", "two"})
		})
	}
}

func TestSessionMultilineOverTheOpenBatchBudgetTheLinesArriveOneByOne(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	fixture.inject(openUnknownBatches(16))
	start := len(fixture.handler.messages)
	fixture.inject(multilineBatch("ml", "one", "two"))
	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]), []string{"one", "two"})
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v", got)
	}
}

func TestSessionMultilineBatchInsideAMultilineIsIgnored(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(":mira!u@h BATCH +ml draft/multiline #omarchy\r\n" +
		"@batch=ml :mira!u@h PRIVMSG #omarchy :one\r\n" +
		"@batch=ml :mira!u@h BATCH +kid netjoin irc.a irc.b\r\n" +
		"@batch=kid :bob!u@h PRIVMSG #omarchy :swallowed\r\n" +
		"@batch=ml :mira!u@h BATCH -kid\r\n" +
		"@batch=ml :mira!u@h PRIVMSG #omarchy :two\r\n" +
		":mira!u@h BATCH -ml\r\n")
	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]), []string{"one\ntwo"})
}

func TestSessionMultilineLineThatIsNotTextIsNotBuffered(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(":mira!u@h BATCH +ml draft/multiline #omarchy\r\n" +
		"@batch=ml :mira!u@h PRIVMSG #omarchy :one\r\n" +
		"@batch=ml :mira!u@h TOPIC #omarchy :nope\r\n" +
		"@batch=ml :mira!u@h PRIVMSG #omarchy :two\r\n" +
		":mira!u@h BATCH -ml\r\n")
	assertStringsEqual(t, messageBodies(fixture.handler.messages[start:]), []string{"one\ntwo"})
}

func TestSessionMultilineEmptyBatchEmitsNothing(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(multilineBatch("ml"))
	if len(fixture.handler.messages) != start {
		t.Fatal("an empty batch produced a message")
	}
}

func TestSessionMultilineOurOwnEchoedBatchIsOneMessageFromUs(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerMultiline(t, fixture)
	start := len(fixture.handler.messages)
	fixture.inject(":omairc!u@h BATCH +ml draft/multiline #omarchy\r\n" +
		"@batch=ml :omairc!u@h PRIVMSG #omarchy :one\r\n" +
		"@batch=ml :omairc!u@h PRIVMSG #omarchy :two\r\n" +
		":omairc!u@h BATCH -ml\r\n")
	messages := fixture.handler.messages[start:]
	if len(messages) != 1 || messageBody(messages[0]) != "one\ntwo" ||
		messages[0].Prefix == nil || messages[0].Prefix.Nick != "omairc" {
		t.Fatalf("messages = %+v", messages)
	}
}
