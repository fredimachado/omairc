//! The chat lines: NOTICE, PRIVMSG (a CTCP ACTION is an emote) and TAGMSG (typing).
//! A nick-targeted line that carries `+draft/channel-context` is filed in that channel
//! visible only to the user, when the sender is a member the caller can confirm.

use crate::irc::ctcp::parse_ctcp_request;
use crate::irc::event::{ActionEvent, Event, MessageEvent, MsgId, NoticeEvent, TypingEvent};
use crate::irc::message::IrcMessage;
use crate::irc::conversation::target_looks_like_service;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::servicenick::is_network_notice_target;
use crate::irc::typing::typing_phase_from_tag;

use super::context::{conversation_for, key, same, Inbound};

/// The longest channel name a `+draft/channel-context` tag may carry: the
/// classic RFC 2811 limit. A longer tag is not a channel key. Keep in lockstep
/// with Go `channelContextLimit`.
pub const CHANNEL_CONTEXT_LIMIT: usize = 50;

/// The CTCP ACTION prefix, including the trailing space.
const ACTION_PREFIX: &str = "\u{1}ACTION ";

/// A nick-targeted line that belongs in a joined channel buffer, shown under that
/// buffer's own name.
struct ChannelDelivery {
    channel: String,
    only_visible_to_you: bool,
}

/// `NOTICE`. A channel-context notice or a nick-targeted direct notice produces a
/// transcript event; channel notices without context and CTCP only update the account
/// which `translate` has already recorded.
pub(super) fn notice(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let body = message.text_param(1);
    if parse_ctcp_request(&body).is_some() {
        return Vec::new();
    }
    let wire_target = message.text_param(0);
    let features = inbound.features;
    let msgid = MsgId::new(message.msgid());
    let sender = &inbound.sender;
    if let Some(delivery) = nick_targeted_channel_delivery(&wire_target, inbound) {
        return vec![NoticeEvent {
            conversation: key(inbound.network_id, &delivery.channel, features),
            author: sender.clone(),
            body,
            timestamp: inbound.timestamp,
            server_time: inbound.server_time,
            target: delivery.channel,
            msgid,
            only_visible_to_you: delivery.only_visible_to_you,
        }
        .into()];
    }
    if features.is_channel(&wire_target) {
        return Vec::new();
    }
    for name in ["+draft/channel-context", "draft/channel-context"] {
        if message.has_tag(name) {
            return Vec::new();
        }
    }
    let Some(conversation) = conversation_for(
        inbound.network_id,
        &wire_target,
        message,
        inbound.current_nick,
        features,
    ) else {
        return Vec::new();
    };
    let display_target =
        if !features.is_channel(&wire_target) && !same(features, sender, inbound.current_nick) {
            sender.clone()
        } else {
            wire_target.clone()
        };
    if is_network_notice_target(&display_target)
        || target_looks_like_service(&display_target, features)
    {
        return Vec::new();
    }
    vec![NoticeEvent {
        conversation,
        author: sender.clone(),
        body,
        timestamp: inbound.timestamp,
        server_time: inbound.server_time,
        target: display_target,
        msgid,
        only_visible_to_you: false,
    }
    .into()]
}

