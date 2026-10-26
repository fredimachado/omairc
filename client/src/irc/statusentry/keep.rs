//! The keep table: which wire lines reach Status at all. Protocol chatter is dropped
//! replies are kept, and a direct message or channel line stays out of Status when the
//! conversation it belongs to is engaged.

use super::incoming::{
    command_of, first_token, has_prefix_fold, is_numeric_command, trailing_text,
};
use super::outgoing::strip_line_end;
use super::whois::whois_spec;
use crate::irc::ctcp::parse_ctcp_request;
use crate::irc::message::{IrcMessage, IrcPrefix};
use crate::irc::prefixnick::prefix_nick;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::servicenick::{
    is_network_notice_target, is_service_identity, is_service_source, nick_is_routable,
};
use crate::irc::wiretext::wire_text;

/// What the keep table does with a line (`statusKeep`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Keep {
    Drop,
    Keep,
    IncomingPrivmsg,
}

/// One keep-table row (`statusKeepRow`).
pub(super) struct KeepRow {
    pub(super) command: &'static str,
    pub(super) incoming: Keep,
    pub(super) outgoing: Keep,
}

/// The keep table, mirroring `kStatusKeep` in `tui/internal/irc/statusentry.go`. Each
/// command is uppercase and appears once; `keep_rows_are_well_formed` checks it.
pub(super) static KEEP_ROWS: [KeepRow; 25] = [
    KeepRow {
        command: "PING",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "PONG",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "CAP",
        incoming: Keep::Keep,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "AUTHENTICATE",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "JOIN",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "PART",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "QUIT",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "NICK",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "KICK",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "TOPIC",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "MODE",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "BATCH",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "TAGMSG",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "AWAY",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "CHATHISTORY",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "MARKREAD",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "READ",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "PRIVMSG",
        incoming: Keep::IncomingPrivmsg,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "NOTICE",
        incoming: Keep::Keep,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "FAIL",
        incoming: Keep::Keep,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "WARN",
        incoming: Keep::Keep,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "NOTE",
        incoming: Keep::Keep,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "ERROR",
        incoming: Keep::Keep,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "INVITE",
        incoming: Keep::Keep,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "PASS",
        incoming: Keep::Drop,
        outgoing: Keep::Keep,
    },
];

/// The keep-table row for a command (`statusKeepRowFor`).
fn keep_row(command: &str) -> Option<&'static KeepRow> {
    KEEP_ROWS.iter().find(|row| row.command == command)
}

/// The feature set the classifier uses when only CHANTYPES is on hand
/// (`statusFeaturesForChannelTypes`). The other fields keep their defaults.
fn features_for_channel_types(channel_types: &str) -> IrcServerFeatures {
    let mut features = IrcServerFeatures::new();
    if !channel_types.is_empty() {
        features.apply_token(&format!("CHANTYPES={channel_types}"));
    }
    features
}

/// Whether a direct message belongs to a conversation the user is in
/// (`statusConversationEngaged`, the part of `ircConversationFor` that Status
/// consults).
fn conversation_engaged(
    message: &IrcMessage,
    target: &str,
    current_nick: &str,
    features: &IrcServerFeatures,
) -> bool {
    if let Some(prefix) = &message.prefix {
        if !prefix.nick.is_empty() && !nick_is_routable(&wire_text(&prefix.nick)) {
            return false;
        }
    }
    if features.is_channel(target) {
        return true;
    }
    if is_network_notice_target(target)
        || !message.prefix.as_ref().is_some_and(IrcPrefix::is_user)
        || is_service_source(message, features)
    {
        return false;
    }
    let sender = prefix_nick(message);
    if sender.is_empty() {
        return false;
    }
    let mapping = features.case_mapping();
    mapping.equals(target, current_nick) || mapping.equals(&sender, current_nick)
}

/// Whether a numeric is one Status keeps (`statusKeepsIncomingNumeric`).
fn keeps_numeric(command: &str) -> bool {
    if !is_numeric_command(command) {
        return false;
    }
    let code: u32 = command.parse().unwrap_or(0);
    if matches!(code, 1..=4) {
        return true;
    }
    if matches!(
        code,
        20 | 42
            | 221
            | 250
            | 251
            | 252
            | 253
            | 254
            | 255
            | 263
            | 265
            | 266
            | 396
            | 372
            | 375
            | 376
            | 422
            | 764
            | 767
            | 769
            | 903
            | 904
            | 905
    ) {
        return true;
    }
    if whois_spec(code).is_some() {
        return true;
    }
    matches!(command.as_bytes()[0], b'4' | b'5')
}

/// Whether a PRIVMSG belongs in Status (`statusKeepsIncomingPrivmsg`). Channel
/// chat stays in its channel. A services line, a CTCP other than ACTION, and a
/// direct message the user is not engaged in do go to Status.
fn keeps_privmsg(message: &IrcMessage, current_nick: &str, channel_types: &str) -> bool {
    if message.params.len() >= 2 {
        if let Some(request) = parse_ctcp_request(&trailing_text(message)) {
            if request.command != "ACTION" {
                return true;
            }
        }
    }

    let features = features_for_channel_types(channel_types);
    let types = features.channel_types();
    let target = message
        .params
        .first()
        .map_or_else(String::new, |p| wire_text(p));
    // Channel chat belongs in the channel transcript. Ergo replays join and quit
    // history as HistServ PRIVMSG to the channel; the service-nick rule below
    // would otherwise flood Status with that replay.
    if features.is_channel(&target) {
        return false;
    }
    let (sender, host) = match &message.prefix {
        Some(prefix) => (wire_text(&prefix.nick), wire_text(&prefix.host)),
        None => (String::new(), String::new()),
    };
    if is_service_identity(&sender, &host, types) || is_service_identity(&target, "", types) {
        return true;
    }
    !conversation_engaged(message, &target, current_nick, &features)
}

/// Whether an incoming message belongs in Status (`ircStatusKeepsIncoming`).
pub fn keeps_incoming(message: &IrcMessage, current_nick: &str, channel_types: &str) -> bool {
    let command = command_of(message);
    match keep_row(&command) {
        Some(row) => match row.incoming {
            Keep::Keep => true,
            Keep::Drop => false,
            Keep::IncomingPrivmsg => keeps_privmsg(message, current_nick, channel_types),
        },
        None => keeps_numeric(&command),
    }
}

/// Whether an outgoing line belongs in Status (`ircStatusKeepsOutgoing`). The
/// CAP REQ the client sent is kept; other CAP lines are not.
pub fn keeps_outgoing(line: &[u8]) -> bool {
    let display = wire_text(strip_line_end(line));
    let verb = first_token(&display);
    if verb.is_empty() {
        return false;
    }
    let Some(row) = keep_row(&verb) else {
        return false;
    };
    if row.outgoing == Keep::Keep {
        return true;
    }
    if verb == "CAP" {
        let trimmed = display.trim();
        if trimmed.len() > 3 {
            if let Some(rest) = trimmed.get(3..) {
                return has_prefix_fold(rest.trim(), "REQ");
            }
        }
    }
    false
}
