package session

import (
	"strings"
	"testing"
	"time"
)

func completeHistoryJoin(fixture *sessionFixture) {
	fixture.inject(":server CAP omairc LS :batch chathistory\r\n" +
		":server CAP omairc ACK :batch chathistory\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":server 376 omairc :End of MOTD\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n")
}

func finishLatestBatch(fixture *sessionFixture) {
	fixture.inject(":irc.host BATCH +hx chathistory #omarchy\r\n" +
		"@batch=hx;time=2011-10-19T16:40:51.620Z;msgid=old :alice!u@h PRIVMSG #omarchy :older\r\n" +
		":irc.host BATCH -hx\r\n")
}

func TestSessionPageUpAtTopSendsBeforeWithOldestMsgid(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("RequestOlderHistory must succeed")
	}
	want := "CHATHISTORY BEFORE #omarchy msgid=old 100\r\n"
	if !fixture.wrote(want) {
		t.Fatalf("frames = %q, want %q", fixture.lastFrame(), want)
	}
}

func TestSessionBeforeWithoutMsgidUsesTimestamp(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	when := time.Date(2011, 10, 19, 16, 40, 51, 620000000, time.UTC)
	if !fixture.session.RequestOlderHistory("#omarchy", "", when) {
		t.Fatal("RequestOlderHistory must succeed")
	}
	want := "CHATHISTORY BEFORE #omarchy timestamp=2011-10-19T16:40:51.620Z 100\r\n"
	if !fixture.wrote(want) {
		t.Fatalf("frames = %q, want %q", fixture.lastFrame(), want)
	}
}

func TestSessionInflightSecondBeforeDoesNotSend(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("first request must succeed")
	}
	if fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("second request while pending must fail")
	}
	count := 0
	for _, frame := range fixture.frames() {
		if strings.Contains(frame, "CHATHISTORY BEFORE") {
			count++
		}
	}
	if count != 1 {
		t.Fatalf("BEFORE count = %d, want 1", count)
	}
}

func TestSessionEmptyBatchStopsFurtherBeforeUntilRejoin(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("first BEFORE must succeed")
	}
	fixture.inject(":irc.host BATCH +empty chathistory #omarchy\r\n" +
		":irc.host BATCH -empty\r\n")
	if fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE after empty batch must fail")
	}
	if len(fixture.handler.errors) > 0 {
		t.Fatalf("unexpected errors: %+v", fixture.handler.errors)
	}
	if fixture.session.State() != StateRegistered {
		t.Fatalf("state = %v, want registered", fixture.session.State())
	}
	fixture.inject(":omairc!u@h PART #omarchy\r\n:omairc!u@h JOIN :#omarchy\r\n")
	fixture.inject(":irc.host BATCH +hx2 chathistory #omarchy\r\n" +
		"@batch=hx2;msgid=older2 :alice!u@h PRIVMSG #omarchy :again\r\n" +
		":irc.host BATCH -hx2\r\n")
	if !fixture.session.RequestOlderHistory("#omarchy", "older2", time.Time{}) {
		t.Fatal("BEFORE after rejoin must succeed")
	}
}

func TestSessionFailChatHistoryStopsBeforeWithoutDisconnect(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("first BEFORE must succeed")
	}
	fixture.inject(":server FAIL CHATHISTORY INVALID_TARGET BEFORE #omarchy :no\r\n")
	if fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE after FAIL must fail")
	}
	if len(fixture.handler.errors) > 0 {
		t.Fatalf("unexpected errors: %+v", fixture.handler.errors)
	}
}

func TestSessionJoinStillSendsExactlyOneLatest(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	latest := 0
	for _, frame := range fixture.frames() {
		if strings.Contains(frame, "CHATHISTORY LATEST") {
			latest++
		}
	}
	if latest != 1 {
		t.Fatalf("LATEST count = %d, want 1", latest)
	}
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE must be allowed after LATEST")
	}
}

func TestSessionNoCapPageUpSendsNothing(t *testing.T) {
	config := historyConfig()
	config.AutojoinChannels = nil
	fixture := newSessionFixture(t, config)
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch\r\n" +
		":server CAP omairc ACK :batch\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":server 376 omairc :End of MOTD\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n")
	if fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE without chathistory must fail")
	}
	for _, frame := range fixture.frames() {
		if strings.Contains(frame, "CHATHISTORY") {
			t.Fatalf("unexpected CHATHISTORY frame: %q", frame)
		}
	}
}

