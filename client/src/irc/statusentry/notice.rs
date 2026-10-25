//! NOTICE and INVITE lines: the speaker is named in front of the notice body, and a
//! bare notice keeps its own text.

use super::incoming::{command_of, trailing_text};
use crate::irc::message::IrcMessage;
use crate::irc::prefixnick::prefix_nick;
use crate::irc::wiretext::wire_text;

/// The Status copy of a notice: wrapped with its speaker, or bare
/// (`statusNoticeStatusCopy`).
pub(super) enum NoticeCopy {
    Wrapped { speaker: String, body: String },
    Bare { body: String },
}

impl NoticeCopy {
    /// The Status text: `-<speaker>- <body>`, or the body alone.
    pub(super) fn text(&self) -> String {
        match self {
            NoticeCopy::Wrapped { speaker, body } => format!("-{speaker}- {body}"),
            NoticeCopy::Bare { body } => body.clone(),
        }
    }
}

/// The speaker token of a notice, trimmed. `*` and empty are not speakers
/// (`statusTryMakeNoticeSpeaker`).
fn notice_speaker(token: &str) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("*") {
        return None;
    }
    Some(trimmed.to_owned())
}

/// The wire pieces of an incoming NOTICE (`statusNoticeWireParts`).
pub(super) struct NoticeWire {
    prefix_nick: Option<String>,
    prefix_raw: Option<String>,
    target: String,
    body: String,
}

/// Reads the pieces of an incoming NOTICE (`statusParseIncomingNotice`).
pub(super) fn parse_incoming_notice(message: &IrcMessage) -> Option<NoticeWire> {
    if command_of(message) != "NOTICE" || message.params.is_empty() {
        return None;
    }
    let (prefix_nick, prefix_raw) = match &message.prefix {
        Some(prefix) => (
            (!prefix.nick.is_empty()).then(|| wire_text(&prefix.nick)),
            (!prefix.raw.is_empty()).then(|| wire_text(&prefix.raw)),
        ),
        None => (None, None),
    };
    let body = if message.params.len() >= 2 {
        trailing_text(message)
    } else {
        String::new()
    };
    Some(NoticeWire {
        prefix_nick,
        prefix_raw,
        target: wire_text(&message.params[0]),
        body,
    })
}

/// Chooses the notice speaker: the nick, then the raw source, then a target that
/// names a speaker (`statusResolveIncomingNoticeSpeaker`). Then presents the copy.
pub(super) fn present_notice(parts: &NoticeWire) -> NoticeCopy {
    let speaker = parts
        .prefix_nick
        .as_deref()
        .and_then(notice_speaker)
        .or_else(|| parts.prefix_raw.as_deref().and_then(notice_speaker))
        .or_else(|| notice_speaker(&parts.target));
    match speaker {
        Some(speaker) => NoticeCopy::Wrapped {
            speaker,
            body: parts.body.clone(),
        },
        None => NoticeCopy::Bare {
            body: parts.body.clone(),
        },
    }
}

/// The Status text of an incoming INVITE (`statusPresentIncomingInvite`).
pub(super) fn present_invite(message: &IrcMessage) -> Option<String> {
    if command_of(message) != "INVITE" || message.params.len() < 2 {
        return None;
    }
    let nick = prefix_nick(message);
    let channel = trailing_text(message);
    if nick.is_empty() || channel.is_empty() {
        return None;
    }
    Some(format!("{nick} invited you to {channel}"))
}
