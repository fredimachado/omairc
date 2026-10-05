package session

import (
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

func TestSessionSelfJoinResumesChatHistoryAfter(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	when := time.Date(2024, 3, 9, 16, 0, 0, 620000000, time.UTC)
	fixture.session.SetChatHistoryResume(func(target string) (time.Time, bool) {
		if target == "#omarchy" {
			return when, true
		}
		return time.Time{}, false
	})
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch chathistory\r\n" +
		":server CAP omairc ACK :batch chathistory\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n")
	want := "CHATHISTORY AFTER #omarchy timestamp=2024-03-09T16:00:00.620Z 100\r\n"
	if !fixture.wrote(want) {
		t.Fatalf("frames = %q, want %q", fixture.lastFrame(), want)
	}
	if fixture.wrote("CHATHISTORY LATEST #omarchy * 100\r\n") {
		t.Fatal("resume sent LATEST")
	}
}

func TestSessionEmptyAfterDoesNotExhaustBefore(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	when := time.Date(2024, 3, 9, 16, 0, 0, 620000000, time.UTC)
	fixture.session.SetChatHistoryResume(func(string) (time.Time, bool) {
		return when, true
	})
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch chathistory\r\n" +
		":server CAP omairc ACK :batch chathistory\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n" +
		":irc.host BATCH +empty chathistory #omarchy\r\n" +
		":irc.host BATCH -empty\r\n")
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE after an empty AFTER must succeed")
	}
	want := "CHATHISTORY BEFORE #omarchy msgid=old 100\r\n"
	if !fixture.wrote(want) {
		t.Fatalf("frames = %q, want %q", fixture.frames(), want)
	}
}

func TestSessionFailAfterDoesNotExhaustBefore(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	when := time.Date(2024, 3, 9, 16, 0, 0, 620000000, time.UTC)
	fixture.session.SetChatHistoryResume(func(string) (time.Time, bool) {
		return when, true
	})
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch chathistory\r\n" +
		":server CAP omairc ACK :batch chathistory\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n" +
		":server FAIL CHATHISTORY INVALID_TARGET AFTER #omarchy :no\r\n")
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE after FAIL AFTER must succeed")
	}
	want := "CHATHISTORY BEFORE #omarchy msgid=old 100\r\n"
	if !fixture.wrote(want) {
		t.Fatalf("frames = %q, want %q", fixture.frames(), want)
	}
	if fixture.session.State() != StateRegistered {
		t.Fatalf("state = %v, want registered", fixture.session.State())
	}
}

func resumeOmarchy(fixture *sessionFixture, when time.Time) {
	fixture.session.SetChatHistoryResume(func(target string) (time.Time, bool) {
		if target == "#omarchy" {
			return when, true
		}
		return time.Time{}, false
	})
}

func TestSessionAfterChainsThreePagesThenLatest(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	when := time.Date(2024, 3, 9, 16, 0, 0, 620000000, time.UTC)
	fixture.session.SetHistoryAfterPageCap(3)
	resumeOmarchy(fixture, when)
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch chathistory\r\n" +
		":server CAP omairc ACK :batch chathistory\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":server 005 omairc CHATHISTORY=1 :are supported\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n")
	page := func(ref, msgid string) {
		fixture.inject(":irc.host BATCH +" + ref + " chathistory #omarchy\r\n" +
			"@batch=" + ref + ";time=2024-03-09T16:00:01.000Z;msgid=" + msgid + " :bob!u@h PRIVMSG #omarchy :line\r\n" +
			":irc.host BATCH -" + ref + "\r\n")
	}
	page("p1", "m1")
	page("p2", "m2")
	page("p3", "m3")
	frames := fixture.frames()
	if count := strings.Count(strings.Join(frames, ""), "CHATHISTORY AFTER "); count != 3 {
		t.Fatalf("AFTER count = %d, frames %q", count, frames)
	}
	wantLatest := "CHATHISTORY LATEST #omarchy msgid=m3 1\r\n"
	if !fixture.wrote(wantLatest) {
		t.Fatalf("frames = %q, want %q", frames, wantLatest)
	}
	fixture.inject(":irc.host BATCH +tail chathistory #omarchy\r\n" +
		":irc.host BATCH -tail\r\n")
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE after the cap LATEST must succeed")
	}
	if !fixture.wrote("CHATHISTORY BEFORE #omarchy msgid=old 1\r\n") {
		t.Fatalf("frames = %q", fixture.frames())
	}
}

func TestSessionAfterShortBatchAndEndTagStop(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	when := time.Date(2024, 3, 9, 16, 0, 0, 620000000, time.UTC)
	resumeOmarchy(fixture, when)
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch chathistory\r\n" +
		":server CAP omairc ACK :batch chathistory\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n" +
		":irc.host BATCH +short chathistory #omarchy\r\n" +
		"@batch=short;time=2024-03-09T16:00:01.000Z;msgid=one :bob!u@h PRIVMSG #omarchy :line\r\n" +
		":irc.host BATCH -short\r\n")
	if strings.Count(strings.Join(fixture.frames(), ""), "CHATHISTORY AFTER ") != 1 {
		t.Fatalf("short batch chained: %q", fixture.frames())
	}

	ended := newSessionFixture(t, historyConfig())
	ended.session.SetHistoryAfterPageCap(10)
	resumeOmarchy(ended, when)
	ended.connectTLS()
	ended.inject(":server CAP omairc LS :batch chathistory\r\n" +
		":server CAP omairc ACK :batch chathistory\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":server 005 omairc CHATHISTORY=1 :are supported\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n" +
		"@draft/chathistory-end :irc.host BATCH +end chathistory #omarchy\r\n" +
		"@batch=end;time=2024-03-09T16:00:01.000Z;msgid=only :bob!u@h PRIVMSG #omarchy :line\r\n" +
		":irc.host BATCH -end\r\n")
	if strings.Count(strings.Join(ended.frames(), ""), "CHATHISTORY AFTER ") != 1 {
		t.Fatalf("end tag chained: %q", ended.frames())
	}
}

