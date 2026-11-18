package session

// connectionLifecycle holds the connection's lifecycle decisions: why the
// transport is going away, how many reconnects have been scheduled, and which
// retryable error kinds were already reported. It mirrors
// client/src/session/connection.rs, which replaced the session's two independent
// disconnect flags with one intent. session.go applies the outcomes; nothing
// here touches the transport, the timers or the state machine.
type connectionLifecycle struct {
	intent disconnectIntent
	// reconnectAttempt counts reconnects scheduled since the last welcome, stop or start.
	reconnectAttempt int
	// reportedRetryErrors packs one dedup bit per ErrorKind, in reportBit's order.
	reportedRetryErrors uint32
}

// disconnectIntent says why the transport is going away, which decides what its
// disconnect means. A stop or a fatal failure outranks a request to reconnect, so
// a reconnect requested after either one is not recorded.
type disconnectIntent uint8

const (
	// intentUnexpected is the zero value. Nothing asked for the disconnect, so the
	// session fails with a network error.
	intentUnexpected disconnectIntent = iota
	// intentStopping means the user stopped the session, or a fatal failure ended
	// it. The disconnect settles the session and nothing reconnects.
	intentStopping
	// intentReconnecting means a retryable failure shut the transport down. The
	// disconnect schedules a reconnect, then the intent returns to intentUnexpected.
	intentReconnecting
)

// disconnectOutcome is what the session does when the transport reports a disconnect.
type disconnectOutcome uint8

const (
	// outcomeScheduleReconnect schedules the reconnect backoff.
	outcomeScheduleReconnect disconnectOutcome = iota
	// outcomeSettleIdle moves a stopping session that is closing to Idle.
	outcomeSettleIdle
	// outcomeFailUnexpected means the server closed the connection on its own, so
	// the session fails with a network error.
	outcomeFailUnexpected
	// outcomeFinishStsUpgrade means the plaintext socket closed for an STS upgrade,
	// so the session connects again at once on the upgraded port.
	outcomeFinishStsUpgrade
	// outcomeIgnore is a stopping session that is not closing, such as one that
	// already failed. It keeps its state.
	outcomeIgnore
)

func (o disconnectOutcome) String() string {
	switch o {
	case outcomeScheduleReconnect:
		return "ScheduleReconnect"
	case outcomeSettleIdle:
		return "SettleIdle"
	case outcomeFailUnexpected:
		return "FailUnexpected"
	case outcomeFinishStsUpgrade:
		return "FinishStsUpgrade"
	case outcomeIgnore:
		return "Ignore"
	}
	return "Unknown"
}

// beginStart is a fresh start: nothing pending and no retries counted.
func (c *connectionLifecycle) beginStart() {
	c.intent = intentUnexpected
	c.resetRetries()
}

// beginStop is the user's stop. The next disconnect settles the session, and
// nothing reconnects after it.
func (c *connectionLifecycle) beginStop() {
	c.intent = intentStopping
	c.resetRetries()
}

// reconnectOnDisconnect records that a retryable failure shut the live transport
// down, so its disconnect should reconnect. A stop or a fatal failure already in
// effect keeps its intent.
func (c *connectionLifecycle) reconnectOnDisconnect() {
	if c.intent == intentUnexpected {
		c.intent = intentReconnecting
	}
}

// endOnDisconnect records that a fatal failure shut the live transport down. Its
// disconnect ends the session and no reconnect follows, even one already pending.
func (c *connectionLifecycle) endOnDisconnect() {
	c.intent = intentStopping
}

// isStopping reports whether the connection is ending, so transport errors are
// not reported.
func (c *connectionLifecycle) isStopping() bool {
	return c.intent == intentStopping
}

// onDisconnect decides what a transport disconnect means while the session is in
// state. A pending reconnect is spent here, so the next disconnect is unexpected
// again.
func (c *connectionLifecycle) onDisconnect(state SessionState) disconnectOutcome {
	switch c.intent {
	case intentReconnecting:
		c.intent = intentUnexpected
		return outcomeScheduleReconnect
	case intentStopping:
		if state == StateClosing {
			return outcomeSettleIdle
		}
		return outcomeIgnore
	}
	if state == StateStsUpgrading {
		return outcomeFinishStsUpgrade
	}
	return outcomeFailUnexpected
}

// firstReportOf records that kind is reported as retryable. It is true only the
// first time since the last reset, so a flapping link reports each kind once.
func (c *connectionLifecycle) firstReportOf(kind ErrorKind) bool {
	bit := reportBit(kind)
	if c.reportedRetryErrors&bit != 0 {
		return false
	}
	c.reportedRetryErrors |= bit
	return true
}

// countScheduledReconnect counts one more scheduled reconnect and returns the new
// attempt number.
func (c *connectionLifecycle) countScheduledReconnect() int {
	c.reconnectAttempt++
	return c.reconnectAttempt
}

// resetRetries forgets the retry count and the reported kinds. A welcome (001)
// does this, and so do beginStart and beginStop.
func (c *connectionLifecycle) resetRetries() {
	c.reconnectAttempt = 0
	c.reportedRetryErrors = 0
}

// reportBit is the dedup bit of one error kind, by its position in ErrorKind.
func reportBit(kind ErrorKind) uint32 {
	return uint32(1) << uint(kind)
}
