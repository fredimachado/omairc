package session

import (
	"reflect"
	"strings"
	"testing"
	"time"
)

// Ledger-level pins for the one-phase-per-target ledger (#467). The Rust twins
// are in `client/src/session/tests/history_phase.rs`. A target cannot be pending
// without a kind, or exhausted while in flight, because the phase types have no
// value for either, so those need no runtime test.

func omarchyPhase(fixture *sessionFixture) historyPhase {
	var phase historyPhase
	fixture.session.locked(func() {
		phase = fixture.session.historyPhases[fixture.session.foldChannelLocked("#omarchy")]
	})
	return phase
}

func TestSessionARequestInFlightCarriesOnlyItsOwnPayload(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	if got := omarchyPhase(fixture); got != (historySettled{}) {
		t.Fatalf("phase = %#v, want settled", got)
	}

	when := time.Date(2024, 3, 9, 16, 0, 0, 620000000, time.UTC)
	if !fixture.session.RequestHistoryAfter("#omarchy", when) {
		t.Fatal("AFTER must be sent")
	}
	want := historyInFlight{
		generation: 1,
		request:    historyAfterRequest{pages: 1, cursor: historyCursor{when: when}},
	}
	if got := omarchyPhase(fixture); !reflect.DeepEqual(got, want) {
		t.Fatalf("phase = %#v, want %#v", got, want)
	}
	// A short page settles it and drops the pages and cursor with the phase.
	afterPage(fixture, "one", "m1")
	if got := omarchyPhase(fixture); got != (historySettled{}) {
		t.Fatalf("phase = %#v, want settled", got)
	}
}

func TestSessionExhaustedTargetRefusesBeforeAndKeepsItsPhase(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE must succeed")
	}
	fixture.inject(":irc.host BATCH +empty chathistory #omarchy\r\n" +
		":irc.host BATCH -empty\r\n")
	if got := omarchyPhase(fixture); got != (historyExhaustedPhase{}) {
		t.Fatalf("phase = %#v, want exhausted", got)
	}
	if fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE on an exhausted target must be refused")
	}
	if got := omarchyPhase(fixture); got != (historyExhaustedPhase{}) {
		t.Fatalf("phase = %#v, want exhausted", got)
	}
	if count := strings.Count(strings.Join(fixture.frames(), ""), "CHATHISTORY BEFORE"); count != 1 {
		t.Fatalf("BEFORE count = %d, want 1", count)
	}
}

func TestSessionBatchThatDisagreesOnKindDoesNotFinishTheRequest(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	completeHistoryJoin(fixture)
	finishLatestBatch(fixture)
	if !fixture.session.RequestOlderHistory("#omarchy", "old", time.Time{}) {
		t.Fatal("BEFORE must succeed")
	}
	// The BEFORE batch opens, then the request it answered is replaced at the
	// same generation, which the wire alone cannot express.
	fixture.inject(":irc.host BATCH +older chathistory #omarchy\r\n" +
		"@batch=older;msgid=page :bob!u@h PRIVMSG #omarchy :backlog\r\n")
	replacement := historyInFlight{generation: 1, request: historyLatestRequest{}}
	fixture.session.locked(func() {
		fixture.session.historyPhases[fixture.session.foldChannelLocked("#omarchy")] = replacement
	})
	fixture.inject(":irc.host BATCH -older\r\n")
	if got := omarchyPhase(fixture); got != historyPhase(replacement) {
		t.Fatalf("a BEFORE batch settled a LATEST request: phase = %#v", got)
	}
	if !fixture.session.HistoryInflightFor("#omarchy") {
		t.Fatal("the LATEST request must still be in flight")
	}
}
