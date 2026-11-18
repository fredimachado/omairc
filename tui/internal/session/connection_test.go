package session

import "testing"

// TestConnectionStartIsUnexpected.
// client/src/session/connection.rs::start_expects_an_unexpected_disconnect.
func TestConnectionStartIsUnexpected(t *testing.T) {
	var connection connectionLifecycle
	connection.beginStart()
	if connection.isStopping() {
		t.Fatal("a fresh start must not be stopping")
	}
	if got := connection.onDisconnect(StateRegistered); got != outcomeFailUnexpected {
		t.Fatalf("outcome = %v, want FailUnexpected", got)
	}
}

// TestConnectionStopThenStartForgetsStop.
// client/src/session/connection.rs::start_clears_a_stop_and_the_retry_counts.
func TestConnectionStopThenStartForgetsStop(t *testing.T) {
	var connection connectionLifecycle
	connection.beginStop()
	connection.countScheduledReconnect()
	connection.firstReportOf(ErrorNetwork)

	connection.beginStart()

	if connection.isStopping() {
		t.Fatal("a start must clear the stop")
	}
	if connection.reconnectAttempt != 0 {
		t.Fatalf("attempt = %d, want 0", connection.reconnectAttempt)
	}
	if !connection.firstReportOf(ErrorNetwork) {
		t.Fatal("a start must clear the once-per-kind report")
	}
}

// TestConnectionStopSettlesOnlyWhileClosing.
// client/src/session/connection.rs::stop_settles_a_closing_session_and_keeps_stopping and
// stop_ignores_a_disconnect_that_is_not_closing.
func TestConnectionStopSettlesOnlyWhileClosing(t *testing.T) {
	var connection connectionLifecycle
	connection.beginStop()
	if got := connection.onDisconnect(StateClosing); got != outcomeSettleIdle {
		t.Fatalf("closing outcome = %v, want SettleIdle", got)
	}
	if !connection.isStopping() {
		t.Fatal("a stop lasts until the next start")
	}

	connection.beginStop()
	if got := connection.onDisconnect(StateFailed); got != outcomeIgnore {
		t.Fatalf("failed outcome = %v, want Ignore", got)
	}
	if !connection.isStopping() {
		t.Fatal("ignoring a disconnect must keep the stop")
	}
}

// TestConnectionReconnectSpentAfterOneDisconnect.
// client/src/session/connection.rs::reconnect_request_turns_a_disconnect_into_one_reconnect.
func TestConnectionReconnectSpentAfterOneDisconnect(t *testing.T) {
	var connection connectionLifecycle
	connection.beginStart()
	connection.reconnectOnDisconnect()

	if got := connection.onDisconnect(StateReconnecting); got != outcomeScheduleReconnect {
		t.Fatalf("first outcome = %v, want ScheduleReconnect", got)
	}
	if got := connection.onDisconnect(StateReconnecting); got != outcomeFailUnexpected {
		t.Fatalf("second outcome = %v, want FailUnexpected (the request is spent)", got)
	}
}

// TestConnectionReconnectRequestedTwiceSchedulesOnce.
// client/src/session/connection.rs::repeated_reconnect_requests_coalesce.
func TestConnectionReconnectRequestedTwiceSchedulesOnce(t *testing.T) {
	var connection connectionLifecycle
	connection.beginStart()
	connection.reconnectOnDisconnect()
	connection.reconnectOnDisconnect()

	if got := connection.onDisconnect(StateConnecting); got != outcomeScheduleReconnect {
		t.Fatalf("first outcome = %v, want ScheduleReconnect", got)
	}
	if got := connection.onDisconnect(StateConnecting); got != outcomeFailUnexpected {
		t.Fatalf("second outcome = %v, want FailUnexpected", got)
	}
}

// TestConnectionStopOutranksLaterReconnectRequest.
// client/src/session/connection.rs::reconnect_request_does_not_override_a_stop.
func TestConnectionStopOutranksLaterReconnectRequest(t *testing.T) {
	var connection connectionLifecycle
	connection.beginStop()
	connection.reconnectOnDisconnect()

	if !connection.isStopping() {
		t.Fatal("a reconnect request must not replace a stop")
	}
	if got := connection.onDisconnect(StateClosing); got != outcomeSettleIdle {
		t.Fatalf("outcome = %v, want SettleIdle", got)
	}
}

// TestConnectionEndOnDisconnectDropsPendingReconnect.
// client/src/session/connection.rs::fatal_failure_overrides_a_pending_reconnect.
func TestConnectionEndOnDisconnectDropsPendingReconnect(t *testing.T) {
	var connection connectionLifecycle
	connection.beginStart()
	connection.reconnectOnDisconnect()
	connection.endOnDisconnect()

	if !connection.isStopping() {
		t.Fatal("a fatal failure must stop the session")
	}
	if got := connection.onDisconnect(StateFailed); got != outcomeIgnore {
		t.Fatalf("outcome = %v, want Ignore (no reconnect after a fatal failure)", got)
	}
}

