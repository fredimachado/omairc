//! Port of `tui/internal/irc/typing.go` plus
//! `irctypingpublisher` (C++).
//!
//! Two halves share this file. The phase type, the TAGMSG builder and the
//! hold clocks are the wire vocabulary of the `+typing` client tag. The inbound
//! hint (`TypingHint`) is what the reducer stores for a peer who is typing, and
//! its expiry is computed from the clock the peer's phase selects. The outbound
//! pacing gate lives in `session/typing.rs`, because it reads the session's
//! state and clock.
//!
//! Pure: no clock is read here. Callers pass the receive time and `now`.

use chrono::{DateTime, TimeDelta, Utc};

/// How long an active hint is retained, in milliseconds (`tui/internal/irc/typing.go`).
pub const TYPING_ACTIVE_HOLD_MS: i64 = 6000;

/// How long a paused hint is retained. The IRCv3 `typing` recommendation says
/// to assume the sender is still typing for at least 30 s after `paused`.
pub const TYPING_PAUSED_HOLD_MS: i64 = 30000;

/// The per-target outbound throttle, in milliseconds (`tui/internal/irc/typing.go`).
pub const TYPING_SEND_INTERVAL_MS: i64 = 3000;

/// Which hold a stored hint uses (`TypingClock`, `IrcTypingClock`). The zero
/// value is `Active`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TypingClock {
    /// An active hint, held for `TYPING_ACTIVE_HOLD_MS`.
    #[default]
    Active,
    /// A paused hint, held for `TYPING_PAUSED_HOLD_MS`.
    Paused,
}

/// A `+typing` phase (`TypingPhase`, `IrcTypingPhase`). The zero value is
/// `Active`. `Done` is never stored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TypingPhase {
    /// The sender is typing.
    #[default]
    Active,
    /// The sender paused, still assumed typing.
    Paused,
    /// The sender stopped.
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

/// A stored inbound typing hint (`TypingHint`, `IrcTypingHint`). `received_at`
/// is `None` for a hint that carries no timestamp, which is never retained.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypingHint {
    pub clock: TypingClock,
    pub received_at: Option<DateTime<Utc>>,
    pub display_nick: String,
}

/// The hint to store for a received phase. `Done` stores nothing, so the
/// result is `None` (`StoredTypingHint`).
pub fn stored_typing_hint(
    phase: TypingPhase,
    received_at: DateTime<Utc>,
    display_nick: &str,
) -> Option<TypingHint> {
    if phase == TypingPhase::Done {
        return None;
    }
    let clock = if phase == TypingPhase::Paused {
        TypingClock::Paused
    } else {
        TypingClock::Active
    };
    Some(TypingHint {
        clock,
        received_at: Some(received_at),
        display_nick: display_nick.to_owned(),
    })
}

/// When the hint's clock-specific hold ends (`TypingExpiresAt`). `None` when the
/// hint has no receive time.
pub fn typing_expires_at(hint: &TypingHint) -> Option<DateTime<Utc>> {
    let hold_ms = match hint.clock {
        TypingClock::Active => TYPING_ACTIVE_HOLD_MS,
        TypingClock::Paused => TYPING_PAUSED_HOLD_MS,
    };
    hint.received_at
        .map(|received| received + TimeDelta::milliseconds(hold_ms))
}

/// Whether the hint is still stored at `now` (`TypingHintRetained`). A hint with
/// no receive time is never retained. The hold is exclusive at its end.
pub fn typing_hint_retained(hint: &TypingHint, now: DateTime<Utc>) -> bool {
    typing_expires_at(hint).is_some_and(|expires| now < expires)
}

/// Whether the typing indicator paints at `now` (`TypingShowsIndicator`). Active
/// and paused both show; they differ only in hold duration.
pub fn typing_shows_indicator(hint: &TypingHint, now: DateTime<Utc>) -> bool {
    typing_hint_retained(hint, now)
}

/// Resolves a `+typing` tag value, ignoring ASCII case (`TypingPhaseFromTag`).
/// `None` for an unknown or empty value.
pub fn typing_phase_from_tag(value: &str) -> Option<TypingPhase> {
    if value.eq_ignore_ascii_case("active") {
        Some(TypingPhase::Active)
    } else if value.eq_ignore_ascii_case("paused") {
        Some(TypingPhase::Paused)
    } else if value.eq_ignore_ascii_case("done") {
        Some(TypingPhase::Done)
    } else {
        None
    }
}

