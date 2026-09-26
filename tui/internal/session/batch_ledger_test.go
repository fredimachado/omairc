package session

import (
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// TestSessionLeftoverHistoryBatchAfterPartDoesNotEmit ports
// SessionTest::leftoverHistoryBatchAfterPartDoesNotEmit.
func TestSessionLeftoverHistoryBatchAfterPartDoesNotEmit(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerWithHistory(t, fixture, true)

	fixture.inject(":irc.host BATCH +stale chathistory #omarchy\r\n" +
		"@batch=stale :alice!u@h PRIVMSG #omarchy :old\r\n" +
		":omairc!u@h PART :#omarchy\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n" +
		":irc.host BATCH -stale\r\n")
	if len(fixture.handler.batches) != 0 {
		t.Fatalf("batches = %d, want 0", len(fixture.handler.batches))
	}
}

// TestSessionDelayedHistoryBatchAfterPartDoesNotEmit ports
// SessionTest::delayedHistoryBatchAfterPartDoesNotEmit.
func TestSessionDelayedHistoryBatchAfterPartDoesNotEmit(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerWithHistory(t, fixture, true)

	fixture.inject(":omairc!u@h PART :#omarchy\r\n" +
		":irc.host BATCH +stale chathistory #omarchy\r\n" +
		"@batch=stale :alice!u@h PRIVMSG #omarchy :old\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n" +
		":irc.host BATCH -stale\r\n")
	if len(fixture.handler.batches) != 0 {
		t.Fatalf("batches = %d, want 0", len(fixture.handler.batches))
	}
	if !fixture.session.HistoryPending() {
		t.Fatal("history must be pending after the rejoin")
	}

	fixture.inject(":irc.host BATCH +hx chathistory #omarchy\r\n" +
		"@batch=hx :alice!u@h PRIVMSG #omarchy :fresh\r\n" +
		":irc.host BATCH -hx\r\n")
	if len(fixture.handler.batches) != 1 || fixture.handler.batches[0].Target != "#omarchy" ||
		len(fixture.handler.batches[0].Lines) != 1 {
		t.Fatalf("batches = %+v, want one #omarchy batch with one line", fixture.handler.batches)
	}
	if fixture.session.HistoryPending() {
		t.Fatal("history must no longer be pending")
	}
}

// TestSessionLeftoverTaggedHistoryLineAfterPartIsSwallowed ports
// SessionTest::leftoverTaggedHistoryLineAfterPartIsSwallowed.
func TestSessionLeftoverTaggedHistoryLineAfterPartIsSwallowed(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	registerWithHistory(t, fixture, true)
	start := len(fixture.handler.messages)

	fixture.inject(":irc.host BATCH +stale chathistory #omarchy\r\n" +
		"@batch=stale :alice!u@h PRIVMSG #omarchy :old\r\n" +
		":omairc!u@h PART :#omarchy\r\n" +
		"@batch=stale :alice!u@h PRIVMSG #omarchy :late\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n" +
		":irc.host BATCH -stale\r\n")

	if got := messageBodies(fixture.handler.messages[start:]); len(got) != 0 {
		t.Fatalf("privmsgs = %q, want none", got)
	}
	if len(fixture.handler.batches) != 0 {
		t.Fatalf("batches = %d, want 0", len(fixture.handler.batches))
	}
}

// TestSessionChatHistoryBatchWithBatchAloneIsIgnored ports
// SessionTest::chatHistoryBatchWithBatchAloneIsIgnored.
func TestSessionChatHistoryBatchWithBatchAloneIsIgnored(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch\r\n" +
		":server CAP omairc ACK :batch\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n")
	if fixture.session.HistoryPending() {
		t.Fatal("history must not be pending without chathistory")
	}
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

// TestSessionSelfJoinKeepsOpenPlaybackAndDropsStaleChatHistory ports
// SessionTest::selfJoinKeepsOpenPlaybackAndDropsStaleChatHistory.
func TestSessionSelfJoinKeepsOpenPlaybackAndDropsStaleChatHistory(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch chathistory znc.in/playback\r\n" +
		":server CAP omairc ACK :batch chathistory znc.in/playback\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":znc.in BATCH +pb znc.in/playback #omarchy\r\n" +
		"@batch=pb :lena!u@h PRIVMSG #omarchy :buffered\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n" +
		":znc.in BATCH -pb\r\n")

	if len(fixture.handler.batches) != 1 {
		t.Fatalf("batches = %d, want 1", len(fixture.handler.batches))
	}
	batch := fixture.handler.batches[0]
	if batch.Kind != irc.HistoryBouncerPlayback || batch.Target != "#omarchy" ||
		len(batch.Lines) != 1 {
		t.Fatalf("batch = %+v, want a bouncer playback batch for #omarchy", batch)
	}
	if got := batch.Lines[0].Params[len(batch.Lines[0].Params)-1]; got != "buffered" {
		t.Fatalf("line = %q, want buffered", got)
	}

	fixture.inject(":irc.host BATCH +hx chathistory #lab\r\n" +
		"@batch=hx :alice!u@h PRIVMSG #lab :stale\r\n" +
		":omairc!u@h JOIN :#lab\r\n" +
		":irc.host BATCH -hx\r\n")
	if len(fixture.handler.batches) != 1 {
		t.Fatalf("batches = %d, want 1", len(fixture.handler.batches))
	}

	fixture.inject(":omairc!u@h JOIN :#parted\r\n" +
		":znc.in BATCH +gone znc.in/playback #parted\r\n" +
		"@batch=gone :lena!u@h PRIVMSG #parted :lost\r\n" +
		":omairc!u@h PART :#parted\r\n" +
		":znc.in BATCH -gone\r\n")
	if len(fixture.handler.batches) != 1 {
		t.Fatalf("batches = %d, want 1", len(fixture.handler.batches))
	}
}
