//! Numerics and metadata: the WHO and WHOIS replies that fill a member's facts (352
//! 311, 313, 330), METADATA with its 761 and 766 replies, and the numerics that refuse
//! a join. A join refusal names a channel, so it becomes a `ChannelErrorEvent` on that
//! channel.

use crate::irc::event::{
    AccountEvent, AwayEvent, ChannelErrorEvent, Event, MemberMetadataEvent, NickFactsEvent,
};
use crate::irc::message::IrcMessage;
use crate::irc::presence::{canonical_key, clamped, Away};
use crate::irc::serverfeatures::IrcServerFeatures;

use super::context::Inbound;

/// `352` WHO reply: the away flag and the real name, and the operator flag as a
/// separate fact.
pub(super) fn who_reply(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let nick = message.text_param(5);
    let flags = message.text_param(6);
    let away = flags.starts_with('G');
    if nick.is_empty() {
        return Vec::new();
    }
    let trailing = message.text_param(7);
    let mut realname = who_reply_realname(&trailing);
    if realname.is_empty() {
        realname = realname_from_who(&trailing);
    }
    vec![
        AwayEvent {
            network_id: inbound.network_id.to_owned(),
            nick: nick.clone(),
            away: away.then(Away::default),
            realname: (!realname.is_empty()).then_some(realname),
            realname_only: false,
        }
        .into(),
        // Operator is a separate fact. The realname stays on the away event
        // so a placeholder cannot wipe a name a join already stored.
        NickFactsEvent {
            network_id: inbound.network_id.to_owned(),
            nick,
            realname: None,
            server_operator: Some(flags.contains('*')),
        }
        .into(),
    ]
}

/// `311` WHOIS user line: the real name, with the nick's away state left alone.
pub(super) fn whois_realname(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let nick = message.text_param(1);
    let realname = message.text_param(5).trim().to_owned();
    if nick.is_empty() || realname.is_empty() {
        return Vec::new();
    }
    vec![AwayEvent {
        network_id: inbound.network_id.to_owned(),
        nick,
        away: None,
        realname: Some(realname),
        realname_only: true,
    }
    .into()]
}

/// `313` WHOIS operator line: the nick is a server operator.
pub(super) fn whois_operator(inbound: &Inbound<'_>) -> Vec<Event> {
    let nick = inbound.message.text_param(1);
    if nick.is_empty() {
        return Vec::new();
    }
    vec![NickFactsEvent {
        network_id: inbound.network_id.to_owned(),
        nick,
        realname: None,
        server_operator: Some(true),
    }
    .into()]
}

/// `330` WHOIS account line: the nick's account.
pub(super) fn whois_account(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let nick = message.text_param(1);
    if nick.is_empty() {
        return Vec::new();
    }
    vec![AccountEvent {
        network_id: inbound.network_id.to_owned(),
        nick,
        account: message.text_param(2),
    }
    .into()]
}

/// `METADATA` (`target_index` 0) and `761`, which carry the same fields one word later.
pub(super) fn metadata(inbound: &Inbound<'_>, target_index: usize) -> Vec<Event> {
    member_metadata(
        inbound.network_id,
        inbound.message,
        target_index,
        inbound.features,
    )
    .into_iter()
    .collect()
}

/// `766 RPL_KEYNOTSET`: a key cleared on a member, with no value.
pub(super) fn key_not_set(inbound: &Inbound<'_>) -> Vec<Event> {
    key_not_set_event(inbound.network_id, inbound.message, inbound.features)
        .into_iter()
        .collect()
}

/// Whether `command` is a numeric that refuses a join, the definite ones and the
/// ambiguous ones alike.
pub(super) fn is_join_failure(command: &str) -> bool {
    is_join_failure_numeric(command) || is_ambiguous_join_failure_numeric(command)
}

/// A join refusal on the channel named in the first parameter. The ambiguous numerics
/// (437, 480, 485) can name a nick instead, so they count only for a channel name.
pub(super) fn join_failure(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let channel = message.text_param(1);
    if is_ambiguous_join_failure_numeric(inbound.command) && !inbound.features.is_channel(&channel)
    {
        return Vec::new();
    }
    vec![ChannelErrorEvent {
        network_id: inbound.network_id.to_owned(),
        channel,
        body: message.text_param(2),
    }
    .into()]
}

