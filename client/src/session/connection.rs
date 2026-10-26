//! The connection's lifecycle decisions: why the transport is going away, how many
//! reconnects have been scheduled, and which retryable error kinds were already
//! reported. These were the `expected_disconnect` and `reconnect_after_disconnect`
//! flags of `Core` plus its retry counters. `session.rs` applies the outcomes
//! nothing here touches the transport, the timers or the state machine.

use std::time::Duration;

use super::config::SessionConfig;
use super::events::{ErrorKind, SessionState};

/// Why the transport is going away, which decides what its disconnect means.
/// A stop or a fatal failure outranks a request to reconnect, so a reconnect
/// requested after either one is not recorded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DisconnectIntent {
    /// Nothing asked for the disconnect, so the session fails with a network error.
    #[default]
    Unexpected,
    /// The user stopped the session, or a fatal failure ended it. The disconnect
    /// settles the session and nothing reconnects.
    Stopping,
    /// A retryable failure shut the transport down. The disconnect schedules a
    /// reconnect, then the intent returns to `Unexpected`.
    Reconnecting,
}

/// What the session does when the transport reports a disconnect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisconnectOutcome {
    /// Schedule the reconnect backoff.
    ScheduleReconnect,
    /// A stopping session that is closing moves to Idle.
    SettleIdle,
    /// The server closed the connection on its own: fail with a network error.
    FailUnexpected,
    /// The plaintext socket closed for an STS upgrade: connect again at once on the
    /// upgraded port.
    FinishStsUpgrade,
    /// A stopping session that is not closing, such as one that already failed
    /// keeps its state.
    Ignore,
}

#[derive(Debug, Default)]
pub(crate) struct Connection {
    intent: DisconnectIntent,
    /// Reconnects scheduled since the last welcome, stop or start.
    reconnect_attempt: u32,
    /// One bit per `ErrorKind` already reported as retryable, in `report_bit`'s order.
    reported_retry_errors: u32,
}

impl Connection {
    /// A fresh `start`: nothing pending and no retries counted.
    pub(crate) fn begin_start(&mut self) {
        self.intent = DisconnectIntent::Unexpected;
        self.reset_retries();
    }

    /// The user's `stop`: the next disconnect settles the session, and nothing
    /// reconnects after it.
    pub(crate) fn begin_stop(&mut self) {
        self.intent = DisconnectIntent::Stopping;
        self.reset_retries();
    }

    /// A retryable failure shut the live transport down, so its disconnect should
    /// reconnect. A stop or a fatal failure already in effect keeps its intent.
    pub(crate) fn reconnect_on_disconnect(&mut self) {
        if self.intent == DisconnectIntent::Unexpected {
            self.intent = DisconnectIntent::Reconnecting;
        }
    }

    /// A fatal failure shut the live transport down, so its disconnect ends the
    /// session and no reconnect follows, even one that was already pending.
    pub(crate) fn end_on_disconnect(&mut self) {
        self.intent = DisconnectIntent::Stopping;
    }

    /// Whether the connection is ending, so transport errors are not reported.
    pub(crate) fn is_stopping(&self) -> bool {
        self.intent == DisconnectIntent::Stopping
    }

    /// The transport reported a disconnect while the session was in `state`. A
    /// pending reconnect is spent here, so the next disconnect is unexpected again.
    pub(crate) fn on_disconnect(&mut self, state: SessionState) -> DisconnectOutcome {
        match self.intent {
            DisconnectIntent::Reconnecting => {
                self.intent = DisconnectIntent::Unexpected;
                DisconnectOutcome::ScheduleReconnect
            }
            DisconnectIntent::Stopping if state == SessionState::Closing => {
                DisconnectOutcome::SettleIdle
            }
            DisconnectIntent::Stopping => DisconnectOutcome::Ignore,
            DisconnectIntent::Unexpected if state == SessionState::StsUpgrading => {
                DisconnectOutcome::FinishStsUpgrade
            }
            DisconnectIntent::Unexpected => DisconnectOutcome::FailUnexpected,
        }
    }

    /// Records that `kind` is reported as retryable. True only the first time since
    /// the last reset, so a flapping link reports each kind once.
    pub(crate) fn first_report_of(&mut self, kind: ErrorKind) -> bool {
        let bit = report_bit(kind);
        if self.reported_retry_errors & bit != 0 {
            return false;
        }
        self.reported_retry_errors |= bit;
        true
    }

