//! Source and capability readers over a parsed `IrcMessage`, shared by the session
//! submodules.
//!
//! Port of the package helpers of `tui/internal/session/session.go` (`capabilityTokens`,
//! `claimedNick`, `ctcpReplyHost`). The field readers (`text_param`, `tag_text`,
//! `has_tag`, `msgid`, `param_index`) live on `IrcMessage` in `irc/message.rs`, the
//! one home for message helpers. Every text value is decoded through `wire_text`,
//! so a stray byte cannot reach the state machine as anything but a scalar.

use crate::irc::message::IrcMessage;
use crate::irc::wiretext::wire_text;

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
