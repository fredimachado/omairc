//! Port of the outbound typing gate in `tui/internal/session/session.go` (Go,
//! `typingPublisher`) and `src/irc/irctypingpublisher.{h,cpp}` (C++).
//!
//! The composer's `+typing` TAGMSG is sent only when `message-tags` is enabled,
//! at most once per target every `TYPING_SEND_INTERVAL` (3 s), and never as a
//! `done` right after the user sent a message. The phase type, the TAGMSG
//! builder and the pace constant are wire vocabulary: they live in `irc/typing`
//! and are re-exported here for the session's callers.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

pub use crate::irc::typing::{typing_tagmsg, TypingPhase, TYPING_SEND_INTERVAL_MS};

use super::session::Core;
use crate::irc::capability::IrcCapability;

/// One target's outbound pace (`IrcTypingPublisher::TargetClock`).
#[derive(Clone, Debug, Default)]
// Phase 4: the typing publisher throttles and sends typing frames
#[allow(dead_code)]
pub(crate) struct TargetClock {
    // Phase 4: the typing publisher throttles and sends typing frames
    #[allow(dead_code)]
    pub(crate) last_sent_at: Option<DateTime<Utc>>,
    // Phase 4: the typing publisher throttles and sends typing frames
    #[allow(dead_code)]
    pub(crate) last_phase: TypingPhase,
    pub(crate) suppress_done: bool,
}

/// The outbound typing gate, keyed by the raw target.
#[derive(Clone, Debug, Default)]
pub(crate) struct TypingPublisher {
    pub(crate) targets: BTreeMap<String, TargetClock>,
}

impl TypingPublisher {
    /// Whether `phase` may go out for `target` at `now`.
    // Phase 4: the typing publisher throttles and sends typing frames
    #[allow(dead_code)]
    pub(crate) fn should_send(&self, target: &str, phase: TypingPhase, now: DateTime<Utc>) -> bool {
        if phase == TypingPhase::Paused {
            return false;
        }
        let Some(clock) = self.targets.get(target) else {
            return phase == TypingPhase::Active;
        };
        if phase == TypingPhase::Done && clock.suppress_done {
            return false;
        }
        if phase == TypingPhase::Done && clock.last_phase != TypingPhase::Active {
            return false;
        }
        if let Some(last) = clock.last_sent_at {
            if now - last < chrono::TimeDelta::milliseconds(TYPING_SEND_INTERVAL_MS) {
                return false;
            }
        }
        true
    }

    /// Records that `phase` went out for `target` at `now`.
    // Phase 4: the typing publisher throttles and sends typing frames
    #[allow(dead_code)]
    pub(crate) fn record_sent(&mut self, target: &str, phase: TypingPhase, now: DateTime<Utc>) {
        let clock = self.targets.entry(target.to_owned()).or_default();
        clock.last_sent_at = Some(now);
        clock.last_phase = phase;
        if phase != TypingPhase::Done {
            clock.suppress_done = false;
        }
    }

    /// A message went to `target`, so a trailing `done` is suppressed.
    pub(crate) fn note_message_sent(&mut self, target: &str) {
        self.targets
            .entry(target.to_owned())
            .or_default()
            .suppress_done = true;
    }

    /// Forgets every target, on a new connection.
    pub(crate) fn reset(&mut self) {
        self.targets.clear();
    }
}

// Phase 4: the typing publisher throttles and sends typing frames
#[allow(dead_code)]
impl Core {
    /// Sends a typing phase when registered, with `message-tags`, and the pace
    /// allows it (`sendTypingLocked`). The TAGMSG bypasses the outbound status
    /// event, as the Go write does.
    // Phase 4: the typing publisher throttles and sends typing frames
    #[allow(dead_code)]
    pub(crate) fn send_typing(&mut self, target: &str, phase: TypingPhase) -> bool {
        if self.state != super::events::SessionState::Registered {
            return false;
        }
        if !self.caps.enabled().contains(IrcCapability::MessageTags) {
            return false;
        }
        let now = self.now();
        if !self.typing.should_send(target, phase, now) {
            return false;
        }
        let Some(line) = typing_tagmsg(target, phase) else {
            return false;
        };
        self.effects
            .push(super::session::Effect::Write(line.into_bytes()));
        self.typing.record_sent(target, phase, now);
        true
    }

    /// Records an outbound PRIVMSG or ACTION, which suppresses a trailing `done`.
    pub(crate) fn note_message_sent(&mut self, target: &str) {
        self.typing.note_message_sent(target);
    }
}