    /// Counts one more scheduled reconnect and returns the new attempt number.
    pub(crate) fn count_scheduled_reconnect(&mut self) -> u32 {
        self.reconnect_attempt += 1;
        self.reconnect_attempt
    }

    /// How many reconnects have been scheduled since the last reset.
    #[cfg(test)]
    pub(crate) fn reconnect_attempt(&self) -> u32 {
        self.reconnect_attempt
    }

    /// The backoff for the current attempt: the base, doubled per attempt, capped
    /// at the maximum (`reconnectDelayLocked`).
    pub(crate) fn next_delay(&self, config: &SessionConfig) -> Duration {
        let base = config.reconnect_base_delay_ms;
        let maximum = config.reconnect_maximum_delay_ms.max(base);
        let mut delay = base;
        let mut step = 1;
        while step < self.reconnect_attempt && delay < maximum {
            delay = (delay * 2).min(maximum);
            step += 1;
        }
        Duration::from_millis(delay.min(i32::MAX as u64))
    }

    /// Forgets the retry count and the reported kinds. A welcome (001) does this
    /// and so do `start` and `stop`.
    pub(crate) fn reset_retries(&mut self) {
        self.reconnect_attempt = 0;
        self.reported_retry_errors = 0;
    }
}

/// The dedup bit of one error kind, by its position in `ErrorKind`.
fn report_bit(kind: ErrorKind) -> u32 {
    1 << (kind as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(base_ms: u64, maximum_ms: u64) -> SessionConfig {
        SessionConfig {
            reconnect_base_delay_ms: base_ms,
            reconnect_maximum_delay_ms: maximum_ms,
            ..SessionConfig::default()
        }
    }

    #[test]
    fn start_expects_an_unexpected_disconnect() {
        let mut connection = Connection::default();
        connection.begin_start();
        assert!(!connection.is_stopping());
        assert_eq!(
            connection.on_disconnect(SessionState::Registered),
            DisconnectOutcome::FailUnexpected
        );
    }

    #[test]
    fn start_clears_a_stop_and_the_retry_counts() {
        let mut connection = Connection::default();
        connection.begin_stop();
        connection.count_scheduled_reconnect();
        connection.first_report_of(ErrorKind::Network);
        connection.begin_start();
        assert!(!connection.is_stopping());
        assert_eq!(connection.reconnect_attempt(), 0);
        assert!(connection.first_report_of(ErrorKind::Network));
    }

    #[test]
    fn stop_settles_a_closing_session_and_keeps_stopping() {
        let mut connection = Connection::default();
        connection.begin_stop();
        assert!(connection.is_stopping());
        assert_eq!(
            connection.on_disconnect(SessionState::Closing),
            DisconnectOutcome::SettleIdle
        );
        assert!(
            connection.is_stopping(),
            "a stop lasts until the next start"
        );
    }

    #[test]
    fn stop_ignores_a_disconnect_that_is_not_closing() {
        let mut connection = Connection::default();
        connection.begin_stop();
        assert_eq!(
            connection.on_disconnect(SessionState::Failed),
            DisconnectOutcome::Ignore
        );
        assert!(connection.is_stopping());
    }

    #[test]
    fn stop_clears_pending_reconnect_counts() {
        let mut connection = Connection::default();
        connection.count_scheduled_reconnect();
        connection.first_report_of(ErrorKind::Tls);
        connection.begin_stop();
        assert_eq!(connection.reconnect_attempt(), 0);
        assert!(connection.first_report_of(ErrorKind::Tls));
    }

    #[test]
    fn reconnect_request_turns_a_disconnect_into_one_reconnect() {
        let mut connection = Connection::default();
        connection.begin_start();
        connection.reconnect_on_disconnect();
        assert_eq!(
            connection.on_disconnect(SessionState::Reconnecting),
            DisconnectOutcome::ScheduleReconnect
        );
        assert_eq!(
            connection.on_disconnect(SessionState::Reconnecting),
            DisconnectOutcome::FailUnexpected,
            "the reconnect request is spent by the disconnect"
        );
    }

    #[test]
    fn repeated_reconnect_requests_coalesce() {
        let mut connection = Connection::default();
        connection.begin_start();
        connection.reconnect_on_disconnect();
        connection.reconnect_on_disconnect();
        assert_eq!(
            connection.on_disconnect(SessionState::Connecting),
            DisconnectOutcome::ScheduleReconnect
        );
        assert_eq!(
            connection.on_disconnect(SessionState::Connecting),
            DisconnectOutcome::FailUnexpected
        );
    }

    #[test]
    fn reconnect_request_does_not_override_a_stop() {
        let mut connection = Connection::default();
        connection.begin_stop();
        connection.reconnect_on_disconnect();
        assert!(connection.is_stopping());
        assert_eq!(
            connection.on_disconnect(SessionState::Closing),
            DisconnectOutcome::SettleIdle
        );
    }

    #[test]
    fn fatal_failure_overrides_a_pending_reconnect() {
        let mut connection = Connection::default();
        connection.begin_start();
        connection.reconnect_on_disconnect();
        connection.end_on_disconnect();
        assert!(connection.is_stopping());
        assert_eq!(
            connection.on_disconnect(SessionState::Failed),
            DisconnectOutcome::Ignore,
            "a fatal failure must not reconnect after its disconnect"
        );
    }

    #[test]
    fn fatal_failure_settles_a_closing_session() {
        let mut connection = Connection::default();
        connection.begin_start();
        connection.end_on_disconnect();
        assert_eq!(
            connection.on_disconnect(SessionState::Closing),
            DisconnectOutcome::SettleIdle
        );
    }

    #[test]
    fn unexpected_disconnect_keeps_no_intent() {
        let mut connection = Connection::default();
        connection.begin_start();
        assert_eq!(
            connection.on_disconnect(SessionState::Registered),
            DisconnectOutcome::FailUnexpected
        );
        assert!(!connection.is_stopping());
    }

    #[test]
    fn an_sts_upgrade_disconnect_reconnects_at_once() {
        let mut connection = Connection::default();
        connection.begin_start();
        assert_eq!(
            connection.on_disconnect(SessionState::StsUpgrading),
            DisconnectOutcome::FinishStsUpgrade
        );
        assert!(!connection.is_stopping());
    }

    #[test]
    fn a_reconnect_request_or_a_stop_outranks_an_sts_upgrade() {
        let mut connection = Connection::default();
        connection.begin_start();
        connection.reconnect_on_disconnect();
        assert_eq!(
            connection.on_disconnect(SessionState::StsUpgrading),
            DisconnectOutcome::ScheduleReconnect
        );

        connection.begin_stop();
        assert_eq!(
            connection.on_disconnect(SessionState::StsUpgrading),
            DisconnectOutcome::Ignore
        );
    }

    #[test]
    fn first_report_is_once_per_kind_until_reset() {
        let mut connection = Connection::default();
        assert!(connection.first_report_of(ErrorKind::Network));
        assert!(!connection.first_report_of(ErrorKind::Network));
        assert!(connection.first_report_of(ErrorKind::Tls));
        assert!(!connection.first_report_of(ErrorKind::Tls));
        connection.reset_retries();
        assert!(connection.first_report_of(ErrorKind::Network));
    }

    #[test]
    fn reconnect_attempts_count_from_one() {
        let mut connection = Connection::default();
        assert_eq!(connection.reconnect_attempt(), 0);
        assert_eq!(connection.count_scheduled_reconnect(), 1);
        assert_eq!(connection.count_scheduled_reconnect(), 2);
        assert_eq!(connection.reconnect_attempt(), 2);
        connection.reset_retries();
        assert_eq!(connection.reconnect_attempt(), 0);
    }

    #[test]
    fn backoff_doubles_per_attempt_and_stops_at_the_maximum() {
        let config = config(250, 1000);
        let mut connection = Connection::default();
        let mut delays = Vec::new();
        for _ in 0..5 {
            connection.count_scheduled_reconnect();
            delays.push(connection.next_delay(&config).as_millis() as u64);
        }
        assert_eq!(delays, vec![250, 500, 1000, 1000, 1000]);
    }

    #[test]
    fn backoff_is_the_base_before_any_attempt() {
        let connection = Connection::default();
        assert_eq!(
            connection.next_delay(&config(250, 1000)),
            Duration::from_millis(250)
        );
    }

    #[test]
    fn backoff_never_drops_below_the_base() {
        let mut connection = Connection::default();
        connection.count_scheduled_reconnect();
        connection.count_scheduled_reconnect();
        assert_eq!(
            connection.next_delay(&config(500, 100)),
            Duration::from_millis(500)
        );
    }
}
