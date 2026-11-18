package session

// The CHATHISTORY ledger keeps one phase per folded target, as the Rust session
// does in `client/src/session/history.rs`. A request in flight carries its kind,
// the membership generation it was sent under and, for AFTER only, the page
// count and cursor, so none of them can disagree. A missing entry means the
// target was never asked, or was forgotten by a PART or KICK and may be asked
// again. CHATHISTORY TARGETS is a separate latch, not a phase.

// historyPhase is where one target stands: historySettled, historyInFlight or
// historyExhaustedPhase.
type historyPhase interface{ historyPhase() }

// historySettled means asked, nothing in flight, BEFORE allowed.
type historySettled struct{}

// historyExhaustedPhase means BEFORE stops. It cannot coexist with a request in
// flight.
type historyExhaustedPhase struct{}

// historyInFlight is an outstanding request, sent under generation.
type historyInFlight struct {
	generation int
	request    historyInFlightRequest
}

func (historySettled) historyPhase()        {}
func (historyExhaustedPhase) historyPhase() {}
func (historyInFlight) historyPhase()       {}

// historyInFlightRequest is what a request in flight carries. kind is the only
// mapping to historyRequestKind.
type historyInFlightRequest interface{ kind() historyRequestKind }

type historyLatestRequest struct{}
type historyBeforeRequest struct{}
type historyAfterTailRequest struct{}

// historyAfterRequest is one page of an AFTER chain: its number and the cursor
// it was sent from. They exist only here.
type historyAfterRequest struct {
	pages  int
	cursor historyCursor
}

func (historyLatestRequest) kind() historyRequestKind    { return historyRequestLatest }
func (historyBeforeRequest) kind() historyRequestKind    { return historyRequestBefore }
func (historyAfterRequest) kind() historyRequestKind     { return historyRequestAfter }
func (historyAfterTailRequest) kind() historyRequestKind { return historyRequestAfterTail }

// exhaustsBefore reports whether an empty answer, or a FAIL, to this request
// means older history has run out. An AFTER page or its tail says nothing about
// older lines.
func (kind historyRequestKind) exhaustsBefore() bool {
	return kind == historyRequestLatest || kind == historyRequestBefore
}

// historyInflightRequestLocked is the request outstanding on target for the
// current membership generation.
func (s *Session) historyInflightRequestLocked(target string) (historyInFlightRequest, bool) {
	if target == "" {
		return nil, false
	}
	phase, ok := s.historyPhases[s.foldChannelLocked(target)].(historyInFlight)
	if !ok || phase.generation != s.historyGenerationLocked(target) {
		return nil, false
	}
	return phase.request, true
}

// pendingHistoryKindLocked is the kind of the request channel's next batch
// would answer, as answersPendingHistoryLocked sees it. openBatch captures it.
func (s *Session) pendingHistoryKindLocked(channel string) (historyRequestKind, bool) {
	if !s.answersPendingHistoryLocked(channel) {
		return 0, false
	}
	request, ok := s.historyInflightRequestLocked(channel)
	if !ok {
		return 0, false
	}
	return request.kind(), true
}

// beginHistoryLocked stores request as in flight at the current generation and
// returns the phase it replaced (nil when there was none), for
// rollbackHistoryLocked.
func (s *Session) beginHistoryLocked(key string, request historyInFlightRequest) historyPhase {
	previous := s.historyPhases[key]
	s.historyPhases[key] = historyInFlight{
		generation: s.historyGeneration[key],
		request:    request,
	}
	return previous
}

// rollbackHistoryLocked undoes beginHistoryLocked after a send the frame budget
// refused. A nil previous removes the entry.
func (s *Session) rollbackHistoryLocked(key string, previous historyPhase) {
	if previous == nil {
		delete(s.historyPhases, key)
		return
	}
	s.historyPhases[key] = previous
}

// settleInflightLocked turns an in-flight request into historySettled and
// returns it. Any other phase stays as it is.
func (s *Session) settleInflightLocked(folded string) (historyInFlightRequest, bool) {
	phase, ok := s.historyPhases[folded].(historyInFlight)
	if !ok {
		return nil, false
	}
	s.historyPhases[folded] = historySettled{}
	return phase.request, true
}

// finishHistoryLocked settles the request a closing batch answers. The batch
// counts only when the target's phase is still the in-flight request it opened
// under: same generation, same kind. A late batch that disagrees leaves the
// phase alone.
func (s *Session) finishHistoryLocked(target string, opened int, openedKind historyRequestKind,
	hasOpenedKind bool) (historyInFlightRequest, bool) {
	folded := s.foldChannelLocked(target)
	phase, ok := s.historyPhases[folded].(historyInFlight)
	if !ok || !hasOpenedKind || opened != s.historyGenerationLocked(target) ||
		phase.generation != opened || phase.request.kind() != openedKind {
		return nil, false
	}
	return s.settleInflightLocked(folded)
}

// exhaustHistoryLocked stops BEFORE for a target whose request just settled with
// nothing left.
func (s *Session) exhaustHistoryLocked(folded string) {
	s.historyPhases[folded] = historyExhaustedPhase{}
}