func TestSessionBeforeBatchSetsOlderPage(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE must succeed")
	}
	fixture.inject(":irc.host BATCH +older chathistory #omarchy\r\n" +
		"@batch=older;msgid=page :bob!u@h PRIVMSG #omarchy :backlog\r\n" +
		":irc.host BATCH -older\r\n")
	if len(fixture.handler.batches) != 2 {
		t.Fatalf("batches = %d, want 2", len(fixture.handler.batches))
	}
	if !fixture.handler.batches[1].OlderPage {
		t.Fatal("BEFORE batch must set OlderPage")
	}
}

func TestSessionBeforeBatchIsDelivered(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE must succeed")
	}
	fixture.inject(":irc.host BATCH +older chathistory #omarchy\r\n" +
		"@batch=older;msgid=page :bob!u@h PRIVMSG #omarchy :backlog\r\n" +
		":irc.host BATCH -older\r\n")
	if len(fixture.handler.batches) != 2 {
		t.Fatalf("batches = %d, want 2", len(fixture.handler.batches))
	}
}

func TestSessionBeforeIgnoresStaleHistoryPendingGeneration(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	fixture.session.locked(func() {
		folded := fixture.session.foldChannelLocked("#omarchy")
		fixture.session.historyPhases[folded] = historyInFlight{generation: 0, request: historyBeforeRequest{}}
		fixture.session.historyGeneration[folded] = 2
	})
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE must ignore a stale historyPending generation")
	}
}

func TestSessionReconnectClearsHistoryPendingBefore(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE must succeed")
	}
	fixture.session.locked(func() {
		phase, ok := fixture.session.historyPhases[fixture.session.foldChannelLocked("#omarchy")].(historyInFlight)
		if !ok {
			t.Fatal("BEFORE must be in flight while the page is outstanding")
		}
		if _, before := phase.request.(historyBeforeRequest); !before {
			t.Fatalf("request = %T, want historyBeforeRequest", phase.request)
		}
	})
	fixture.inject(":server ERROR :gone\r\n")
	if fixture.session.State() != StateReconnecting {
		t.Fatalf("state = %v, want Reconnecting", fixture.session.State())
	}
	fixture.fireReconnect()
	fixture.transport.CompleteConnect()
	fixture.inject(":server CAP omairc LS :batch chathistory\r\n" +
		":server CAP omairc ACK :batch chathistory\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":server 376 omairc :End of MOTD\r\n")
	fixture.session.locked(func() {
		if len(fixture.session.historyPhases) != 0 {
			t.Fatalf("historyPhases = %v, want empty after reconnect", fixture.session.historyPhases)
		}
	})
	fixture.inject(":irc.host BATCH +unsol chathistory #omarchy\r\n" +
		"@batch=unsol;msgid=fresh :alice!u@h PRIVMSG #omarchy :latest\r\n" +
		":irc.host BATCH -unsol\r\n")
	if len(fixture.handler.batches) == 0 {
		t.Fatal("unsolicited batch must be delivered")
	}
	last := fixture.handler.batches[len(fixture.handler.batches)-1]
	if last.OlderPage {
		t.Fatal("unsolicited LATEST after reconnect must not set OlderPage")
	}
}

func TestSessionBeforeBlockedUntilLatestOnChannel(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch chathistory\r\n" +
		":server CAP omairc ACK :batch chathistory\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":server 376 omairc :End of MOTD\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n")
	if fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE before LATEST batch finishes must fail when pending")
	}
}

// Behavior pins for the one-phase-per-target ledger (#467). They use the public
// session API only, so they hold before and after the ledger refactor.

