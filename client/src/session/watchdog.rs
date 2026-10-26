//! The ping watchdog (`pingWatchdogState` in Go). Once registered, a silent socket
//! gets one PING probe, and a probe that stays unanswered fails the connection.
//! `PingWatchdog` owns the Off / Watching / Probing state and its transitions.
//! The Ping timer slot stays in `Core`, so the `Core` methods below pair each
//! transition with its timer.

use std::time::Duration;

use super::events::{ErrorKind, SessionState};
use super::session::Core;
use super::timers::TimerSlot;
use crate::irc::message::IrcMessage;

/// The PING token the watchdog sends, and expects back in PONG.
const PING_WATCHDOG_TOKEN: &str = "omairc-watchdog";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PingState {
    #[default]
    Off,
    /// Silence is being timed; no probe is outstanding.
    Watching,
    /// A probe went out and its PONG has not arrived.
    Probing,
}

/// What an expired silence window asks the session to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PingExpiry {
    /// Send a probe and time the answer.
    Probe,
    /// The probe went unanswered: fail the connection.
    Timeout,
    /// The watchdog is off, so the expiry is stale.
    Nothing,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PingWatchdog {
    state: PingState,
}

impl PingWatchdog {
    /// Whether silence is being timed, so inbound bytes restart the window.
    pub(crate) fn is_watching(&self) -> bool {
        self.state == PingState::Watching
    }

    /// Whether `message` is the PONG that answers the outstanding probe.
    pub(crate) fn answers_probe(&self, message: &IrcMessage) -> bool {
        self.state == PingState::Probing
            && message
                .params
                .last()
                .is_some_and(|token| token.as_slice() == PING_WATCHDOG_TOKEN.as_bytes())
    }

    fn watch(&mut self) {
        self.state = PingState::Watching;
    }

    fn stop(&mut self) {
        self.state = PingState::Off;
    }

    fn expire(&mut self) -> PingExpiry {
        match self.state {
            PingState::Watching => {
                self.state = PingState::Probing;
                PingExpiry::Probe
            }
            PingState::Probing => PingExpiry::Timeout,
            PingState::Off => PingExpiry::Nothing,
        }
    }
}

impl Core {
    /// Restarts the silence window, once registered (`armPingWatchdog`).
    pub(crate) fn arm_ping_watchdog(&mut self) {
        if self.state != SessionState::Registered {
            return;
        }
        self.ping.watch();
        self.arm_timer(
            TimerSlot::Ping,
            Duration::from_millis(self.config.ping_timeout_ms),
        );
    }

    pub(crate) fn cancel_ping_watchdog(&mut self) {
        self.ping.stop();
        self.cancel_timer(TimerSlot::Ping);
    }

    pub(crate) fn on_ping_watchdog_fired(&mut self) {
        if self.state != SessionState::Registered {
            return;
        }
        match self.ping.expire() {
            PingExpiry::Probe => {
                self.send_line(format!("PING :{PING_WATCHDOG_TOKEN}\r\n").as_bytes());
                self.arm_timer(
                    TimerSlot::Ping,
                    Duration::from_millis(self.config.ping_timeout_ms),
                );
            }
            PingExpiry::Timeout => self.fail(ErrorKind::Network, "Ping timeout", true),
            PingExpiry::Nothing => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::parser::parse;

    #[test]
    fn silence_is_probed_once_then_times_out() {
        let mut watchdog = PingWatchdog::default();
        assert_eq!(
            watchdog.expire(),
            PingExpiry::Nothing,
            "an off watchdog ignores expiry"
        );

        watchdog.watch();
        assert!(watchdog.is_watching());
        assert_eq!(watchdog.expire(), PingExpiry::Probe);
        assert!(!watchdog.is_watching(), "a probe is outstanding");
        assert_eq!(watchdog.expire(), PingExpiry::Timeout);

        watchdog.stop();
        assert!(!watchdog.is_watching());
        assert_eq!(watchdog.expire(), PingExpiry::Nothing);
    }

    #[test]
    fn only_the_probe_token_answers_an_outstanding_probe() {
        let mut watchdog = PingWatchdog::default();
        let pong = parse(b":server PONG irc.example :omairc-watchdog").expect("parses");
        let other = parse(b":server PONG irc.example :something-else").expect("parses");

        assert!(!watchdog.answers_probe(&pong), "no probe is outstanding");
        watchdog.watch();
        assert!(!watchdog.answers_probe(&pong), "silence is not a probe");
        watchdog.expire();
        assert!(
            !watchdog.answers_probe(&other),
            "a foreign token is not an answer"
        );
        assert!(watchdog.answers_probe(&pong));
    }
}
