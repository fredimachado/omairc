//! Port of `tui/internal/irc/translator.go` (Go) and `src/irc/irceventtranslator.cpp` (C++).
//!
//! Turns one inbound `IrcMessage` into the `Event`s the reducer consumes, and a
//! completed history or playback batch into one `HistoryEvent`. This is the one
//! place inbound bytes become display text after the parser: every value goes
//! through `wire_text`. The clock is a parameter. A missing or unparseable `time`
//! tag falls back to `now` and never to the wall clock.
//!
//! The batch types the session hands over (`HistoryBatch`, `HistoryTarget`) and
//! the two time readers (`server_time_of`, `parse_history_target_time`) live here
//! too, so `session/` imports them from the portable core.

use chrono::{DateTime, Utc};

use super::ctcp::parse_ctcp_request;
use super::event::{
    AccountEvent, ActionEvent, AwayEvent, ChannelErrorEvent, ConversationKey, Event, HistoryEvent,
    HistoryKind, JoinEvent, KickEvent, MemberMetadataEvent, MessageEvent, MessageKindTag,
    ModeEvent, MsgId, Name, NamesEvent, NickEvent, NickFactsEvent, NoticeEvent, PartEvent,
    QuitEvent, ReplayLine, SelfAwayEvent, TopicEvent, TypingEvent,
};
use super::leafcontract::{
    canonical_key, clamped, join_line, nick_line, part_line, quit_line, shown_account,
    typing_phase_from_tag, Away,
};
use super::message::IrcMessage;
use super::prefixnick::prefix_nick;
use super::serverfeatures::IrcServerFeatures;
use super::servicenick::{is_service_identity, nick_is_routable};
use super::wiretext::wire_text;

/// Resolves the channel a sender is a member of, for channel-context delivery
/// (`IrcJoinedMemberChannel`). Returns the canonical channel name, or `None` to
/// leave the line on the direct-message path.
pub type JoinedMemberLookup<'a> = &'a dyn Fn(&str, &str) -> Option<String>;

/// The longest channel name a `+draft/channel-context` tag may carry: the
/// classic RFC 2811 limit. A longer tag is not a channel key.
const CHANNEL_CONTEXT_LIMIT: usize = 50;

/// The CTCP ACTION prefix, including the trailing space.
const ACTION_PREFIX: &str = "\u{1}ACTION ";

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

