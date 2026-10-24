//! Outcome and status strings for reply-router watches (`replyrouter.rs`).

use crate::irc::command::{CommandOutcome, ComposerSurface, Words};
use crate::irc::message::IrcMessage;
use crate::irc::presence::{AVATAR_KEY, STATUS_KEY};
use crate::irc::wiretext::wire_text;

/// The outcome for a verb whose network the surface cannot name: a conversation with no
/// selection is a wrong scope, and Status with no network is a refusal.
pub(super) fn refused_or_wrong_scope(surface: ComposerSurface) -> CommandOutcome {
    match surface {
        ComposerSurface::Conversation => CommandOutcome::WrongScope,
        ComposerSurface::Status => CommandOutcome::Refused,
    }
}

/// One message parameter as text, or "" when it is absent (`parameterText`).
pub(super) fn param_text(message: &IrcMessage, index: usize) -> String {
    message.params.get(index).map(|raw| wire_text(raw)).unwrap_or_default()
}

/// Whether an argument is exactly `clear` (`isOwnMetadataClearAlias`).
pub(super) fn is_clear_alias(argument: &str) -> bool {
    let words = Words::split(argument);
    words.first.eq_ignore_ascii_case("clear") && words.rest.is_empty()
}

/// The success text for a cleared key (`ownMetadataClearedMessage`).
pub(super) fn cleared_message(canonical: &str) -> String {
    match canonical {
        STATUS_KEY => "Standing status cleared.".to_owned(),
        AVATAR_KEY => "Avatar cleared.".to_owned(),
        _ => String::new(),
    }
}

/// The success text for a set key (`ownMetadataSetMessage`).
pub(super) fn set_message(canonical: &str, value: &str) -> String {
    match canonical {
        STATUS_KEY => format!("Standing status set to {value}."),
        AVATAR_KEY => format!("Avatar set to {value}."),
        _ => String::new(),
    }
}

/// The failure text for a refused set (`ownMetadataSetFailMessage`).
pub(super) fn set_fail_message(canonical: &str, reason: &str) -> String {
    match canonical {
        STATUS_KEY => format!("Could not set standing status: {reason}"),
        AVATAR_KEY => format!("Could not set avatar: {reason}"),
        _ => String::new(),
    }
}

/// The failure text for a refused clear (`ownMetadataClearFailMessage`).
pub(super) fn clear_fail_message(canonical: &str, reason: &str) -> String {
    match canonical {
        STATUS_KEY => format!("Could not clear standing status: {reason}"),
        AVATAR_KEY => format!("Could not clear avatar: {reason}"),
        _ => String::new(),
    }
}

/// The text for a network without the capability (`ownMetadataNoCapMessage`).
pub(super) fn no_capability_message(key: &str) -> String {
    match key {
        STATUS_KEY => "This network does not support standing status.".to_owned(),
        _ => "This network does not support avatars.".to_owned(),
    }
}

/// The text for a network whose value budget is zero (`ownMetadataNoValueMessage`).
pub(super) fn no_value_message(key: &str) -> String {
    match key {
        STATUS_KEY => "This network does not allow standing status text.".to_owned(),
        _ => "This network does not allow avatar URLs.".to_owned(),
    }
}

/// The text for an inspect with nothing stored (`ownMetadataInspectEmptyMessage`).
pub(super) fn inspect_empty_message(key: &str) -> String {
    match key {
        STATUS_KEY => "No standing status. Use /status <text> or /status clear.".to_owned(),
        _ => "No standing avatar. Use /avatar <url|email> or /avatar clear.".to_owned(),
    }
}

/// The text for an inspect with a stored value (`ownMetadataInspectValueMessage`).
pub(super) fn inspect_value_message(key: &str, value: &str) -> String {
    match key {
        STATUS_KEY => format!("Standing status: {value}"),
        _ => format!("Standing avatar: {value}"),
    }
}

/// Whether a labeled standard reply is worth copying into the transcript: a FAIL, WARN or
/// NOTE, or a three-digit 4xx or 5xx (`copiesLabeledStandardReply`).
pub(super) fn copies_labeled_standard_reply(label: &str) -> bool {
    if matches!(label, "FAIL" | "WARN" | "NOTE") {
        return true;
    }
    let bytes = label.as_bytes();
    bytes.len() == 3 && bytes.iter().all(u8::is_ascii_digit) && matches!(bytes[0], b'4' | b'5')
}

/// Substitutes `%1` and `%2` in `pattern`, scanning the pattern only, so a substituted
/// value is never expanded again (`replyFormatPair`).
pub(super) fn format_pair(pattern: &str, first: &str, second: &str) -> String {
    let mut out = String::new();
    let mut rest = pattern;
    while let Some(index) = rest.find('%') {
        out.push_str(&rest[..index]);
        let after = &rest[index..];
        match after.as_bytes().get(1) {
            Some(b'1') => {
                out.push_str(first);
                rest = &after[2..];
            }
            Some(b'2') => {
                out.push_str(second);
                rest = &after[2..];
            }
            _ => {
                out.push('%');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}
