//! Field readers over a parsed `IrcMessage`, shared by the session submodules.
//!
//! Port of the package helpers of `tui/internal/session/session.go` (`parameter`,
//! `tagValue`, `tagPresent`, `parameterIndex`, `capabilityTokens`, `claimedNick`,
//! `ctcpReplyHost`, `messageMsgID`) and `ircServerTimeOf` in
//! `tui/internal/irc/translator.go`. Every text value is decoded through
//! `wire_text`, so a stray byte cannot reach the state machine as anything but
//! a scalar.

use crate::irc::message::IrcMessage;
use crate::irc::wiretext::wire_text;

/// Parameter `index` as wire text, or empty when the message has no such
/// parameter.
pub(crate) fn param(message: &IrcMessage, index: usize) -> String {
    message
        .params
        .get(index)
        .map(|raw| wire_text(raw))
        .unwrap_or_default()
}

/// The index of the first parameter equal to `value` under ASCII case folding.
pub(crate) fn param_index(message: &IrcMessage, value: &str) -> Option<usize> {
    (0..message.params.len()).find(|&index| param(message, index).eq_ignore_ascii_case(value))
}

/// The value of the first tag named `name` that carries a value, as wire text.
/// Empty when the tag is absent or valueless.
pub(crate) fn tag_value(message: &IrcMessage, name: &str) -> String {
    message
        .tags
        .iter()
        .filter(|tag| tag.name == name)
        .find_map(|tag| tag.value.as_ref())
        .map(|raw| wire_text(raw))
        .unwrap_or_default()
}

/// Whether a tag named `name` is present, with or without a value.
/// `draft/chathistory-end` carries no value, so `tag_value` would miss it.
pub(crate) fn tag_present(message: &IrcMessage, name: &str) -> bool {
    message.tags.iter().any(|tag| tag.name == name)
}

/// The `msgid` tag value, or empty.
pub(crate) fn msgid(message: &IrcMessage) -> String {
    tag_value(message, "msgid")
}

/// The trailing parameter of a `CAP` line, split on spaces, after the
/// subcommand at `subcommand_index`. Empty fields are dropped.
pub(crate) fn capability_tokens(message: &IrcMessage, subcommand_index: usize) -> Vec<String> {
    if message.params.len() <= subcommand_index + 1 {
        return Vec::new();
    }
    let raw = wire_text(&message.params[message.params.len() - 1]);
    raw.split(' ')
        .filter(|field| !field.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The nick a message claims: its source nick, or the raw source when the
/// source names a server rather than a user. Empty without a source.
pub(crate) fn claimed_nick(message: &IrcMessage) -> String {
    let Some(prefix) = &message.prefix else {
        return String::new();
    };
    if !prefix.nick.is_empty() {
        return wire_text(&prefix.nick);
    }
    wire_text(&prefix.raw)
}

/// The host a CTCP reply is checked against: the source host, or the raw source
/// when there is no nick to route by. Empty otherwise.
pub(crate) fn ctcp_reply_host(message: &IrcMessage) -> String {
    let Some(prefix) = &message.prefix else {
        return String::new();
    };
    if !prefix.host.is_empty() {
        return wire_text(&prefix.host);
    }
    if prefix.nick.is_empty() {
        return wire_text(&prefix.raw);
    }
    String::new()
}

/// The IRCv3 `time` tag reader lives in the portable translator, beside the rest
/// of the inbound time rules. Re-exported so the session keeps its path.
pub(crate) use crate::irc::translator::server_time_of;