func TestSessionAfterSameTimeUsesMsgid(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	when := time.Date(2024, 3, 9, 16, 0, 0, 620000000, time.UTC)
	resumeOmarchy(fixture, when)
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch chathistory\r\n" +
		":server CAP omairc ACK :batch chathistory\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":server 005 omairc CHATHISTORY=2 :are supported\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n" +
		":irc.host BATCH +p chathistory #omarchy\r\n" +
		"@batch=p;time=2024-03-09T16:00:01.000Z;msgid=first :a!u@h PRIVMSG #omarchy :one\r\n" +
		"@batch=p;time=2024-03-09T16:00:01.000Z;msgid=second :b!u@h PRIVMSG #omarchy :two\r\n" +
		":irc.host BATCH -p\r\n")
	want := "CHATHISTORY AFTER #omarchy msgid=second 2\r\n"
	if !fixture.wrote(want) {
		t.Fatalf("frames = %q, want %q", fixture.frames(), want)
	}
}

func TestSessionAfterSendFailureDoesNotStick(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	when := time.Date(2024, 3, 9, 16, 0, 0, 620000000, time.UTC)
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch chathistory\r\n" +
		":server CAP omairc ACK :batch chathistory\r\n")
	if fixture.session.RequestHistoryAfter("#omarchy", when) {
		t.Fatal("AFTER before registration must fail")
	}
	fixture.inject(":server 001 omairc :Welcome\r\n")
	if !fixture.session.RequestHistoryAfter("#omarchy", when) {
		t.Fatal("AFTER after a failed send must succeed")
	}
	want := "CHATHISTORY AFTER #omarchy timestamp=2024-03-09T16:00:00.620Z 100\r\n"
	if !fixture.wrote(want) {
		t.Fatalf("frames = %q, want %q", fixture.frames(), want)
	}
}

func TestSessionHistoryTargetsCommandAndBatch(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch chathistory\r\n" +
		":server CAP omairc ACK :batch chathistory\r\n" +
		":server 001 omairc :Welcome\r\n")
	from := time.Date(2024, 3, 9, 15, 59, 59, 620000000, time.UTC)
	until := time.Date(2026, 10, 4, 12, 0, 10, 0, time.UTC)
	if !fixture.session.RequestHistoryTargets(from, until) {
		t.Fatal("RequestHistoryTargets")
	}
	want := "CHATHISTORY TARGETS timestamp=2024-03-09T15:59:59.620Z timestamp=2026-10-04T12:00:10.000Z 100\r\n"
	if !fixture.wrote(want) {
		t.Fatalf("frames = %q, want %q", fixture.frames(), want)
	}
	fixture.inject(":irc.host BATCH +t draft/chathistory-targets\r\n" +
		"@batch=t :irc.host CHATHISTORY TARGETS lena 2024-03-09T16:00:00.620Z\r\n" +
		"@batch=t :irc.host CHATHISTORY TARGETS alice timestamp=2024-03-09T16:00:01.000Z\r\n" +
		":irc.host BATCH -t\r\n")
	if len(fixture.handler.batches) != 1 {
		t.Fatalf("batches = %d, want 1", len(fixture.handler.batches))
	}
	batch := fixture.handler.batches[0]
	if batch.Kind != irc.HistoryTargets || len(batch.Targets) != 2 || len(batch.Lines) != 0 {
		t.Fatalf("batch = %+v", batch)
	}
	if batch.Targets[0].Name != "lena" || !batch.Targets[0].Latest.Equal(time.Date(2024, 3, 9, 16, 0, 0, 620000000, time.UTC)) {
		t.Fatalf("lena = %+v", batch.Targets[0])
	}
	if batch.Targets[1].Name != "alice" {
		t.Fatalf("alice = %+v", batch.Targets[1])
	}
}

func TestSessionFailTargetsCanBeAskedAgain(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch chathistory\r\n" +
		":server CAP omairc ACK :batch chathistory\r\n" +
		":server 001 omairc :Welcome\r\n")
	from := time.Date(2024, 3, 9, 15, 59, 59, 620000000, time.UTC)
	until := time.Date(2026, 10, 4, 12, 0, 10, 0, time.UTC)
	if !fixture.session.RequestHistoryTargets(from, until) {
		t.Fatal("first TARGETS")
	}
	fixture.inject(":server FAIL CHATHISTORY MESSAGE_ERROR TARGETS :no\r\n")
	if !fixture.session.RequestHistoryTargets(from, until) {
		t.Fatal("TARGETS after FAIL must succeed")
	}
	count := 0
	for _, frame := range fixture.frames() {
		if strings.Contains(frame, "CHATHISTORY TARGETS ") {
			count++
		}
	}
	if count != 2 {
		t.Fatalf("TARGETS count = %d, want 2", count)
	}
}
