//! Batch replay. `translate_history` turns one completed history or playback batch into
//! the `HistoryEvent` the session files. A replay also keeps two kinds of line a live
//! translation would drop: a channel batch's joins, parts, quits and nicks as event
//! lines, and a bouncer query's lines sent under a nick the user has since changed.

use chrono::{DateTime, Utc};

use crate::irc::ctcp::parse_ctcp_request;
use crate::irc::event::{Event, HistoryEvent, HistoryKind, MessageKindTag, MsgId, ReplayLine};
use crate::irc::membershipnoise::{join_line, nick_line, part_line, quit_line, shown_account};
use crate::irc::message::{IrcMessage, IrcPrefix};
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::servicenick::{is_network_notice_target, is_service_source};

use super::context::{
    author, key, routable_source, same, server_time_of, timestamp_for, JoinedMemberLookup,
};
use super::messages::action_text;
use super::translate;

/// One row of `CHATHISTORY TARGETS` (`IrcHistoryTarget`). `latest` is the
/// server's time of that target's newest message, when the reply gave one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryTarget {
    pub name: String,
    pub latest: Option<DateTime<Utc>>,
}

/// One completed replay batch (`IrcHistoryBatch`). The lines are still raw
/// protocol, so the translator applies the same rules it applies to live traffic.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryBatch {
    pub target: String,
    pub lines: Vec<IrcMessage>,
    /// `None` is an unknown reply; the translator treats it as `Chat`.
    pub kind: Option<HistoryKind>,
    /// True for a solicited `CHATHISTORY BEFORE` answer.
    pub older_page: bool,
    /// True when `draft/chathistory-end` was on the opening `BATCH`.
    pub history_ended: bool,
    /// True when the batch answers `CHATHISTORY AFTER`.
    pub after_request: bool,
    /// The rows of a `Targets` batch. Empty for every other kind.
    pub targets: Vec<HistoryTarget>,
}

/// Replays one completed batch into a `HistoryEvent` (`IrcEventTranslator::translateHistory`).
/// `None` for a `Targets` batch or one without a target. Each line keeps only the
/// conversation it belongs to, and a bouncer query also keeps lines the user sent
/// under a nick they have since changed.
pub fn translate_history(
    network_id: &str,
    current_nick: &str,
    features: &IrcServerFeatures,
    batch: &HistoryBatch,
    now: DateTime<Utc>,
    joined: Option<JoinedMemberLookup<'_>>,
) -> Option<HistoryEvent> {
    if batch.kind == Some(HistoryKind::Targets) || batch.target.is_empty() {
        return None;
    }
    let conversation = key(network_id, &batch.target, features);
    // Channel batches already key off the channel. CHATHISTORY keeps the live nick
    // check; only a bouncer query needs the previous-nick rule. A query batch
    // never relocates a channel-context line: relocation is only for a channel
    // batch whose buffer we joined.
    let channel_batch = features.is_channel(&batch.target);
    let history_lookup = if channel_batch { joined } else { None };
    let bouncer_query = batch.kind == Some(HistoryKind::BouncerPlayback) && !channel_batch;
    let mut event = HistoryEvent {
        conversation: conversation.clone(),
        target: batch.target.clone(),
        lines: Vec::new(),
        kind: batch.kind.unwrap_or_default(),
        older_page: batch.older_page,
    };
    for line in &batch.lines {
        let server_time = server_time_of(line);
        let mut kept = false;
        let translated = translate(
            network_id,
            current_nick,
            features,
            line,
            now,
            history_lookup,
        );
        for translated_event in &translated {
            match translated_event {
                Event::Message(value) => {
                    if value.conversation != conversation {
                        continue;
                    }
                    event.lines.push(ReplayLine {
                        author: value.author.clone(),
                        body: value.body.clone(),
                        timestamp: value.timestamp,
                        kind: MessageKindTag::Chat,
                        msgid: value.msgid.clone(),
                        server_time,
                        only_visible_to_you: value.only_visible_to_you,
                    });
                    kept = true;
                }
                Event::Action(value) => {
                    if value.conversation != conversation {
                        continue;
                    }
                    event.lines.push(ReplayLine {
                        author: value.author.clone(),
                        body: value.body.clone(),
                        timestamp: value.timestamp,
                        kind: MessageKindTag::Emote,
                        msgid: value.msgid.clone(),
                        server_time,
                        only_visible_to_you: value.only_visible_to_you,
                    });
                    kept = true;
                }
                _ => {}
            }
        }
        if !kept {
            if let Some(membership) = channel_membership_replay_line(
                &batch.target,
                features,
                line,
                &translated,
                server_time,
                now,
            ) {
                event.lines.push(membership);
                kept = true;
            }
        }
        if kept || !bouncer_query {
            continue;
        }
        if let Some(replay) =
            bouncer_query_replay_line(&batch.target, features, line, server_time, now)
        {
            event.lines.push(replay);
        }
    }
    Some(event)
}

