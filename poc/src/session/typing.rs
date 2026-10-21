//! Port of the outbound typing gate in `tui/internal/session/session.go` (Go,
//! `typingPublisher`), `src/irc/irctypingpublisher.{h,cpp}` (C++), and the
//! `TypingTagmsg` builder of `tui/internal/irc/typing.go`.
//!
//! The composer's `+typing` TAGMSG is sent only when `message-tags` is enabled,
//! at most once per target every `TYPING_SEND_INTERVAL` (3 s), and never as a
//! `done` right after the user sent a message. Phase 4 moves the phase type and
//! the builder into `irc/typing.rs` with the inbound hint side.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::session::Core;
use crate::irc::capability::IrcCapability;

/// The per-target outbound throttle, in milliseconds.
pub const TYPING_SEND_INTERVAL_MS: i64 = 3000;

/// A `+typing` phase (`IrcTypingPhase`). `Done` is never stored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TypingPhase {
    #[default]
    Active,
    Paused,
    Done,
}

impl TypingPhase {
    /// The tag value on the wire.
    pub fn tag_value(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Done => "done",
        }
    }
}

/// Builds `@+typing=<phase> TAGMSG <target>` with its CRLF. `None` when the
/// target is empty or carries a space, CR or LF.
pub fn typing_tagmsg(target: &str, phase: TypingPhase) -> Option<String> {
    if target.is_empty() || target.bytes().any(|byte| matches!(byte, b' ' | b'\r' | b'\n')) {
        return None;
    }
    Some(format!("@+typing={} TAGMSG {target}\r\n", phase.tag_value()))
}

/// One target's outbound pace (`IrcTypingPublisher::TargetClock`).
#[derive(Clone, Debug, Default)]
struct TargetClock {
    last_sent_at: Option<DateTime<Utc>>,
    last_phase: TypingPhase,
    suppress_done: bool,
}

/// The outbound typing gate, keyed by the raw target.
#[derive(Clone, Debug, Default)]
pub(crate) struct TypingPublisher {
    targets: BTreeMap<String, TargetClock>,
}

impl TypingPublisher {
    /// Whether `phase` may go out for `target` at `now`.
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
        self.targets.entry(target.to_owned()).or_default().suppress_done = true;
    }

    /// Forgets every target, on a new connection.
    pub(crate) fn reset(&mut self) {
        self.targets.clear();
    }
}

impl Core {
    /// Sends a typing phase when registered, with `message-tags`, and the pace
    /// allows it (`sendTypingLocked`). The TAGMSG bypasses the outbound status
    /// event, as the Go write does.
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
        self.effects.push(super::session::Effect::Write(line.into_bytes()));
        self.typing.record_sent(target, phase, now);
        true
    }

    /// Records an outbound PRIVMSG or ACTION, which suppresses a trailing `done`.
    pub(crate) fn note_message_sent(&mut self, target: &str) {
        self.typing.note_message_sent(target);
    }
}