/// `METADATA` and `761`: `<Target> <Key> <Visibility> [<Value>]`, read from
/// `target_index`. A channel target, an unknown key or an empty value yields
/// nothing (`appendMemberMetadata`).
fn member_metadata(
    network_id: &str,
    message: &IrcMessage,
    target_index: usize,
    features: &IrcServerFeatures,
) -> Option<Event> {
    if message.params.len() < target_index + 3 {
        return None;
    }
    let target = message.text_param(target_index);
    let key_text = message.text_param(target_index + 1);
    if target.is_empty() || features.is_channel(&target) {
        return None;
    }
    let canonical = canonical_key(&key_text)?;
    let value = clamped(&message.text_param(target_index + 3));
    if value.is_empty() {
        return None;
    }
    Some(
        MemberMetadataEvent {
            network_id: network_id.to_owned(),
            nick: target,
            key: canonical.to_owned(),
            value,
        }
        .into(),
    )
}

/// `766 RPL_KEYNOTSET`: `<Target> <Key> [:reason]`. It clears one known key on a
/// GET miss, a successful SET unset, or a CLEAR batch. Replies that omit `<Key>`
/// are ignored, and the trailing text is never taken as the key (`append766KeyNotSet`).
fn key_not_set_event(
    network_id: &str,
    message: &IrcMessage,
    features: &IrcServerFeatures,
) -> Option<Event> {
    const TARGET_INDEX: usize = 1;
    if message.params.len() < TARGET_INDEX + 2 {
        return None;
    }
    let target = message.text_param(TARGET_INDEX);
    let key_text = message.text_param(TARGET_INDEX + 1);
    if target.is_empty() || features.is_channel(&target) {
        return None;
    }
    let canonical = canonical_key(&key_text)?;
    Some(
        MemberMetadataEvent {
            network_id: network_id.to_owned(),
            nick: target,
            key: canonical.to_owned(),
            value: String::new(),
        }
        .into(),
    )
}

/// The numerics that refuse a join on a channel, whatever the target (`isJoinFailureNumeric`).
fn is_join_failure_numeric(command: &str) -> bool {
    matches!(
        command,
        "403"
            | "405"
            | "448"
            | "471"
            | "473"
            | "474"
            | "475"
            | "476"
            | "477"
            | "479"
            | "489"
            | "520"
    )
}

fn is_ambiguous_join_failure_numeric(command: &str) -> bool {
    matches!(command, "437" | "480" | "485")
}

/// The MODE-free `352` real name: `<hopcount> <real name>`. A lone hop count is
/// empty, and a trailing value with no hop count is the name itself
/// (`whoReplyRealname`). A signed hop count counts, as Go's `strconv.Atoi` does.
fn who_reply_realname(trailing: &str) -> String {
    let trimmed = trailing.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match trimmed.split_once(' ') {
        None => {
            if is_go_int(trimmed) {
                String::new()
            } else {
                trimmed.to_owned()
            }
        }
        Some((hop, rest)) => {
            if !is_go_int(hop) {
                trimmed.to_owned()
            } else {
                rest.trim().to_owned()
            }
        }
    }
}

/// Splits `RPL_WHOREPLY`'s trailing `<hopcount> <real name>` (`realnameFromWho`).
/// A purely numeric first token is the hop count; anything else is the name.
fn realname_from_who(trailing: &str) -> String {
    let trimmed = trailing.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match trimmed.split_once(' ') {
        None => {
            if all_digits(trimmed) {
                String::new()
            } else {
                trimmed.to_owned()
            }
        }
        Some((head, rest)) => {
            if !all_digits(head) {
                trimmed.to_owned()
            } else {
                rest.trim().to_owned()
            }
        }
    }
}

/// Whether `value` parses as a Go `strconv.Atoi` integer.
pub(super) fn is_go_int(value: &str) -> bool {
    value.parse::<i64>().is_ok()
}

/// Whether `value` is one or more ASCII digits.
pub(super) fn all_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}
