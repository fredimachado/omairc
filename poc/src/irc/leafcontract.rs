//! Temporary seam for the leaf modules the event taxonomy and translator use.
//!
//! `presence.go`, `membershipnoise.go`, `typing.go` and `inbox.go` are ported by
//! leaf modules in parallel with this phase. The translator needs one slice of
//! each: the `Away` value, metadata key canonicalization and clamping, the
//! membership line wording, the `+typing` tag reader and the inbox kind. This
//! file carries that slice from the Go, byte for byte, so the event and
//! translator port can land and be tested first.
//!
//! When the leaf modules land, point each `use super::leafcontract::...` at its
//! owner (`presence`, `membershipnoise`, `typing`, `inbox`) and delete this file.
//! `TypingPhase` is re-exported from `session::typing` until `typing.rs` owns it.

pub use crate::session::typing::TypingPhase;

/// An away notice with its optional reason (`presence.go` `Away`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Away {
    pub reason: String,
}

/// The client ceiling on one metadata value, in UTF-8 bytes
/// (`presence.go` `MaximumValueBytes`).
pub const MAXIMUM_VALUE_BYTES: usize = 512;

/// The metadata keys Omairc subscribes to, in priority order (`subscribedKeys`).
/// Status comes first so `/status` still works under a max-subs limit.
const SUBSCRIBED_KEYS: [&str; 7] = [
    "status",
    "avatar",
    "bot",
    "display-name",
    "pronouns",
    "homepage",
    "color",
];

/// The IRCv3 `avatar` metadata key (`AvatarKey`).
pub fn avatar_key() -> &'static str {
    "avatar"
}

/// The IRCv3 `bot` metadata key (`BotKey`).
pub fn bot_key() -> &'static str {
    "bot"
}

/// The canonical spelling of a subscribed metadata key, matched without regard to
/// case (`CanonicalKey`). `None` for a key Omairc does not subscribe to.
pub fn canonical_key(key: &str) -> Option<&'static str> {
    SUBSCRIBED_KEYS
        .iter()
        .copied()
        .find(|known| known.eq_ignore_ascii_case(key))
}

/// Truncates `value` to `MAXIMUM_VALUE_BYTES` UTF-8 bytes (`Clamped`). Truncation
/// drops one UTF-16 code unit at a time, as `QString::chop` does, so a split
/// surrogate becomes U+FFFD.
pub fn clamped(value: &str) -> String {
    let mut units: Vec<u16> = value.encode_utf16().collect();
    loop {
        let text = String::from_utf16_lossy(&units);
        if text.len() <= MAXIMUM_VALUE_BYTES {
            return text;
        }
        units.pop();
    }
}

/// The parenthetical account on a join, or "" when it adds nothing (`ShownAccount`).
pub fn shown_account(same_as_nick: bool, account: &str) -> String {
    if account.is_empty() || account == "*" || same_as_nick {
        return String::new();
    }
    account.to_owned()
}

/// One join, rendered the way a lone line reads (`JoinLine`).
pub fn join_line(nick: &str, shown_account: &str) -> String {
    if shown_account.is_empty() {
        return format!("{nick} joined");
    }
    format!("{nick} ({shown_account}) joined")
}

/// One part (`PartLine`).
pub fn part_line(nick: &str) -> String {
    format!("{nick} left")
}

/// One quit (`QuitLine`).
pub fn quit_line(nick: &str) -> String {
    format!("{nick} quit")
}

/// One nick change (`NickLine`).
pub fn nick_line(old_nick: &str, new_nick: &str) -> String {
    format!("{old_nick} is now {new_nick}")
}

/// Resolves a `+typing` tag value, case-insensitively (`TypingPhaseFromTag`).
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

/// The kind of one inbox entry (`InboxKind`). The order mirrors the Go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboxKind {
    /// A channel line naming the user.
    Mention,
    /// A channel line the highlight rules matched.
    Highlight,
    /// A direct message.
    Direct,
    /// A channel invite.
    Invite,
    /// A monitor target coming online.
    MonitorOnline,
    /// A kick aimed at the user.
    Kick,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_keeps_short_values_and_drops_a_split_surrogate() {
        assert_eq!(clamped("writing docs"), "writing docs");
        // 511 ASCII bytes plus one 4-byte emoji is 515 bytes. The emoji's low
        // surrogate goes first, then its high surrogate, leaving 511 bytes.
        let value = format!("{}\u{1F600}", "a".repeat(511));
        assert_eq!(clamped(&value), "a".repeat(511));
    }

    #[test]
    fn canonical_key_matches_case_insensitively_and_rejects_strangers() {
        assert_eq!(canonical_key("STATUS"), Some("status"));
        assert_eq!(canonical_key("Display-Name"), Some("display-name"));
        assert_eq!(canonical_key("unknown"), None);
        assert_eq!(canonical_key(""), None);
    }

    #[test]
    fn shown_account_hides_placeholders_and_the_nick_itself() {
        assert_eq!(shown_account(false, "services"), "services");
        assert_eq!(shown_account(false, "*"), "");
        assert_eq!(shown_account(false, ""), "");
        assert_eq!(shown_account(true, "alice"), "");
    }

    #[test]
    fn membership_lines_read_the_way_the_go_does() {
        assert_eq!(join_line("alice", ""), "alice joined");
        assert_eq!(join_line("alice", "services"), "alice (services) joined");
        assert_eq!(part_line("alice"), "alice left");
        assert_eq!(quit_line("alice"), "alice quit");
        assert_eq!(nick_line("alice", "alicia"), "alice is now alicia");
    }

    #[test]
    fn typing_tag_is_case_insensitive_and_closed() {
        assert_eq!(typing_phase_from_tag("ACTIVE"), Some(TypingPhase::Active));
        assert_eq!(typing_phase_from_tag("Paused"), Some(TypingPhase::Paused));
        assert_eq!(typing_phase_from_tag("done"), Some(TypingPhase::Done));
        assert_eq!(typing_phase_from_tag("draft"), None);
    }
}