/// Builds `@+typing=<phase> TAGMSG <target>` with its CRLF (`TypingTagmsg`,
/// `ircTypingTagmsg`). `None` when the target is empty or carries a space, CR or
/// LF.
pub fn typing_tagmsg(target: &str, phase: TypingPhase) -> Option<String> {
    if target.is_empty()
        || target
            .bytes()
            .any(|byte| matches!(byte, b' ' | b'\r' | b'\n'))
    {
        return None;
    }
    Some(format!(
        "@+typing={} TAGMSG {target}\r\n",
        phase.tag_value()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// The fixed clock the ported typing tests use (`typingT0`).
    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap()
    }

    fn at(ms: i64) -> DateTime<Utc> {
        t0() + TimeDelta::milliseconds(ms)
    }

    #[test]
    fn stored_clocks_follow_the_phase() {
        let active = stored_typing_hint(TypingPhase::Active, t0(), "alice").expect("stored");
        assert_eq!(active.clock, TypingClock::Active);
        assert_eq!(active.display_nick, "alice");
        assert_eq!(active.received_at, Some(t0()));

        let paused = stored_typing_hint(TypingPhase::Paused, t0(), "bob").expect("stored");
        assert_eq!(paused.clock, TypingClock::Paused);

        assert!(
            stored_typing_hint(TypingPhase::Done, t0(), "carol").is_none(),
            "done must never store a hint"
        );
    }

    #[test]
    fn an_active_hint_shows_until_six_seconds() {
        let active = stored_typing_hint(TypingPhase::Active, t0(), "alice").expect("stored");
        assert!(typing_shows_indicator(&active, at(0)));
        assert!(typing_shows_indicator(&active, at(5999)));
        assert!(!typing_shows_indicator(&active, at(6000)));
        assert!(typing_hint_retained(&active, at(5999)));
        assert!(!typing_hint_retained(&active, at(6000)));
        assert!(typing_shows_indicator(&active, at(1000)), "still typing");
    }

    #[test]
    fn a_paused_hint_shows_until_thirty_seconds() {
        let paused = stored_typing_hint(TypingPhase::Paused, t0(), "bob").expect("stored");
        assert!(typing_shows_indicator(&paused, at(0)));
        assert!(typing_shows_indicator(&paused, at(5999)));
        assert!(typing_shows_indicator(&paused, at(6000)));
        assert!(typing_shows_indicator(&paused, at(29999)));
        assert!(!typing_shows_indicator(&paused, at(30000)));
        assert!(typing_hint_retained(&paused, at(29999)));
        assert!(!typing_hint_retained(&paused, at(30000)));
        assert!(typing_shows_indicator(&paused, at(1000)), "still typing");
    }

    #[test]
    fn a_hint_without_a_receive_time_is_never_retained() {
        let invalid = TypingHint {
            clock: TypingClock::Active,
            received_at: None,
            display_nick: String::new(),
        };
        assert!(!typing_shows_indicator(&invalid, t0()));
        assert!(!typing_hint_retained(&invalid, t0()));
        assert_eq!(typing_expires_at(&invalid), None);
    }

    #[test]
    fn expiry_is_received_plus_the_clock_hold() {
        let active = stored_typing_hint(TypingPhase::Active, t0(), "alice").expect("stored");
        assert_eq!(typing_expires_at(&active), Some(at(6000)));
        let paused = stored_typing_hint(TypingPhase::Paused, t0(), "bob").expect("stored");
        assert_eq!(typing_expires_at(&paused), Some(at(30000)));
    }

    #[test]
    fn phase_resolves_from_the_tag_ignoring_case() {
        let cases = [
            ("active", Some(TypingPhase::Active)),
            ("ACTIVE", Some(TypingPhase::Active)),
            ("paused", Some(TypingPhase::Paused)),
            ("PAUSED", Some(TypingPhase::Paused)),
            ("done", Some(TypingPhase::Done)),
            ("DONE", Some(TypingPhase::Done)),
            ("draft", None),
            ("", None),
        ];
        for (value, want) in cases {
            assert_eq!(typing_phase_from_tag(value), want, "tag {value:?}");
        }
    }

    #[test]
    fn tagmsg_is_byte_for_byte_the_tag_frame() {
        assert_eq!(
            typing_tagmsg("#omarchy", TypingPhase::Active).as_deref(),
            Some("@+typing=active TAGMSG #omarchy\r\n")
        );
        assert_eq!(
            typing_tagmsg("#omarchy", TypingPhase::Paused).as_deref(),
            Some("@+typing=paused TAGMSG #omarchy\r\n")
        );
        assert_eq!(
            typing_tagmsg("#omarchy", TypingPhase::Done).as_deref(),
            Some("@+typing=done TAGMSG #omarchy\r\n")
        );
    }

    #[test]
    fn tagmsg_rejects_a_space_a_line_break_or_an_empty_target() {
        assert_eq!(typing_tagmsg("bad target", TypingPhase::Active), None);
        assert_eq!(typing_tagmsg("", TypingPhase::Done), None);
        assert_eq!(typing_tagmsg("#a\rb", TypingPhase::Active), None);
        assert_eq!(typing_tagmsg("#a\nb", TypingPhase::Active), None);
    }

    #[test]
    fn the_hold_and_pace_constants_match_the_qt_header() {
        assert_eq!(TYPING_ACTIVE_HOLD_MS, 6000);
        assert_eq!(TYPING_PAUSED_HOLD_MS, 30000);
        assert_eq!(TYPING_SEND_INTERVAL_MS, 3000);
    }
}