/// Translates one inbound message (`IrcEventTranslator::translate`). `now` is the
/// wall-clock fallback for a missing or unparseable `time` tag. `joined` answers
/// channel-context delivery; `None` leaves every line on its direct-message path.
pub fn translate(
    network_id: &str,
    current_nick: &str,
    features: &IrcServerFeatures,
    message: &IrcMessage,
    now: DateTime<Utc>,
    joined: Option<JoinedMemberLookup<'_>>,
) -> Vec<Event> {
    let mut events = Vec::new();
    let command = message.command.to_ascii_uppercase();
    let sender = author(message);
    let timestamp = timestamp_for(message, now);
    let server_time = server_time_of(message);

    // The account tag names the sender. Record it before command handling so
    // NOTICE, which produces no transcript event, still updates the nick.
    if let Some(account) = present_tag_value(message, "account") {
        if has_user_prefix(message) && !sender.is_empty() {
            events.push(
                AccountEvent {
                    network_id: network_id.to_owned(),
                    nick: sender.clone(),
                    account,
                }
                .into(),
            );
        }
    }

    if command == "NOTICE" {
        if message.params.len() >= 2 {
            let body = param(message, 1);
            if parse_ctcp_request(&body).is_none() {
                if let Some(delivery) = nick_targeted_channel_delivery(
                    &param(message, 0),
                    message,
                    current_nick,
                    features,
                    joined,
                ) {
                    events.push(
                        NoticeEvent {
                            conversation: key(network_id, &delivery.channel, features),
                            author: sender,
                            body,
                            timestamp,
                            server_time,
                            target: delivery.channel,
                            msgid: MsgId::new(tag_value_or_empty(message, "msgid")),
                            only_visible_to_you: delivery.only_visible_to_you,
                        }
                        .into(),
                    );
                }
            }
        }
        return events;
    }

    if command == "PRIVMSG" && message.params.len() >= 2 {
        let wire_target = param(message, 0);
        let body = param(message, 1);
        if let Some(ctcp) = parse_ctcp_request(&body) {
            if ctcp.command != "ACTION" {
                return events;
            }
        }
        let msgid = MsgId::new(tag_value_or_empty(message, "msgid"));
        let action = action_text(&body);
        if let Some(delivery) =
            nick_targeted_channel_delivery(&wire_target, message, current_nick, features, joined)
        {
            let conversation = key(network_id, &delivery.channel, features);
            match action {
                Some(text) => events.push(
                    ActionEvent {
                        conversation,
                        author: sender,
                        body: text.to_owned(),
                        timestamp,
                        server_time,
                        target: delivery.channel,
                        msgid,
                        only_visible_to_you: delivery.only_visible_to_you,
                    }
                    .into(),
                ),
                None => events.push(
                    MessageEvent {
                        conversation,
                        author: sender,
                        body,
                        timestamp,
                        server_time,
                        target: delivery.channel,
                        msgid,
                        only_visible_to_you: delivery.only_visible_to_you,
                    }
                    .into(),
                ),
            }
            return events;
        }
        let Some(conversation) =
            conversation_for(network_id, &wire_target, message, current_nick, features)
        else {
            return events;
        };
        let display_target =
            if !features.is_channel(&wire_target) && !same(features, &sender, current_nick) {
                sender.clone()
            } else {
                wire_target
            };
        match action {
            Some(text) => events.push(
                ActionEvent {
                    conversation,
                    author: sender,
                    body: text.to_owned(),
                    timestamp,
                    server_time,
                    target: display_target,
                    msgid,
                    only_visible_to_you: false,
                }
                .into(),
            ),
            None => events.push(
                MessageEvent {
                    conversation,
                    author: sender,
                    body,
                    timestamp,
                    server_time,
                    target: display_target,
                    msgid,
                    only_visible_to_you: false,
                }
                .into(),
            ),
        }
        return events;
    }

    if command == "TAGMSG" && !message.params.is_empty() {
        let Some(value) = tag_value(message, "+typing") else {
            return events;
        };
        if sender.is_empty() {
            return events;
        }
        let Some(phase) = typing_phase_from_tag(&value) else {
            return events;
        };
        let Some(conversation) = conversation_for(
            network_id,
            &param(message, 0),
            message,
            current_nick,
            features,
        ) else {
            return events;
        };
        events.push(
            TypingEvent {
                conversation,
                nick: sender,
                phase,
                received_at: timestamp,
            }
            .into(),
        );
        return events;
    }

    if command == "JOIN" && !message.params.is_empty() {
        let account = (message.params.len() >= 2).then(|| param(message, 1));
        let realname = (message.params.len() >= 3).then(|| param(message, 2));
        events.push(
            JoinEvent {
                network_id: network_id.to_owned(),
                channel: param(message, 0),
                nick: sender,
                account,
                realname,
            }
            .into(),
        );
        return events;
    }

    if command == "ACCOUNT" && !sender.is_empty() {
        let account = if message.params.is_empty() {
            String::new()
        } else {
            param(message, 0)
        };
        events.push(
            AccountEvent {
                network_id: network_id.to_owned(),
                nick: sender,
                account,
            }
            .into(),
        );
        return events;
    }

    if command == "330" && message.params.len() >= 3 {
        let nick = param(message, 1);
        if !nick.is_empty() {
            events.push(
                AccountEvent {
                    network_id: network_id.to_owned(),
                    nick,
                    account: param(message, 2),
                }
                .into(),
            );
        }
        return events;
    }

    if command == "PART" && !message.params.is_empty() {
        events.push(
            PartEvent {
                network_id: network_id.to_owned(),
                channel: param(message, 0),
                nick: sender,
                reason: param(message, 1),
            }
            .into(),
        );
        return events;
    }

    if command == "QUIT" {
        events.push(
            QuitEvent {
                network_id: network_id.to_owned(),
                nick: sender,
                reason: param(message, 0),
            }
            .into(),
        );
        return events;
    }

    if command == "NICK" && !message.params.is_empty() {
        events.push(
            NickEvent {
                network_id: network_id.to_owned(),
                old_nick: sender,
                new_nick: param(message, 0),
            }
            .into(),
        );
        return events;
    }

    if command == "KICK" && message.params.len() >= 2 {
        events.push(
            KickEvent {
                network_id: network_id.to_owned(),
                channel: param(message, 0),
                target: param(message, 1),
                author: sender,
                reason: param(message, 2),
            }
            .into(),
        );
        return events;
    }

    if command == "TOPIC" && message.params.len() >= 2 {
        events.push(
            TopicEvent {
                network_id: network_id.to_owned(),
                channel: param(message, 0),
                topic: param(message, 1),
                author: sender,
            }
            .into(),
        );
        return events;
    }

    if command == "332" && message.params.len() >= 3 {
        events.push(
            TopicEvent {
                network_id: network_id.to_owned(),
                channel: param(message, 1),
                topic: param(message, 2),
                author: String::new(),
            }
            .into(),
        );
        return events;
    }

    if command == "353" && message.params.len() >= 4 {
        let names = param(message, 3)
            .split(' ')
            .filter(|token| !token.is_empty())
            .filter_map(|token| features.parse_names_token(token))
            .map(|parsed| Name {
                nick: parsed.nick,
                ranks: parsed.ranks,
            })
            .collect();
        events.push(
            NamesEvent {
                network_id: network_id.to_owned(),
                channel: param(message, 2),
                names,
                complete: false,
            }
            .into(),
        );
        return events;
    }

    if command == "366" && message.params.len() >= 2 {
        events.push(
            NamesEvent {
                network_id: network_id.to_owned(),
                channel: param(message, 1),
                names: Vec::new(),
                complete: true,
            }
            .into(),
        );
        return events;
    }

    if command == "MODE" && message.params.len() >= 2 {
        events.push(
            ModeEvent {
                network_id: network_id.to_owned(),
                target: param(message, 0),
                author: sender,
                mode: param(message, 1),
                arguments: remaining_params(message, 2),
            }
            .into(),
        );
        return events;
    }

    if command == "AWAY" && !sender.is_empty() {
        let away = (!message.params.is_empty()).then(|| Away {
            reason: param(message, 0),
        });
        events.push(
            AwayEvent {
                network_id: network_id.to_owned(),
                nick: sender,
                away,
                realname: None,
                realname_only: false,
            }
            .into(),
        );
        return events;
    }

    if command == "306" {
        events.push(
            SelfAwayEvent {
                network_id: network_id.to_owned(),
                away: true,
            }
            .into(),
        );
        return events;
    }

    if command == "305" {
        events.push(
            SelfAwayEvent {
                network_id: network_id.to_owned(),
                away: false,
            }
            .into(),
        );
        return events;
    }

    if command == "352" && message.params.len() >= 7 {
        let nick = param(message, 5);
        let flags = param(message, 6);
        let away = flags.starts_with('G');
        if !nick.is_empty() {
            let trailing = param(message, 7);
            let mut realname = who_reply_realname(&trailing);
            if realname.is_empty() {
                realname = realname_from_who(&trailing);
            }
            events.push(
                AwayEvent {
                    network_id: network_id.to_owned(),
                    nick: nick.clone(),
                    away: away.then(Away::default),
                    realname: (!realname.is_empty()).then_some(realname),
                    realname_only: false,
                }
                .into(),
            );
            // Operator is a separate fact. The realname stays on the away event,
            // so a placeholder cannot wipe a name a join already stored.
            events.push(
                NickFactsEvent {
                    network_id: network_id.to_owned(),
                    nick,
                    realname: None,
                    server_operator: Some(flags.contains('*')),
                }
                .into(),
            );
        }
        return events;
    }

    if command == "311" && message.params.len() >= 6 {
        let nick = param(message, 1);
        let realname = param(message, 5).trim().to_owned();
        if !nick.is_empty() && !realname.is_empty() {
            events.push(
                AwayEvent {
                    network_id: network_id.to_owned(),
                    nick,
                    away: None,
                    realname: Some(realname),
                    realname_only: true,
                }
                .into(),
            );
        }
        return events;
    }

    if command == "313" && message.params.len() >= 2 {
        let nick = param(message, 1);
        if !nick.is_empty() {
            events.push(
                NickFactsEvent {
                    network_id: network_id.to_owned(),
                    nick,
                    realname: None,
                    server_operator: Some(true),
                }
                .into(),
            );
        }
        return events;
    }

    if command == "CHGHOST" {
        // Members store nick plus ranks. User and host are not modeled.
        return events;
    }

    if command == "METADATA" {
        events.extend(member_metadata(network_id, message, 0, features));
        return events;
    }

    if command == "761" {
        events.extend(member_metadata(network_id, message, 1, features));
        return events;
    }

    if command == "766" {
        events.extend(key_not_set(network_id, message, features));
        return events;
    }

    if (is_join_failure_numeric(&command) || is_ambiguous_join_failure_numeric(&command))
        && message.params.len() >= 3
    {
        let channel = param(message, 1);
        if !is_ambiguous_join_failure_numeric(&command) || features.is_channel(&channel) {
            events.push(
                ChannelErrorEvent {
                    network_id: network_id.to_owned(),
                    channel,
                    body: param(message, 2),
                }
                .into(),
            );
        }
    }

    events
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

/// The IRCv3 `time` tag, parsed to UTC. `None` when the tag is absent, empty or
/// unparseable (`ircServerTimeOf`, `ServerTimeOf`).
pub fn server_time_of(message: &IrcMessage) -> Option<DateTime<Utc>> {
    let raw = tag_value(message, "time")?;
    if raw.is_empty() {
        return None;
    }
    DateTime::parse_from_rfc3339(&raw)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

/// Parses the time of a `CHATHISTORY TARGETS` row, with or without `timestamp=`
/// (`parseHistoryTargetTime`).
pub fn parse_history_target_time(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.strip_prefix("timestamp=").unwrap_or(raw);
    if raw.is_empty() {
        return None;
    }
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

/// The conversation an inbound message belongs to, or `None` when it has none
/// (`ConversationFor`). A channel target is its own conversation. A direct
/// message belongs to the peer: the sender when the target is our nick, the target
/// when we sent it. Services, networks notices and pseudo-clients have none.
pub fn conversation_for(
    network_id: &str,
    target: &str,
    message: &IrcMessage,
    current_nick: &str,
    features: &IrcServerFeatures,
) -> Option<ConversationKey> {
    // A bouncer addresses its playback markers to the channel, so the sender is
    // read before the target is classified.
    if !routable_source(message) {
        return None;
    }
    if features.is_channel(target) {
        return Some(key(network_id, target, features));
    }
    if is_network_notice_target(target)
        || !has_user_prefix(message)
        || is_service_user(message, features)
    {
        return None;
    }
    let sender = author(message);
    if sender.is_empty() {
        return None;
    }
    if same(features, target, current_nick) {
        return Some(key(network_id, &sender, features));
    }
    if same(features, &sender, current_nick) {
        return Some(key(network_id, target, features));
    }
    None
}

/// A nick-targeted line that belongs in a joined channel buffer, shown under that
/// buffer's own name.
struct ChannelDelivery {
    channel: String,
    only_visible_to_you: bool,
}

/// Routes a nick-targeted line that carries `+draft/channel-context` into the
/// channel buffer when the sender is a member of it. The context must be one
/// channel name, and the sender must be a member the caller can confirm.
fn nick_targeted_channel_delivery(
    wire_target: &str,
    message: &IrcMessage,
    current_nick: &str,
    features: &IrcServerFeatures,
    joined: Option<JoinedMemberLookup<'_>>,
) -> Option<ChannelDelivery> {
    let context = channel_context_channel(message, features)?;
    if features.is_channel(wire_target) || !same(features, wire_target, current_nick) {
        return None;
    }
    let sender = author(message);
    if sender.is_empty() || same(features, &context, current_nick) {
        return None;
    }
    let lookup = joined?;
    let canonical = lookup(&context, &sender)?;
    if canonical.is_empty() || !features.is_channel(&canonical) {
        return None;
    }
    Some(ChannelDelivery {
        channel: canonical,
        only_visible_to_you: true,
    })
}

/// The channel named by `+draft/channel-context`, or the unprefixed draft tag,
/// when the value is one channel name (`channelContextChannel`).
fn channel_context_channel(message: &IrcMessage, features: &IrcServerFeatures) -> Option<String> {
    for name in ["+draft/channel-context", "draft/channel-context"] {
        if let Some(value) = tag_value(message, name) {
            if channel_context_text(&value, features) {
                return Some(value);
            }
        }
    }
    None
}

/// Whether `value` is one channel name: a CHANTYPES prefix, two to fifty runes,
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
    let msgid = MsgId::new(tag_value_or_empty(message, "msgid"));
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
    let wire_target = param(message, 0);
    if features.is_channel(&wire_target)
        || is_network_notice_target(&wire_target)
        || !has_user_prefix(message)
        || is_service_user(message, features)
    {
        return None;
    }
    let sender = author(message);
    if sender.is_empty()
        || (!same(features, &sender, batch_target) && !same(features, &wire_target, batch_target))
    {
        return None;
    }
    let body = param(message, 1);
    if let Some(ctcp) = parse_ctcp_request(&body) {
        if ctcp.command != "ACTION" {
            return None;
        }
    }
    let msgid = MsgId::new(tag_value_or_empty(message, "msgid"));
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
    let target = param(message, target_index);
    let key_text = param(message, target_index + 1);
    if target.is_empty() || features.is_channel(&target) {
        return None;
    }
    let canonical = canonical_key(&key_text)?;
    let value = clamped(&param(message, target_index + 3));
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
fn key_not_set(
    network_id: &str,
    message: &IrcMessage,
    features: &IrcServerFeatures,
) -> Option<Event> {
    const TARGET_INDEX: usize = 1;
    if message.params.len() < TARGET_INDEX + 2 {
        return None;
    }
    let target = param(message, TARGET_INDEX);
    let key_text = param(message, TARGET_INDEX + 1);
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

/// One message parameter as display text, or "" past the end (`ircParameter`).
/// The Go decoded parameters once; the Rust core keeps raw bytes until here.
fn param(message: &IrcMessage, index: usize) -> String {
    message
        .params
        .get(index)
        .map(|raw| wire_text(raw))
        .unwrap_or_default()
}

/// Every parameter from `start` on, as display text (`ircRemainingParameters`).
fn remaining_params(message: &IrcMessage, start: usize) -> Vec<String> {
    (start..message.params.len())
        .map(|index| param(message, index))
        .collect()
}

/// The source nick, through the same rule every handler uses (`ircAuthor`).
fn author(message: &IrcMessage) -> String {
    prefix_nick(message)
}

/// Whether the source carries both a nick and a user. A bare server name does not.
fn has_user_prefix(message: &IrcMessage) -> bool {
    message
        .prefix
        .as_ref()
        .is_some_and(|prefix| !prefix.nick.is_empty() && !prefix.user.is_empty())
}

/// False when the source nick is a bouncer pseudo-client that cannot take a reply.
fn routable_source(message: &IrcMessage) -> bool {
    match &message.prefix {
        Some(prefix) if !prefix.nick.is_empty() => nick_is_routable(&wire_text(&prefix.nick)),
        _ => true,
    }
}

/// Whether the source is a network service (`ircIsServiceUser`).
fn is_service_user(message: &IrcMessage, features: &IrcServerFeatures) -> bool {
    match &message.prefix {
        None => false,
        Some(prefix) => is_service_identity(
            &wire_text(&prefix.nick),
            &wire_text(&prefix.host),
            features.channel_types(),
        ),
    }
}

/// The targets that name the network rather than a person (`ircIsNetworkNoticeTarget`).
fn is_network_notice_target(target: &str) -> bool {
    target.is_empty() || target == "*" || target.eq_ignore_ascii_case("AUTH")
}

/// The conversation key for `target` on `network_id` (`ircKey`).
fn key(network_id: &str, target: &str, features: &IrcServerFeatures) -> ConversationKey {
    ConversationKey::new(network_id, features.case_mapping().normalize(target))
}

/// Whether two identifiers name the same thing under the network's case mapping.
fn same(features: &IrcServerFeatures, left: &str, right: &str) -> bool {
    features.case_mapping().equals(left, right)
}

/// The value of the first tag named `name` that carries one (`ircTagValue`). A
/// valueless tag is absent here.
fn tag_value(message: &IrcMessage, name: &str) -> Option<String> {
    message
        .tags
        .iter()
        .filter(|tag| tag.name == name)
        .find_map(|tag| tag.value.as_ref())
        .map(|raw| wire_text(raw))
}

/// The tag value, or "" when it is absent or valueless (`tagValue(...).value_or`).
fn tag_value_or_empty(message: &IrcMessage, name: &str) -> String {
    tag_value(message, name).unwrap_or_default()
}

/// Distinguishes "the server omitted the tag" (`None`) from "the server sent it"
/// (`Some`, empty when valueless). A logout (`account=*`) and a valueless
/// `account` therefore both count as present (`ircPresentTagValue`).
fn present_tag_value(message: &IrcMessage, name: &str) -> Option<String> {
    let tag = message.tags.iter().find(|tag| tag.name == name)?;
    Some(
        tag.value
            .as_ref()
            .map(|raw| wire_text(raw))
            .unwrap_or_default(),
    )
}

/// The message time: the parsed `time` tag, else the wall clock `now` (`timestampFor`).
fn timestamp_for(message: &IrcMessage, now: DateTime<Utc>) -> DateTime<Utc> {
    server_time_of(message).unwrap_or(now)
}

/// The body of a CTCP ACTION, or `None` when the body is not one (`ACTION_PREFIX` test).
fn action_text(body: &str) -> Option<&str> {
    if body.starts_with(ACTION_PREFIX) && body.ends_with('\u{1}') {
        return Some(&body[ACTION_PREFIX.len()..body.len() - 1]);
    }
    None
}

/// Whether `value` parses as a Go `strconv.Atoi` integer.
fn is_go_int(value: &str) -> bool {
    value.parse::<i64>().is_ok()
}

/// Whether `value` is one or more ASCII digits.
fn all_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::event::{ConversationKey as Key, EventKind, MsgId as Id};
    use crate::irc::leafcontract::TypingPhase;
    use crate::irc::parser::parse;
    use chrono::TimeZone;

    const NETWORK_A: &str = "network-a";

    /// The fixed reducer clock every Go translator test uses.
    fn reducer_timestamp() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 4, 0, 0, 0).unwrap()
    }

    fn features() -> IrcServerFeatures {
        IrcServerFeatures::new()
    }

    fn must_parse(line: &str) -> IrcMessage {
        parse(line.as_bytes()).unwrap_or_else(|err| panic!("parse({line:?}) failed: {err}"))
    }

    fn translate_line(line: &str) -> Vec<Event> {
        translate(
            NETWORK_A,
            "omairc",
            &features(),
            &must_parse(line),
            reducer_timestamp(),
            None,
        )
    }

    fn translate_joined(line: &str, joined: JoinedMemberLookup<'_>) -> Vec<Event> {
        translate(
            NETWORK_A,
            "omairc",
            &features(),
            &must_parse(line),
            reducer_timestamp(),
            Some(joined),
        )
    }

    fn room_member_lookup(channel: &str, sender: &str) -> Option<String> {
        if sender.eq_ignore_ascii_case("bot") && channel.eq_ignore_ascii_case("#room") {
            Some("#room".to_owned())
        } else {
            None
        }
    }

    fn accept_any(channel: &str, _sender: &str) -> Option<String> {
        Some(channel.to_owned())
    }

    fn parsed_name(token: &str) -> Name {
        let parsed = features()
            .parse_names_token(token)
            .unwrap_or_else(|| panic!("parse_names_token({token:?}) failed"));
        Name {
            nick: parsed.nick,
            ranks: parsed.ranks,
        }
    }

    fn message_of(events: &[Event]) -> &MessageEvent {
        match events.first() {
            Some(Event::Message(value)) => value,
            other => panic!("want MessageEvent, got {other:?}"),
        }
    }

    fn history_of(batch: &HistoryBatch, joined: Option<JoinedMemberLookup<'_>>) -> HistoryEvent {
        translate_history(
            NETWORK_A,
            "omairc",
            &features(),
            batch,
            reducer_timestamp(),
            joined,
        )
        .expect("the batch translates")
    }

    fn batch(target: &str, kind: Option<HistoryKind>, lines: &[&str]) -> HistoryBatch {
        HistoryBatch {
            target: target.to_owned(),
            lines: lines.iter().map(|line| must_parse(line)).collect(),
            kind,
            ..HistoryBatch::default()
        }
    }

    #[test]
    fn numeric_and_command_table_yields_the_expected_first_kind() {
        let cases: [(&str, &str, EventKind); 21] = [
            (
                "self away",
                ":server 306 omairc :marked away",
                EventKind::SelfAway,
            ),
            ("self back", ":server 305 omairc :back", EventKind::SelfAway),
            (
                "whois away",
                ":server 352 omairc #room user host server Alice G :0 Alice",
                EventKind::Away,
            ),
            (
                "whois account",
                ":server 330 omairc Alice whoisacct :is logged in as",
                EventKind::Account,
            ),
            (
                "channel topic reply",
                ":server 332 omairc #room :topic",
                EventKind::Topic,
            ),
            (
                "names reply",
                ":server 353 omairc = #room :@Alice +Bob",
                EventKind::Names,
            ),
            (
                "end of names",
                ":server 366 omairc #room :End of /NAMES list.",
                EventKind::Names,
            ),
            ("mode", ":op!u@h MODE #room +o alice", EventKind::Mode),
            ("topic", ":op!u@h TOPIC #room :topic", EventKind::Topic),
            ("part", ":alice!u@h PART #room :bye", EventKind::Part),
            ("quit", ":alice!u@h QUIT :gone", EventKind::Quit),
            ("nick", ":alice!u@h NICK Alicia", EventKind::Nick),
            ("kick", ":op!u@h KICK #room bob :bye", EventKind::Kick),
            ("account", ":alice!u@h ACCOUNT services", EventKind::Account),
            ("away", ":alice!u@h AWAY :lunch", EventKind::Away),
            (
                "metadata 761",
                ":server 761 omairc Alice status priv :writing docs",
                EventKind::MemberMetadata,
            ),
            (
                "metadata 766",
                ":server 766 omairc Alice status :Key not set",
                EventKind::MemberMetadata,
            ),
            (
                "join failure 471",
                ":server 471 omairc #room :Cannot join channel (+l)",
                EventKind::ChannelError,
            ),
            (
                "join failure 473",
                ":server 473 omairc #room :Invite only",
                EventKind::ChannelError,
            ),
            (
                "ambiguous join failure 437",
                ":server 437 omairc #room :unavailable",
                EventKind::ChannelError,
            ),
            (
                "whois user",
                ":server 311 omairc Alice user host * :Alice Example",
                EventKind::Away,
            ),
        ];
        for (name, line, want) in cases {
            let events = translate_line(line);
            assert!(
                !events.is_empty(),
                "{name}: Translate({line:?}) produced no events"
            );
            assert_eq!(events[0].kind(), want, "{name}");
        }
    }

    #[test]
    fn lines_the_translator_does_not_model_produce_nothing() {
        // CHGHOST is a no-op, and an unknown numeric has no handler.
        assert!(translate_line(":alice!u@h CHGHOST new host").is_empty());
        assert!(translate_line(":server 999 omairc :nope").is_empty());
    }

    #[test]
    fn notice_updates_account_but_produces_no_transcript_event() {
        assert!(translate_line(":Alice!u@h NOTICE #room :hi").is_empty());

        let events = translate_line("@account=services :Alice!u@h NOTICE #room :hi");
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Account(account) => {
                assert_eq!(account.nick, "Alice");
                assert_eq!(account.account, "services");
            }
            other => panic!("want AccountEvent, got {other:?}"),
        }
    }

    #[test]
    fn privmsg_routes_channel_and_direct_display_targets() {
        let events = translate_line(":alice!u@h PRIVMSG #room :hello");
        assert_eq!(events.len(), 1);
        let channel = message_of(&events);
        assert_eq!(channel.conversation.normalized_target, "#room");
        assert_eq!(channel.author, "alice");
        assert_eq!(channel.body, "hello");
        assert_eq!(channel.target, "#room");

        let events = translate_line(":alice!u@h PRIVMSG omairc :hi");
        assert_eq!(events.len(), 1);
        let inbound = message_of(&events);
        assert_eq!(inbound.conversation.normalized_target, "alice");
        assert_eq!(inbound.target, "alice");

        let events = translate_line(":omairc!u@h PRIVMSG alice :hi");
        assert_eq!(events.len(), 1);
        let outbound = message_of(&events);
        assert_eq!(outbound.conversation.normalized_target, "alice");
        assert_eq!(outbound.target, "alice");
    }

    #[test]
    fn ctcp_action_is_an_emote_and_other_ctcp_is_dropped() {
        let events = translate_line(":alice!u@h PRIVMSG #room :\u{1}ACTION waves\u{1}");
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Action(action) => {
                assert_eq!(action.author, "alice");
                assert_eq!(action.body, "waves");
            }
            other => panic!("want ActionEvent, got {other:?}"),
        }
        assert!(translate_line(":alice!u@h PRIVMSG #room :\u{1}VERSION\u{1}").is_empty());
    }

    #[test]
    fn extended_join_carries_account_and_realname_and_classic_join_does_not() {
        let events = translate_line(":alice!u@h JOIN #room account :Alice Example");
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Join(join) => {
                assert_eq!(join.channel, "#room");
                assert_eq!(join.nick, "alice");
                assert_eq!(join.account.as_deref(), Some("account"));
                assert_eq!(join.realname.as_deref(), Some("Alice Example"));
            }
            other => panic!("want JoinEvent, got {other:?}"),
        }

        match translate_line(":alice!u@h JOIN #room").first() {
            Some(Event::Join(join)) => {
                assert!(
                    join.account.is_none(),
                    "a classic join must not clear an account"
                );
                assert!(join.realname.is_none());
            }
            other => panic!("want JoinEvent, got {other:?}"),
        }
    }

    #[test]
    fn names_tokens_split_ranks_and_end_of_names_completes() {
        let events = translate_line(":server 353 omairc = #room :@Alice +Bob");
        match &events[0] {
            Event::Names(names) => {
                assert_eq!(names.channel, "#room");
                assert!(!names.complete);
                assert_eq!(names.names.len(), 2);
                assert_eq!(names.names[0].nick, "Alice");
                assert_eq!(names.names[0].ranks, parsed_name("@Alice").ranks);
                assert_eq!(names.names[1].nick, "Bob");
                assert_eq!(names.names[1].ranks, parsed_name("+Bob").ranks);
            }
            other => panic!("want NamesEvent, got {other:?}"),
        }

        match translate_line(":server 366 omairc #room :End of /NAMES list.").first() {
            Some(Event::Names(end)) => {
                assert_eq!(end.channel, "#room");
                assert!(end.complete);
                assert!(end.names.is_empty());
            }
            other => panic!("want NamesEvent, got {other:?}"),
        }
    }

    #[test]
    fn away_numerics_set_clear_and_fill_the_facts_each_one_owns() {
        match translate_line(":server 306 omairc :You have been marked as being away").first() {
            Some(Event::SelfAway(self_away)) => assert!(self_away.away),
            other => panic!("want SelfAwayEvent, got {other:?}"),
        }
        match translate_line(":server 305 omairc :You are no longer away").first() {
            Some(Event::SelfAway(self_away)) => assert!(!self_away.away),
            other => panic!("want SelfAwayEvent, got {other:?}"),
        }

        // 352 with G: away, realname "Alice", and no operator flag on the away event.
        let events = translate_line(":server 352 omairc #room user host server Alice G :0 Alice");
        match &events[0] {
            Event::Away(away) => {
                assert_eq!(away.nick, "Alice");
                assert!(away.away.is_some());
                assert_eq!(away.realname.as_deref(), Some("Alice"));
                assert!(!away.realname_only);
            }
            other => panic!("want AwayEvent, got {other:?}"),
        }

        // 352 with H: here, so no away value.
        match translate_line(":server 352 omairc #room user host server Alice H :0 Alice").first() {
            Some(Event::Away(away)) => assert!(away.away.is_none()),
            other => panic!("want AwayEvent, got {other:?}"),
        }

        // 352 with H*: operator, and the realname stays on the away event.
        let events =
            translate_line(":server 352 omairc #room user host server Alice H* :0 Alice Vale");
        assert_eq!(events.len(), 2);
        match (&events[0], &events[1]) {
            (Event::Away(away), Event::NickFacts(facts)) => {
                assert_eq!(away.realname.as_deref(), Some("Alice Vale"));
                assert!(away.away.is_none());
                assert!(
                    facts.realname.is_none(),
                    "the realname stays on the away event"
                );
                assert_eq!(facts.server_operator, Some(true));
            }
            other => panic!("want AwayEvent then NickFactsEvent, got {other:?}"),
        }

        // 311 is a WHOIS real name only: it must not change away.
        match translate_line(":server 311 omairc Alice user host * :Alice Example").first() {
            Some(Event::Away(away)) => {
                assert!(away.realname_only);
                assert!(away.away.is_none());
                assert_eq!(away.realname.as_deref(), Some("Alice Example"));
            }
            other => panic!("want AwayEvent, got {other:?}"),
        }

        // 313 sets operator true and leaves the realname alone.
        let events = translate_line(":server 313 omairc Alice :is an IRC operator");
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::NickFacts(facts) => {
                assert!(facts.realname.is_none());
                assert_eq!(facts.server_operator, Some(true));
            }
            other => panic!("want NickFactsEvent, got {other:?}"),
        }

        match translate_line(":alice!u@h AWAY :lunch").first() {
            Some(Event::Away(away)) => {
                assert_eq!(away.away.as_ref().map(|a| a.reason.as_str()), Some("lunch"))
            }
            other => panic!("want AwayEvent, got {other:?}"),
        }
        match translate_line(":alice!u@h AWAY").first() {
            Some(Event::Away(away)) => assert!(away.away.is_none(), "a bare AWAY clears away"),
            other => panic!("want AwayEvent, got {other:?}"),
        }
    }

    #[test]
    fn who_reply_real_names_follow_the_hop_count_rules() {
        // A signed hop count is still a hop count, as Go's Atoi reads it.
        match translate_line(":server 352 omairc #room user host server Alice H :+1 Alice Vale")
            .first()
        {
            Some(Event::Away(away)) => assert_eq!(away.realname.as_deref(), Some("Alice Vale")),
            other => panic!("want AwayEvent, got {other:?}"),
        }
        // A lone hop count is no real name at all.
        match translate_line(":server 352 omairc #room user host server Alice H :0").first() {
            Some(Event::Away(away)) => assert!(away.realname.is_none()),
            other => panic!("want AwayEvent, got {other:?}"),
        }
        // A trailing value with no hop count is the name itself.
        match translate_line(":server 352 omairc #room user host server Alice H :Alice Vale")
            .first()
        {
            Some(Event::Away(away)) => assert_eq!(away.realname.as_deref(), Some("Alice Vale")),
            other => panic!("want AwayEvent, got {other:?}"),
        }
    }

    #[test]
    fn metadata_numerics_read_the_known_keys_and_drop_the_rest() {
        match translate_line(":server 761 omairc Alice status priv :writing docs").first() {
            Some(Event::MemberMetadata(metadata)) => {
                assert_eq!(metadata.nick, "Alice");
                assert_eq!(metadata.key, "status");
                assert_eq!(metadata.value, "writing docs");
            }
            other => panic!("want MemberMetadataEvent, got {other:?}"),
        }
        match translate_line(":server 766 omairc Alice status :Key not set").first() {
            Some(Event::MemberMetadata(metadata)) => {
                assert_eq!(metadata.nick, "Alice");
                assert_eq!(metadata.key, "status");
                assert_eq!(metadata.value, "", "766 clears the key");
            }
            other => panic!("want MemberMetadataEvent, got {other:?}"),
        }
        assert!(translate_line(":server 761 omairc Alice unknown priv :value").is_empty());
        assert!(translate_line(":server 761 omairc #room status priv :value").is_empty());
        assert!(translate_line(":server 766 omairc Alice unknown :gone").is_empty());
    }

    #[test]
    fn join_failure_numerics_need_a_channel_when_they_are_ambiguous() {
        match translate_line(":server 471 omairc #room :Cannot join channel (+l)").first() {
            Some(Event::ChannelError(error)) => {
                assert_eq!(error.channel, "#room");
                assert_eq!(error.body, "Cannot join channel (+l)");
            }
            other => panic!("want ChannelErrorEvent, got {other:?}"),
        }
        match translate_line(":server 437 omairc #room :unavailable").first() {
            Some(Event::ChannelError(error)) => assert_eq!(error.channel, "#room"),
            other => panic!("want ChannelErrorEvent, got {other:?}"),
        }
        assert!(translate_line(":server 437 omairc someNick :unavailable").is_empty());
    }

    #[test]
    fn account_tag_is_present_for_a_logout_and_a_valueless_tag_but_not_absent() {
        let events = translate_line(":Alice!u@h PRIVMSG #room :hi");
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], Event::Message(_)));

        let events = translate_line("@account=services :Alice!u@h PRIVMSG #room :hi");
        assert_eq!(events.len(), 2);
        match &events[0] {
            Event::Account(account) => assert_eq!(account.account, "services"),
            other => panic!("want AccountEvent, got {other:?}"),
        }
        assert!(matches!(events[1], Event::Message(_)));

        match translate_line("@account=* :Alice!u@h PRIVMSG #room :hi").first() {
            Some(Event::Account(account)) => assert_eq!(account.account, "*", "a logout"),
            other => panic!("want AccountEvent, got {other:?}"),
        }

        let events = translate_line("@account :Alice!u@h PRIVMSG #room :hi");
        assert_eq!(events.len(), 2);
        match &events[0] {
            Event::Account(account) => assert_eq!(account.account, "", "a valueless account"),
            other => panic!("want AccountEvent, got {other:?}"),
        }
    }

    #[test]
    fn conversation_for_classifies_every_kind_of_target() {
        let features = features();
        let from_alice = must_parse(":alice!u@h PRIVMSG #room :hi");

        let channel = conversation_for(NETWORK_A, "#room", &from_alice, "omairc", &features);
        assert_eq!(channel, Some(Key::new(NETWORK_A, "#room")));

        let inbound = conversation_for(NETWORK_A, "omairc", &from_alice, "omairc", &features);
        assert_eq!(inbound, Some(Key::new(NETWORK_A, "alice")));

        let from_self = must_parse(":omairc!u@h PRIVMSG alice :hi");
        assert_eq!(
            conversation_for(NETWORK_A, "alice", &from_self, "omairc", &features),
            Some(Key::new(NETWORK_A, "alice"))
        );

        let service = must_parse(":NickServ!s@services PRIVMSG omairc :hi");
        assert!(
            conversation_for(NETWORK_A, "omairc", &service, "omairc", &features).is_none(),
            "a service identity must not resolve to a conversation"
        );

        let no_user = must_parse(":alice PRIVMSG omairc :hi");
        assert!(
            conversation_for(NETWORK_A, "omairc", &no_user, "omairc", &features).is_none(),
            "a user-less prefix must not resolve to a conversation"
        );

        let notice_target = must_parse(":alice!u@h PRIVMSG * :hi");
        assert!(
            conversation_for(NETWORK_A, "*", &notice_target, "omairc", &features).is_none(),
            "a network notice target must not resolve to a conversation"
        );

        let bouncer = must_parse(":*status!u@h PRIVMSG omairc :hi");
        assert!(
            conversation_for(NETWORK_A, "omairc", &bouncer, "omairc", &features).is_none(),
            "a non-routable bouncer nick must not resolve to a conversation"
        );
    }

    #[test]
    fn typing_tagmsg_reads_the_phase_and_ignores_the_rest() {
        let events = translate_line("@+typing=active :alice!u@h TAGMSG #omarchy");
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Typing(typing) => {
                assert_eq!(typing.nick, "alice");
                assert_eq!(typing.phase, TypingPhase::Active);
                assert_eq!(typing.conversation.normalized_target, "#omarchy");
            }
            other => panic!("want TypingEvent, got {other:?}"),
        }

        match translate_line("@+typing=PAUSED :alice!u@h TAGMSG omairc").first() {
            Some(Event::Typing(typing)) => {
                assert_eq!(typing.phase, TypingPhase::Paused);
                assert_eq!(typing.conversation.normalized_target, "alice");
            }
            other => panic!("want TypingEvent, got {other:?}"),
        }

        for line in [
            "@+typing=draft :alice!u@h TAGMSG #omarchy",
            ":alice!u@h TAGMSG #omarchy",
            "@+typing=active TAGMSG #omarchy",
        ] {
            assert!(
                translate_line(line).is_empty(),
                "{line} must produce nothing"
            );
        }

        let events = translate_line("@+typing=active :alice!u@h PRIVMSG #omarchy :here");
        assert_eq!(events.len(), 1);
        assert!(
            matches!(events[0], Event::Message(_)),
            "a typing tag on chat is still chat"
        );
    }

    #[test]
    fn history_batch_keeps_the_server_time_and_falls_back_to_now() {
        let now = reducer_timestamp();
        let event = history_of(
            &batch("lena", None, &[":lena!u@h PRIVMSG omairc :hi"]),
            None,
        );
        assert_eq!(event.lines.len(), 1);
        let line = &event.lines[0];
        assert_eq!(line.author, "lena");
        assert_eq!(line.body, "hi");
        assert!(line.server_time.is_none());
        assert_eq!(
            line.timestamp, now,
            "no time tag falls back to the wall clock"
        );

        let event = history_of(
            &batch(
                "lena",
                None,
                &["@time=2024-03-09T16:00:00.620Z :lena!u@h PRIVMSG omairc :hi"],
            ),
            None,
        );
        let line = &event.lines[0];
        let want = Utc.with_ymd_and_hms(2024, 3, 9, 16, 0, 0).unwrap()
            + chrono::Duration::milliseconds(620);
        assert_eq!(line.server_time, Some(want));
        assert_eq!(line.timestamp, want, "the timestamp uses the tag");

        assert!(
            translate_history(
                NETWORK_A,
                "omairc",
                &features(),
                &HistoryBatch::default(),
                now,
                None
            )
            .is_none(),
            "an empty target must not translate"
        );
    }

    #[test]
    fn targets_batches_do_not_translate_into_lines() {
        let targets = HistoryBatch {
            target: "lena".to_owned(),
            kind: Some(HistoryKind::Targets),
            ..HistoryBatch::default()
        };
        assert!(translate_history(
            NETWORK_A,
            "omairc",
            &features(),
            &targets,
            reducer_timestamp(),
            None
        )
        .is_none());
    }

    #[test]
    fn bouncer_query_keeps_lines_under_a_previous_nick_but_nothing_else() {
        let event = history_of(
            &batch(
                "lena",
                Some(HistoryKind::BouncerPlayback),
                &[
                    ":oldnick!u@h PRIVMSG lena :mine",
                    ":lena!u@h PRIVMSG oldnick :before",
                    ":stranger!u@h PRIVMSG oldnick :elsewhere",
                    ":lena!u@h PRIVMSG #other :channel",
                    ":NickServ!s@services PRIVMSG lena :hi",
                ],
            ),
            None,
        );
        assert_eq!(event.lines.len(), 2);
        assert_eq!(event.lines[0].author, "oldnick");
        assert_eq!(event.lines[0].body, "mine");
        assert_eq!(event.lines[1].author, "lena");
        assert_eq!(event.lines[1].body, "before");
    }

    #[test]
    fn chat_history_keeps_only_the_current_nick() {
        let event = history_of(
            &batch("lena", None, &[":oldnick!u@h PRIVMSG lena :mine"]),
            None,
        );
        assert!(event.lines.is_empty());
    }

    #[test]
    fn channel_history_keeps_joins_as_event_lines_and_chat_as_chat() {
        let event = history_of(
            &batch(
                "#omarchy",
                None,
                &[
                    ":alice!u@h JOIN :#omarchy",
                    ":alice!u@h PRIVMSG #omarchy :from history",
                ],
            ),
            None,
        );
        assert_eq!(event.lines.len(), 2);
        assert_eq!(event.lines[0].author, "");
        assert_eq!(event.lines[0].kind, MessageKindTag::Event);
        assert_eq!(event.lines[0].body, "alice joined");
        assert_eq!(event.lines[1].body, "from history");
    }

    #[test]
    fn channel_context_private_line_is_only_visible_to_you() {
        let marked = translate_joined(
            "@+draft/channel-context=#room :bot!u@h PRIVMSG omairc :just you",
            &room_member_lookup,
        );
        assert_eq!(marked.len(), 1);
        let message = message_of(&marked);
        assert_eq!(message.conversation.normalized_target, "#room");
        assert_eq!(message.target, "#room");
        assert_eq!(message.body, "just you");
        assert!(message.only_visible_to_you);

        let unprefixed = translate_joined(
            "@draft/channel-context=#room :bot!u@h PRIVMSG omairc :also you",
            &room_member_lookup,
        );
        assert!(message_of(&unprefixed).only_visible_to_you);

        // The lookup's canonical spelling is the buffer name.
        let spelled = translate_joined(
            "@+draft/channel-context=#Room :bot!u@h PRIVMSG omairc :canonical",
            &room_member_lookup,
        );
        let canonical = message_of(&spelled);
        assert_eq!(canonical.target, "#room");
        assert!(canonical.only_visible_to_you);

        let ordinary = translate_joined(":alice!u@h PRIVMSG #room :everyone", &room_member_lookup);
        let ordinary = message_of(&ordinary);
        assert_eq!(ordinary.conversation.normalized_target, "#room");
        assert!(!ordinary.only_visible_to_you);

        let tagged_channel = translate_joined(
            "@+draft/channel-context=#room :alice!u@h PRIVMSG #room :still public",
            &room_member_lookup,
        );
        assert!(!message_of(&tagged_channel).only_visible_to_you);

        // A channel-operator target never reaches the transcript through the translator.
        assert!(translate_joined(":op!u@h PRIVMSG @#room :ops", &room_member_lookup).is_empty());

        let notice = translate_joined(
            "@+draft/channel-context=#room :bot!u@h NOTICE omairc :heads up",
            &room_member_lookup,
        );
        assert_eq!(notice.len(), 1);
        match &notice[0] {
            Event::Notice(value) => {
                assert!(value.only_visible_to_you);
                assert_eq!(value.target, "#room");
            }
            other => panic!("want NoticeEvent, got {other:?}"),
        }
        assert!(translate_line(":alice!u@h NOTICE omairc :dm notice").is_empty());

        let replay = batch(
            "#room",
            None,
            &[
                "@+draft/channel-context=#room :bot!u@h PRIVMSG omairc :replayed",
                ":alice!u@h PRIVMSG #room :public replay",
                "@+draft/channel-context=#room :bot!u@h NOTICE omairc :notice replay",
                ":op!u@h PRIVMSG @#room :ops replay",
            ],
        );
        let lookup: JoinedMemberLookup<'_> = &room_member_lookup;
        let history = history_of(&replay, Some(lookup));
        assert_eq!(history.lines.len(), 2);
        assert!(history.lines[0].only_visible_to_you);
        assert_eq!(history.lines[0].kind, MessageKindTag::Chat);
        assert_eq!(history.lines[0].body, "replayed");
        assert!(!history.lines[1].only_visible_to_you);
        assert_eq!(history.lines[1].body, "public replay");
    }

    #[test]
    fn channel_context_stays_a_direct_message_when_it_is_not_trusted() {
        let expect_direct = |events: &[Event], sender: &str, body: &str| {
            assert_eq!(events.len(), 1, "{body}: event count");
            let message = message_of(events);
            assert_eq!(message.author, sender, "{body}: author");
            assert_eq!(message.body, body, "{body}: body");
            assert_eq!(
                message.conversation.normalized_target,
                sender.to_lowercase(),
                "{body}: conversation"
            );
            assert_eq!(message.target, sender, "{body}: target");
            assert!(!message.only_visible_to_you, "{body}: only you");
        };

        let accept: JoinedMemberLookup<'_> = &accept_any;

        expect_direct(
            &translate_line("@+draft/channel-context=#room :eve!u@h PRIVMSG omairc :unjoined"),
            "eve",
            "unjoined",
        );
        expect_direct(
            &translate_joined(
                "@+draft/channel-context=#room :eve!u@h PRIVMSG omairc :not a member",
                &room_member_lookup,
            ),
            "eve",
            "not a member",
        );
        let overlong = format!(
            "@+draft/channel-context=#{} :eve!u@h PRIVMSG omairc :overlong",
            "x".repeat(60)
        );
        expect_direct(&translate_joined(&overlong, accept), "eve", "overlong");
        expect_direct(
            &translate_joined(
                "@+draft/channel-context=#a,#b :eve!u@h PRIVMSG omairc :comma",
                accept,
            ),
            "eve",
            "comma",
        );
        expect_direct(
            &translate_joined(
                "@+draft/channel-context=#my\\sroom :eve!u@h PRIVMSG omairc :space",
                accept,
            ),
            "eve",
            "space",
        );
        expect_direct(
            &translate_joined(
                "@+draft/channel-context=#room\u{1}x :eve!u@h PRIVMSG omairc :control",
                accept,
            ),
            "eve",
            "control",
        );
        expect_direct(
            &translate_joined(
                "@+draft/channel-context=omairc :eve!u@h PRIVMSG omairc :own nick",
                accept,
            ),
            "eve",
            "own nick",
        );

        let action = translate_line(
            "@+draft/channel-context=#room :eve!u@h PRIVMSG omairc :\u{1}ACTION waves\u{1}",
        );
        assert_eq!(action.len(), 1);
        match &action[0] {
            Event::Action(value) => {
                assert_eq!(value.conversation.normalized_target, "eve");
                assert_eq!(value.body, "waves");
                assert!(!value.only_visible_to_you);
            }
            other => panic!("want ActionEvent, got {other:?}"),
        }
        assert!(
            translate_line("@+draft/channel-context=#room :eve!u@h NOTICE omairc :heads up")
                .is_empty()
        );

        let query = history_of(
            &batch(
                "alice",
                None,
                &["@+draft/channel-context=#room :alice!u@h PRIVMSG omairc :kept in query"],
            ),
            Some(accept),
        );
        assert_eq!(query.conversation.normalized_target, "alice");
        assert_eq!(query.lines.len(), 1);
        assert_eq!(query.lines[0].body, "kept in query");
        assert_eq!(query.lines[0].kind, MessageKindTag::Chat);
        assert!(!query.lines[0].only_visible_to_you);

        let unjoined = history_of(
            &batch(
                "#room",
                None,
                &[
                    "@+draft/channel-context=#room :bot!u@h PRIVMSG omairc :dropped",
                    ":alice!u@h PRIVMSG #room :public only",
                ],
            ),
            None,
        );
        assert_eq!(unjoined.lines.len(), 1);
        assert_eq!(unjoined.lines[0].body, "public only");
        assert!(!unjoined.lines[0].only_visible_to_you);
    }

    #[test]
    fn the_time_tag_reader_takes_only_a_parseable_value() {
        assert_eq!(server_time_of(&must_parse(":a!u@h PRIVMSG #c :x")), None);
        assert_eq!(
            server_time_of(&must_parse("@time= :a!u@h PRIVMSG #c :x")),
            None
        );
        assert_eq!(
            server_time_of(&must_parse("@time=not-a-time :a!u@h PRIVMSG #c :x")),
            None
        );
        assert_eq!(
            server_time_of(&must_parse(
                "@time=2024-03-09T16:00:00Z :a!u@h PRIVMSG #c :x"
            )),
            Some(Utc.with_ymd_and_hms(2024, 3, 9, 16, 0, 0).unwrap())
        );
    }

    #[test]
    fn chathistory_targets_times_accept_the_prefix_or_the_bare_value() {
        let want = Some(Utc.with_ymd_and_hms(2024, 3, 9, 16, 0, 0).unwrap());
        assert_eq!(
            parse_history_target_time("timestamp=2024-03-09T16:00:00Z"),
            want
        );
        assert_eq!(parse_history_target_time("2024-03-09T16:00:00Z"), want);
        assert_eq!(parse_history_target_time("timestamp="), None);
        assert_eq!(parse_history_target_time(""), None);
        assert_eq!(parse_history_target_time("yesterday"), None);
    }

    #[test]
    fn corpus_frames_translate_to_the_expected_events() {
        // 01: a classic line. The corpus file carries CRLF, which the framer strips.
        let classic = corpus_frame(include_bytes!("testdata/corpus/01-classic-privmsg.txt"));
        let events = translate(
            NETWORK_A,
            "omairc",
            &features(),
            &classic,
            reducer_timestamp(),
            None,
        );
        let message = message_of(&events);
        assert_eq!(message.conversation.normalized_target, "#c");
        assert_eq!(message.author, "n");
        assert_eq!(message.body, "hello world");
        assert_eq!(message.msgid, Id::default());

        // 02: a tagged line. msgid survives and the +typing tag adds no event.
        let tagged = corpus_frame(include_bytes!("testdata/corpus/02-tagged-privmsg.txt"));
        let events = translate(
            NETWORK_A,
            "omairc",
            &features(),
            &tagged,
            reducer_timestamp(),
            None,
        );
        assert_eq!(
            events.len(),
            1,
            "+typing on a PRIVMSG is not a typing event"
        );
        let message = message_of(&events);
        assert_eq!(message.msgid, Id::new("abc"));
        assert_eq!(message.body, "tagged");

        // 03: the Latin-1 body decodes byte for byte as the scalar it names.
        let latin1 = corpus_frame(include_bytes!("testdata/corpus/03-latin1-e9-body.txt"));
        let events = translate(
            NETWORK_A,
            "omairc",
            &features(),
            &latin1,
            reducer_timestamp(),
            None,
        );
        assert_eq!(message_of(&events).body, "caf\u{e9}");
    }

    #[test]
    fn corpus_tagged_line_replays_as_chat_with_its_msgid() {
        let tagged = corpus_frame(include_bytes!("testdata/corpus/02-tagged-privmsg.txt"));
        let history = history_of(
            &HistoryBatch {
                target: "#c".to_owned(),
                lines: vec![tagged],
                ..HistoryBatch::default()
            },
            None,
        );
        assert_eq!(history.lines.len(), 1);
        assert_eq!(history.lines[0].kind, MessageKindTag::Chat);
        assert_eq!(history.lines[0].msgid, Id::new("abc"));
    }

    /// Parses one corpus file as the framer hands it to the translator: the
    /// trailing CRLF removed, the bytes otherwise untouched.
    fn corpus_frame(bytes: &[u8]) -> IrcMessage {
        let trimmed = bytes
            .strip_suffix(b"\r\n")
            .unwrap_or_else(|| panic!("corpus file must end in CRLF"));
        parse(trimmed).unwrap_or_else(|err| panic!("corpus frame failed to parse: {err}"))
    }

    #[test]
    fn helper_edges_match_the_go_readers() {
        assert_eq!(
            features().case_mapping().normalize("#Room"),
            "#room",
            "channels key on the folded name"
        );
        assert!(!is_go_int(""));
        assert!(is_go_int("-3"));
        assert!(!all_digits("+1"));
        assert_eq!(action_text("\u{1}ACTION \u{1}"), Some(""));
        assert_eq!(action_text("\u{1}ACTION"), None);
    }
}