/// `PRIVMSG`. Any CTCP other than ACTION is dropped.
pub(super) fn privmsg(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let features = inbound.features;
    let wire_target = message.text_param(0);
    let body = message.text_param(1);
    if let Some(ctcp) = parse_ctcp_request(&body) {
        if ctcp.command != "ACTION" {
            return Vec::new();
        }
    }
    let msgid = MsgId::new(message.msgid());
    let action = action_text(&body);
    let sender = &inbound.sender;
    if let Some(delivery) = nick_targeted_channel_delivery(&wire_target, inbound) {
        let conversation = key(inbound.network_id, &delivery.channel, features);
        return match action {
            Some(text) => vec![ActionEvent {
                conversation,
                author: sender.clone(),
                body: text.to_owned(),
                timestamp: inbound.timestamp,
                server_time: inbound.server_time,
                target: delivery.channel,
                msgid,
                only_visible_to_you: delivery.only_visible_to_you,
            }
            .into()],
            None => vec![MessageEvent {
                conversation,
                author: sender.clone(),
                body,
                timestamp: inbound.timestamp,
                server_time: inbound.server_time,
                target: delivery.channel,
                msgid,
                only_visible_to_you: delivery.only_visible_to_you,
            }
            .into()],
        };
    }
    let Some(conversation) = conversation_for(
        inbound.network_id,
        &wire_target,
        message,
        inbound.current_nick,
        features,
    ) else {
        return Vec::new();
    };
    let display_target =
        if !features.is_channel(&wire_target) && !same(features, sender, inbound.current_nick) {
            sender.clone()
        } else {
            wire_target
        };
    match action {
        Some(text) => vec![ActionEvent {
            conversation,
            author: sender.clone(),
            body: text.to_owned(),
            timestamp: inbound.timestamp,
            server_time: inbound.server_time,
            target: display_target,
            msgid,
            only_visible_to_you: false,
        }
        .into()],
        None => vec![MessageEvent {
            conversation,
            author: sender.clone(),
            body,
            timestamp: inbound.timestamp,
            server_time: inbound.server_time,
            target: display_target,
            msgid,
            only_visible_to_you: false,
        }
        .into()],
    }
}

/// `TAGMSG` carrying `+typing`. Any other tag, or a phase that is not one, is dropped.
pub(super) fn tagmsg(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let Some(value) = message.tag_text("+typing") else {
        return Vec::new();
    };
    if inbound.sender.is_empty() {
        return Vec::new();
    }
    let Some(phase) = typing_phase_from_tag(&value) else {
        return Vec::new();
    };
    let Some(conversation) = conversation_for(
        inbound.network_id,
        &message.text_param(0),
        message,
        inbound.current_nick,
        inbound.features,
    ) else {
        return Vec::new();
    };
    vec![TypingEvent {
        conversation,
        nick: inbound.sender.clone(),
        phase,
        received_at: inbound.timestamp,
    }
    .into()]
}

/// Routes a nick-targeted line that carries `+draft/channel-context` into the
/// channel buffer when the sender is a member of it. The context must be one
/// channel name, and the sender must be a member the caller can confirm.
fn nick_targeted_channel_delivery(
    wire_target: &str,
    inbound: &Inbound<'_>,
) -> Option<ChannelDelivery> {
    let features = inbound.features;
    let context = channel_context_channel(inbound.message, features)?;
    if features.is_channel(wire_target) || !same(features, wire_target, inbound.current_nick) {
        return None;
    }
    let sender = &inbound.sender;
    if sender.is_empty() || same(features, &context, inbound.current_nick) {
        return None;
    }
    let lookup = inbound.joined?;
    let canonical = lookup(&context, sender)?;
    if canonical.is_empty() || !features.is_channel(&canonical) {
        return None;
    }
    Some(ChannelDelivery {
        channel: canonical,
        only_visible_to_you: true,
    })
}

/// The channel named by `+draft/channel-context`, or the unprefixed draft tag
/// when the value is one channel name (`channelContextChannel`).
fn channel_context_channel(message: &IrcMessage, features: &IrcServerFeatures) -> Option<String> {
    for name in ["+draft/channel-context", "draft/channel-context"] {
        if let Some(value) = message.tag_text(name) {
            if channel_context_text(&value, features) {
                return Some(value);
            }
        }
    }
    None
}

/// Whether `value` is one channel name: a CHANTYPES prefix, two to fifty runes
/// and no control character, space, comma or colon (`channelContextText`).
fn channel_context_text(value: &str, features: &IrcServerFeatures) -> bool {
    let length = value.chars().count();
    if !(2..=CHANNEL_CONTEXT_LIMIT).contains(&length) || !features.is_channel(value) {
        return false;
    }
    value.chars().all(|character| {
        !(character < '\u{20}'
            || character == '\u{7F}'
            || character.is_whitespace()
            || character == ','
            || character == ':')
    })
}

/// The body of a CTCP ACTION, or `None` when the body is not one (`ACTION_PREFIX` test).
pub(super) fn action_text(body: &str) -> Option<&str> {
    if body.starts_with(ACTION_PREFIX) && body.ends_with('\u{1}') {
        return Some(&body[ACTION_PREFIX.len()..body.len() - 1]);
    }
    None
}