/// A join, part, quit or nick on a channel batch, filed as a replayed event line.
/// The author stays empty so playback does not open a query. Membership is not
/// changed (`channelMembershipReplayLine`).
fn channel_membership_replay_line(
    batch_target: &str,
    features: &IrcServerFeatures,
    message: &IrcMessage,
    translated: &[Event],
    server_time: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<ReplayLine> {
    if !features.is_channel(batch_target) {
        return None;
    }
    let msgid = MsgId::new(message.msgid());
    let timestamp = timestamp_for(message, now);
    let line = |body: String| ReplayLine {
        author: String::new(),
        body,
        timestamp,
        kind: MessageKindTag::Event,
        msgid: msgid.clone(),
        server_time,
        only_visible_to_you: false,
    };
    let same_target = |channel: &str| same(features, channel, batch_target);
    for translated_event in translated {
        match translated_event {
            Event::Join(value) => {
                if value.nick.is_empty() || !same_target(&value.channel) {
                    return None;
                }
                let shown = value
                    .account
                    .as_deref()
                    .map(|account| shown_account(same(features, account, &value.nick), account))
                    .unwrap_or_default();
                return Some(line(join_line(&value.nick, &shown)));
            }
            Event::Part(value) => {
                if value.nick.is_empty() || !same_target(&value.channel) {
                    return None;
                }
                return Some(line(part_line(&value.nick)));
            }
            Event::Quit(value) => {
                if value.nick.is_empty() {
                    return None;
                }
                return Some(line(quit_line(&value.nick)));
            }
            Event::Nick(value) => {
                if value.old_nick.is_empty() || value.new_nick.is_empty() {
                    return None;
                }
                return Some(line(nick_line(&value.old_nick, &value.new_nick)));
            }
            _ => {}
        }
    }
    None
}

/// A `znc.in/playback` query line on the batch conversation (`bouncerQueryReplayLine`).
/// A playback query buffer is named for the peer, so after a nick change the same
/// buffer still has PRIVMSGs to the previous nick and echoes from that nick. A
/// channel target stays with its channel batch, and live PRIVMSG still has to
/// name the current nick.
fn bouncer_query_replay_line(
    batch_target: &str,
    features: &IrcServerFeatures,
    message: &IrcMessage,
    server_time: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<ReplayLine> {
    if message.command.to_ascii_uppercase() != "PRIVMSG" || message.params.len() < 2 {
        return None;
    }
    if !routable_source(message) {
        return None;
    }
    let wire_target = message.text_param(0);
    if features.is_channel(&wire_target)
        || is_network_notice_target(&wire_target)
        || !message.prefix.as_ref().is_some_and(IrcPrefix::is_user)
        || is_service_source(message, features)
    {
        return None;
    }
    let sender = author(message);
    if sender.is_empty()
        || (!same(features, &sender, batch_target) && !same(features, &wire_target, batch_target))
    {
        return None;
    }
    let body = message.text_param(1);
    if let Some(ctcp) = parse_ctcp_request(&body) {
        if ctcp.command != "ACTION" {
            return None;
        }
    }
    let msgid = MsgId::new(message.msgid());
    let timestamp = timestamp_for(message, now);
    let (kind, body) = match action_text(&body) {
        Some(text) => (MessageKindTag::Emote, text.to_owned()),
        None => (MessageKindTag::Chat, body),
    };
    Some(ReplayLine {
        author: sender,
        body,
        timestamp,
        kind,
        msgid,
        server_time,
        only_visible_to_you: false,
    })
}