// TestConnectionFatalSettlesClosingSession.
// client/src/session/connection.rs::fatal_failure_settles_a_closing_session.
func TestConnectionFatalSettlesClosingSession(t *testing.T) {
	var connection connectionLifecycle
	connection.beginStart()
	connection.endOnDisconnect()
	if got := connection.onDisconnect(StateClosing); got != outcomeSettleIdle {
		t.Fatalf("outcome = %v, want SettleIdle", got)
	}
}

// TestConnectionStsUpgradeFinishesOnceThenIntentDecides.
// client/src/session/connection.rs::an_sts_upgrade_disconnect_reconnects_at_once and
// unexpected_disconnect_keeps_no_intent.
func TestConnectionStsUpgradeFinishesOnceThenIntentDecides(t *testing.T) {
	var connection connectionLifecycle
	connection.beginStart()
	if got := connection.onDisconnect(StateStsUpgrading); got != outcomeFinishStsUpgrade {
		t.Fatalf("upgrade outcome = %v, want FinishStsUpgrade", got)
	}
	if connection.isStopping() {
		t.Fatal("an STS upgrade must not stop the session")
	}
	if got := connection.onDisconnect(StateRegistered); got != outcomeFailUnexpected {
		t.Fatalf("later outcome = %v, want FailUnexpected", got)
	}
}

// TestConnectionReconnectOrStopOutranksStsUpgrade.
// client/src/session/connection.rs::a_reconnect_request_or_a_stop_outranks_an_sts_upgrade.
func TestConnectionReconnectOrStopOutranksStsUpgrade(t *testing.T) {
	var connection connectionLifecycle
	connection.beginStart()
	connection.reconnectOnDisconnect()
	if got := connection.onDisconnect(StateStsUpgrading); got != outcomeScheduleReconnect {
		t.Fatalf("reconnect outcome = %v, want ScheduleReconnect", got)
	}

	connection.beginStop()
	if got := connection.onDisconnect(StateStsUpgrading); got != outcomeIgnore {
		t.Fatalf("stop outcome = %v, want Ignore", got)
	}
}

// TestConnectionFirstReportOfIsOncePerKindUntilReset.
// client/src/session/connection.rs::first_report_is_once_per_kind_until_reset.
func TestConnectionFirstReportOfIsOncePerKindUntilReset(t *testing.T) {
	var connection connectionLifecycle
	if !connection.firstReportOf(ErrorNetwork) {
		t.Fatal("the first network report must be new")
	}
	if connection.firstReportOf(ErrorNetwork) {
		t.Fatal("a repeated network report must be suppressed")
	}
	if !connection.firstReportOf(ErrorTLS) {
		t.Fatal("a different kind is a new report")
	}
	if connection.firstReportOf(ErrorTLS) {
		t.Fatal("a repeated TLS report must be suppressed")
	}

	connection.resetRetries()

	if !connection.firstReportOf(ErrorNetwork) {
		t.Fatal("a reset must re-arm the network report")
	}
}

// TestConnectionResetRetriesClearsAttemptAndReports.
// client/src/session/connection.rs::reconnect_attempts_count_from_one.
func TestConnectionResetRetriesClearsAttemptAndReports(t *testing.T) {
	var connection connectionLifecycle
	if got := connection.countScheduledReconnect(); got != 1 {
		t.Fatalf("first attempt = %d, want 1", got)
	}
	if got := connection.countScheduledReconnect(); got != 2 {
		t.Fatalf("second attempt = %d, want 2", got)
	}
	connection.firstReportOf(ErrorNetwork)

	connection.resetRetries()

	if connection.reconnectAttempt != 0 {
		t.Fatalf("attempt = %d, want 0", connection.reconnectAttempt)
	}
	if got := connection.countScheduledReconnect(); got != 1 {
		t.Fatalf("attempt after reset = %d, want 1", got)
	}
	if !connection.firstReportOf(ErrorNetwork) {
		t.Fatal("a reset must clear the once-per-kind report")
	}
}

// TestConnectionStopClearsRetryCounts.
// client/src/session/connection.rs::stop_clears_pending_reconnect_counts.
func TestConnectionStopClearsRetryCounts(t *testing.T) {
	var connection connectionLifecycle
	connection.countScheduledReconnect()
	connection.firstReportOf(ErrorTLS)

	connection.beginStop()

	if connection.reconnectAttempt != 0 {
		t.Fatalf("attempt = %d, want 0", connection.reconnectAttempt)
	}
	if !connection.firstReportOf(ErrorTLS) {
		t.Fatal("a stop must clear the once-per-kind report")
	}
}