func TestSessionLateBatchFromAnOlderGenerationDoesNotFinishTheNewRequest(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	// The LATEST batch is open when a second self-JOIN bumps the generation and
	// BEFORE is sent under the new one.
	fixture.inject(":irc.host BATCH +late chathistory #omarchy\r\n" +
		"@batch=late;msgid=old :alice!u@h PRIVMSG #omarchy :older\r\n")
	fixture.inject(":omairc!u@h JOIN :#omarchy\r\n")
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE under the new generation must succeed")
	}
	if !fixture.session.HistoryInflightFor("#omarchy") {
		t.Fatal("BEFORE must be in flight")
	}
	fixture.inject(":irc.host BATCH -late\r\n")
	if !fixture.session.HistoryInflightFor("#omarchy") {
		t.Fatal("the late batch settled the new request")
	}
	if len(fixture.handler.batches) != 0 {
		t.Fatalf("late batch was delivered: %d batches", len(fixture.handler.batches))
	}
	if fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("the new BEFORE is still in flight")
	}
}

func TestSessionGenerationBumpClearsInflightPhase(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE must succeed")
	}
	if !fixture.session.HistoryInflightFor("#omarchy") {
		t.Fatal("BEFORE must be in flight")
	}
	fixture.inject(":omairc!u@h JOIN :#omarchy\r\n")
	if fixture.session.HistoryInflightFor("#omarchy") {
		t.Fatal("a membership change left the request in flight")
	}
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE must be possible again after the bump")
	}
	if count := strings.Count(strings.Join(fixture.frames(), ""), "CHATHISTORY BEFORE"); count != 2 {
		t.Fatalf("BEFORE count = %d, want 2", count)
	}
}

func TestSessionStaleBatchAfterPartIsDiscardedAndPartForgetsTheTarget(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	fixture.inject(":irc.host BATCH +gone chathistory #omarchy\r\n" +
		"@batch=gone;msgid=old :alice!u@h PRIVMSG #omarchy :older\r\n")
	fixture.inject(":omairc!u@h PART #omarchy\r\n")
	fixture.inject(":irc.host BATCH -gone\r\n")
	if len(fixture.handler.batches) != 0 {
		t.Fatalf("stale batch was delivered: %d batches", len(fixture.handler.batches))
	}
	if fixture.session.HistoryInflightFor("#omarchy") {
		t.Fatal("PART left a request in flight")
	}
	if count := strings.Count(strings.Join(fixture.frames(), ""), "CHATHISTORY LATEST"); count != 1 {
		t.Fatalf("LATEST count = %d, want 1", count)
	}
	fixture.inject(":omairc!u@h JOIN :#omarchy\r\n")
	if count := strings.Count(strings.Join(fixture.frames(), ""), "CHATHISTORY LATEST"); count != 2 {
		t.Fatalf("LATEST count after rejoin = %d, want 2 (eligible again)", count)
	}
}

func TestSessionTargetsLatchIsIndependentOfChannelPhases(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	from := time.Date(2024, 3, 9, 15, 59, 59, 620000000, time.UTC)
	until := time.Date(2026, 10, 4, 12, 0, 10, 0, time.UTC)
	if !fixture.session.RequestHistoryTargets(from, until) {
		t.Fatal("first TARGETS must succeed")
	}
	fixture.inject(":omairc!u@h PART #omarchy\r\n:omairc!u@h JOIN :#omarchy\r\n")
	if fixture.session.RequestHistoryTargets(from, until) {
		t.Fatal("a membership change cleared the TARGETS latch")
	}
	fixture.inject(":irc.host BATCH +t draft/chathistory-targets\r\n" +
		":irc.host BATCH -t\r\n")
	if !fixture.session.RequestHistoryTargets(from, until) {
		t.Fatal("the TARGETS batch must release the latch")
	}
}

func TestSessionAfterOnAnExhaustedTargetClearsExhaustion(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE must succeed")
	}
	fixture.inject(":irc.host BATCH +empty chathistory #omarchy\r\n" +
		":irc.host BATCH -empty\r\n")
	if fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("an empty BEFORE exhausts the target")
	}
	when := time.Date(2024, 3, 9, 16, 0, 0, 620000000, time.UTC)
	if !fixture.session.RequestHistoryAfter("#omarchy", when) {
		t.Fatal("AFTER must be sent")
	}
	fixture.inject(":irc.host BATCH +gap chathistory #omarchy\r\n" +
		"@batch=gap;time=2024-03-09T16:00:01.000Z;msgid=m1 :bob!u@h PRIVMSG #omarchy :line\r\n" +
		":irc.host BATCH -gap\r\n")
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("AFTER on an exhausted target clears the exhaustion")
	}
}
