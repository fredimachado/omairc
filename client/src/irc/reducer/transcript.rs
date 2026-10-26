//! The transcript half of the reducer: admitting rows, the 2000-row cap, the
//! log, live chat, event and error rows, the unread and mention accounting for
//! an arriving line, and the membership folding a replay batch goes through.
//!
//! Port of `tui/internal/irc/reducer.go` (Go: `appendChat`, `noteChatArrival`,
//! `appendEvent`, `admitMembershipReplay`, `appendWhois`, `appendError`,
//! `hydrateFromLog`, `persistMessage`, `persistMessagesPrepend`, `capMessages`,
//! `dropLastTranscriptRow`, `classifyChatLine`, `inboxKindFor`, `kindToken`,
//! `kindFromToken`, `persistableKind`, `admitMessage`) and of
//! `tui/internal/irc/reducer.go`.

use std::collections::HashSet;

use chrono::{DateTime, Utc};

use crate::irc::event::{
    ConversationKey, HistoryEvent, InboxArrival, MentionArrival, MsgId,
};
use crate::irc::inbox::InboxKind;
use crate::irc::membershipnoise::{fold_membership, FoldKind, MembershipNoise};
use crate::irc::world::{Message, MessageKind, Origin};

use super::facts::Facts;
use super::log::TranscriptLine;
use super::state::{zero_time, ConversationState, MAX_MESSAGES};
use super::EventReducer;

/// The reducer's reason for surfacing a chat line (the Go `chatLineReason`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ChatLineReason {
    NickMention,
    Highlight,
    Direct,
}

/// One live chat line the reducer admits, before its row exists.
pub(super) struct ChatLine<'a> {
    pub key: &'a ConversationKey,
    pub display_target: &'a str,
    pub author: &'a str,
    pub body: &'a str,
    pub timestamp: DateTime<Utc>,
    pub server_time: Option<DateTime<Utc>>,
    pub kind: MessageKind,
    pub msgid: &'a MsgId,
    pub only_visible_to_you: bool,
}

/// What `note_chat_arrival` needs about one admitted chat row.
pub(super) struct ChatArrival<'a> {
    pub author: &'a str,
    pub body: &'a str,
    pub kind: MessageKind,
    pub msgid: &'a MsgId,
    pub sequence: i64,
    pub server_time: Option<DateTime<Utc>>,
    pub origin: Origin,
    /// The batch the row came from, for replay. `None` for a live line.
    pub history: Option<&'a HistoryEvent>,
}

/// One membership line from a replay batch, before it meets the transcript.
pub(super) struct ReplayMembership<'a> {
    pub body: &'a str,
    pub timestamp: DateTime<Utc>,
    pub msgid: &'a MsgId,
    pub server_time: Option<DateTime<Utc>>,
}

/// Where a replayed membership line landed (`membershipReplayResult`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MembershipReplayResult {
    pub added_row: bool,
    pub sequence: i64,
    pub dropped: bool,
    pub dropped_sequence: i64,
}

/// Appends a row and assigns its sequence (`admitMessage`).
pub(super) fn admit_message(conv: &mut ConversationState, mut message: Message) {
    message.sequence = conv.next_sequence;
    conv.next_sequence += 1;
    conv.messages.push(message);
}

/// Trims the transcript to `MAX_MESSAGES`, dropping the oldest rows and their
/// msgids (`capMessages`).
pub(super) fn cap_messages(conv: &mut ConversationState) {
    cap_with_policy(conv, true);
}

/// The tail-first trim a CHATHISTORY older page uses (`capMessagesPreferTail`).
pub(super) fn cap_messages_prefer_tail(conv: &mut ConversationState) {
    cap_with_policy(conv, false);
}

fn cap_with_policy(conv: &mut ConversationState, drop_from_head: bool) {
    if conv.messages.len() <= MAX_MESSAGES {
        return;
    }
    let extra = conv.messages.len() - MAX_MESSAGES;
    if drop_from_head {
        for message in &conv.messages[..extra] {
            if !message.msgid.is_empty() {
                conv.message_ids.remove(&message.msgid);
            }
        }
        conv.messages.drain(..extra);
        conv.trimmed += extra;
        if let Some(channel) = conv.channel.as_mut() {
            if let Some(anchor) = channel.history_anchor {
                if anchor.sequence < conv.trimmed as i64 {
                    channel.history_anchor = None;
                }
            }
        }
        return;
    }
    let start = conv.messages.len() - extra;
    for message in &conv.messages[start..] {
        if !message.msgid.is_empty() {
            conv.message_ids.remove(&message.msgid);
        }
    }
    conv.messages.truncate(start);
}

/// Removes the newest row, forgetting its msgid and shifting a history anchor
/// that sat past it (`dropLastTranscriptRow`).
pub(super) fn drop_last_transcript_row(conv: &mut ConversationState) {
    let Some(last) = conv.messages.last() else {
        return;
    };
    let msgid = last.msgid.clone();
    if !msgid.is_empty() {
        conv.message_ids.remove(&msgid);
    }
    let removed_index = conv.messages.len() as i64 - 1;
    conv.messages.pop();
    let trimmed = conv.trimmed as i64;
    if let Some(anchor) = conv
        .channel
        .as_mut()
        .and_then(|channel| channel.history_anchor.as_mut())
    {
        if anchor.sequence - trimmed > removed_index {
            anchor.sequence -= 1;
        }
    }
}

/// Whether a row kind is written to the log (`persistableKind`).
pub(super) fn persistable_kind(kind: MessageKind) -> bool {
    matches!(
        kind,
        MessageKind::Message | MessageKind::Notice | MessageKind::Action | MessageKind::Event
    )
}

/// The stored token for a row kind (`kindToken`). Errors persist as `event`.
pub(super) fn kind_token(kind: MessageKind) -> Option<&'static str> {
    match kind {
        MessageKind::Action => Some("action"),
        MessageKind::Event | MessageKind::Error => Some("event"),
        MessageKind::Whois => Some("whois"),
        MessageKind::Notice => Some("notice"),
        MessageKind::Message => Some("message"),
    }
}

/// The row kind a stored token names (`kindFromToken`). Only the kinds the log
/// stores resolve.
pub(super) fn kind_from_token(token: &str) -> Option<MessageKind> {
    match token {
        "action" => Some(MessageKind::Action),
        "event" => Some(MessageKind::Event),
        "notice" => Some(MessageKind::Notice),
        "message" => Some(MessageKind::Message),
        _ => None,
    }
}

fn transcript_line(message: &Message) -> Option<TranscriptLine> {
    Some(TranscriptLine {
        timestamp: message.time,
        author: message.author.clone(),
        kind: kind_token(message.kind)?.to_owned(),
        body: message.body.clone(),
        msgid: message.msgid.value.clone(),
        only_visible_to_you: message.only_visible_to_you,
    })
}

/// Appends one row to the log when it is a persistable kind (`persistMessage`).
pub(super) fn persist_message(facts: &Facts, conv: &ConversationState, message: &Message) {
    let Some(log) = facts.log.as_ref() else {
        return;
    };
    if !persistable_kind(message.kind) {
        return;
    }
    let Some(line) = transcript_line(message) else {
        return;
    };
    let network = conv.key.network_id.as_str();
    log.append(
        network,
        &conv.key.normalized_target,
        facts.case_mapping(network),
        &line,
    );
}

/// Writes rows at the head of the log, oldest first (`persistMessagesPrepend`).
pub(super) fn persist_messages_prepend(facts: &Facts, conv: &ConversationState, messages: &[Message]) {
    let Some(log) = facts.log.as_ref() else {
        return;
    };
    let lines: Vec<TranscriptLine> = messages
        .iter()
        .filter(|message| persistable_kind(message.kind))
        .filter_map(transcript_line)
        .collect();
    if lines.is_empty() {
        return;
    }
    let network = conv.key.network_id.as_str();
    log.prepend(
        network,
        &conv.key.normalized_target,
        facts.case_mapping(network),
        &lines,
    );
}

/// Loads a new conversation's stored tail, skipping unknown kinds and msgids
/// already present (`hydrateFromLog`).
pub(super) fn hydrate_from_log(facts: &Facts, conv: &mut ConversationState) {
    let Some(log) = facts.log.as_ref() else {
        return;
    };
    let network = conv.key.network_id.clone();
    let lines = log.read_tail(
        &network,
        &conv.key.normalized_target,
        facts.case_mapping(&network),
        MAX_MESSAGES,
    );
    for line in lines {
        let Some(kind) = kind_from_token(&line.kind) else {
            continue;
        };
        let msgid = MsgId::new(line.msgid.clone());
        if !msgid.is_empty() {
            if conv.message_ids.contains(&msgid) {
                continue;
            }
            conv.message_ids.insert(msgid.clone());
        }
        let mentioned = facts.transcript_highlight(&network, &line.author, kind, &line.body);
        admit_message(
            conv,
            Message {
                kind,
                author: line.author,
                body: line.body,
                time: line.timestamp,
                server_time: None,
                msgid,
                origin: Origin::Replay,
                mentioned,
                collapsible: false,
                sequence: 0,
                only_visible_to_you: line.only_visible_to_you,
            },
        );
    }
    cap_messages(conv);
}

/// Which chat reason a line carries: a nick hit, a highlight hit, or a direct
/// message (`classifyChatLine`). The bool is false when the line carries none.
pub(super) fn classify_chat_line(
    is_channel: bool,
    kind: MessageKind,
    self_author: bool,
    nick_hit: bool,
    highlight_hit: bool,
) -> (ChatLineReason, bool) {
    if !matches!(kind, MessageKind::Message | MessageKind::Action) || self_author {
        return (ChatLineReason::NickMention, false);
    }
    if nick_hit {
        return (ChatLineReason::NickMention, true);
    }
    if highlight_hit {
        return (ChatLineReason::Highlight, true);
    }
    if !is_channel {
        return (ChatLineReason::Direct, true);
    }
    (ChatLineReason::NickMention, false)
}

/// The inbox kind for a chat reason (`inboxKindFor`).
pub(super) fn inbox_kind_for(reason: ChatLineReason) -> InboxKind {
    match reason {
        ChatLineReason::NickMention => InboxKind::Mention,
        ChatLineReason::Highlight => InboxKind::Highlight,
        ChatLineReason::Direct => InboxKind::Direct,
    }
}

/// Plants the mention and inbox arrivals for one admitted chat row and advances
/// its unread accounting (`noteChatArrival`). Replay that is backlog while the
/// window is unfocused does not plant the boundary mid-splice.
pub(super) fn note_chat_arrival(
    facts: &mut Facts,
    conv: &mut ConversationState,
    key: &ConversationKey,
    arrival: ChatArrival<'_>,
) {
    if arrival.history.is_some_and(|history| history.older_page) {
        return;
    }
    let network = key.network_id.as_str();
    let author = arrival.author;
    let body = arrival.body;
    let self_author = facts.is_self(network, author)
        || arrival.history.is_some_and(|history| {
            facts.bouncer_query_own_line(history, author)
                || facts.bouncer_channel_own_line(history, author)
        });
    let nick_hit = facts.is_nick_mention(network, body);
    let highlight_hit = !nick_hit && facts.is_highlight_hit(network, body);
    let (reason, has_reason) = classify_chat_line(
        conv.is_channel(),
        arrival.kind,
        self_author,
        nick_hit,
        highlight_hit,
    );
    if self_author {
        return;
    }
    if super::readmarker::covered_by_read_marker(conv, arrival.server_time) {
        return;
    }
    let selected = facts.selected.as_ref() == Some(key);
    if has_reason && !conv.muted {
        facts.mention_arrival = Some(MentionArrival {
            author: author.to_owned(),
            body: body.to_owned(),
            network_id: key.network_id.clone(),
            target: conv.target.clone(),
            msgid: arrival.msgid.clone(),
        });
    }
    // The inbox is the waiting list for conversations the user is not looking
    // at. Direct messages never enter it.
    if arrival.origin == Origin::Live
        && has_reason
        && !conv.muted
        && !selected
        && reason != ChatLineReason::Direct
    {
        facts.inbox_arrival = Some(InboxArrival {
            kind: inbox_kind_for(reason),
            actor: author.to_owned(),
            body: body.to_owned(),
            network_id: key.network_id.clone(),
            target: conv.target.clone(),
            msgid: arrival.msgid.clone(),
        });
    }
    // Selected live chat is unread only while unfocused. Replay while unfocused
    // is backlog, so it must not plant the mark mid-splice. Focused replay plants
    // the mark without bumping unread.
    let live = arrival.origin == Origin::Live;
    let skip_live_focused = selected && live && !facts.window_inactive;
    let skip_replay_unfocused = selected && !live && facts.window_inactive;
    if skip_live_focused || skip_replay_unfocused {
        return;
    }
    let hold_focused_replay_mark = selected && !live && conv.unread_mark.is_some();
    if conv.unread == 0 && !hold_focused_replay_mark {
        conv.unread_mark = Some(arrival.sequence);
    }
    if selected && !live {
        return;
    }
    conv.unread += 1;
    if has_reason
        && matches!(
            reason,
            ChatLineReason::NickMention | ChatLineReason::Highlight
        )
        && !conv.muted
    {
        conv.mentions += 1;
    }
}

/// Adds one event row (join, part, quit, nick, kick, mode). Membership lines
/// follow the noise setting: folded lines merge into the caption above, a leave
/// the same nick undoes at once is dropped, every line stays separate when shown
/// each, and hidden lines are skipped (`appendEvent`).
pub(super) fn append_event(
    facts: &Facts,
    conv: &mut ConversationState,
    body: &str,
    collapsible: bool,
    membership: bool,
) {
    let mut collapsible = collapsible;
    let noise = facts.membership_noise;
    if membership && noise == MembershipNoise::Hidden {
        return;
    }
    if membership && noise == MembershipNoise::Every {
        collapsible = false;
    }
    if membership && noise == MembershipNoise::Folded && !conv.messages.is_empty() {
        let network = conv.key.network_id.clone();
        let last_index = conv.messages.len() - 1;
        let last = &conv.messages[last_index];
        if last.kind == MessageKind::Event && last.collapsible {
            let folded = fold_membership(&last.body, body, |left, right| {
                facts.equals(&network, left, right)
            });
            if folded.kind == FoldKind::Cancel {
                if folded.drop_row {
                    drop_last_transcript_row(conv);
                } else {
                    conv.messages[last_index].body = folded.body;
                }
                return;
            }
            if collapsible {
                conv.messages[last_index].body = folded.body;
                return;
            }
        }
    }
    admit_message(
        conv,
        Message {
            kind: MessageKind::Event,
            author: String::new(),
            body: body.to_owned(),
            time: zero_time(),
            server_time: None,
            msgid: MsgId::default(),
            origin: Origin::Live,
            mentioned: false,
            collapsible,
            sequence: 0,
            only_visible_to_you: false,
        },
    );
    let last = conv
        .messages
        .last()
        .cloned()
        .expect("a row was just admitted");
    persist_message(facts, conv, &last);
    cap_messages(conv);
}

/// Folds one replayed membership line into an existing caption row
/// (`foldInto`). Returns false when the row is not a caption.
fn fold_into(
    row: &mut Message,
    body: &str,
    network_id: &str,
    facts: &Facts,
    message_ids: &mut HashSet<MsgId>,
    result: &mut MembershipReplayResult,
) -> bool {
    if row.kind != MessageKind::Event || !row.collapsible {
        return false;
    }
    let folded = fold_membership(&row.body, body, |left, right| {
        facts.equals(network_id, left, right)
    });
    if folded.kind == FoldKind::Cancel && folded.drop_row {
        result.dropped = true;
        result.dropped_sequence = row.sequence;
        if !row.msgid.is_empty() {
            message_ids.remove(&row.msgid);
        }
        return true;
    }
    row.body = folded.body;
    result.dropped = false;
    true
}

/// Places one replayed membership line into the run being built, or folds it
/// into the caption it joins (`admitMembershipReplay`). `at` is the splice point
/// in the transcript and moves when a folded-away row is removed above it.
pub(super) fn admit_membership_replay(
    facts: &Facts,
    conv: &mut ConversationState,
    run: &mut Vec<Message>,
    at: &mut usize,
    line: &ReplayMembership<'_>,
) -> MembershipReplayResult {
    let mut result = MembershipReplayResult::default();
    let noise = facts.membership_noise;
    if noise == MembershipNoise::Hidden {
        return result;
    }
    let network = conv.key.network_id.clone();
    let show_each = noise == MembershipNoise::Every;

    if !show_each && !run.is_empty() {
        let last = run.len() - 1;
        if fold_into(
            &mut run[last],
            line.body,
            &network,
            facts,
            &mut conv.message_ids,
            &mut result,
        ) {
            if result.dropped {
                run.pop();
            }
            return result;
        }
    }
    if !show_each && run.is_empty() && *at > 0 && *at <= conv.messages.len() {
        let index = *at - 1;
        if fold_into(
            &mut conv.messages[index],
            line.body,
            &network,
            facts,
            &mut conv.message_ids,
            &mut result,
        ) {
            if result.dropped {
                conv.messages.remove(index);
                *at -= 1;
                let trimmed = conv.trimmed as i64;
                if let Some(anchor) = conv
                    .channel
                    .as_mut()
                    .and_then(|channel| channel.history_anchor.as_mut())
                {
                    if anchor.sequence - trimmed > index as i64 {
                        anchor.sequence -= 1;
                    }
                }
            }
            return result;
        }
    }

    let sequence = conv.next_sequence;
    conv.next_sequence += 1;
    run.push(Message {
        kind: MessageKind::Event,
        author: String::new(),
        body: line.body.to_owned(),
        time: line.timestamp,
        server_time: line.server_time,
        msgid: line.msgid.clone(),
        origin: Origin::Replay,
        mentioned: false,
        collapsible: !show_each,
        sequence,
        only_visible_to_you: false,
    });
    result.added_row = true;
    result.sequence = sequence;
    result
}

/// Adds one WHOIS transcript row (`appendWhois`).
pub(super) fn append_whois(conv: &mut ConversationState, body: &str) {
    admit_message(
        conv,
        Message {
            kind: MessageKind::Whois,
            author: String::new(),
            body: body.to_owned(),
            time: zero_time(),
            server_time: None,
            msgid: MsgId::default(),
            origin: Origin::Live,
            mentioned: false,
            collapsible: false,
            sequence: 0,
            only_visible_to_you: false,
        },
    );
    cap_messages(conv);
}

/// Adds one error row (`appendError`).
pub(super) fn append_error(conv: &mut ConversationState, body: &str) {
    admit_message(
        conv,
        Message {
            kind: MessageKind::Error,
            author: String::new(),
            body: body.to_owned(),
            time: zero_time(),
            server_time: None,
            msgid: MsgId::default(),
            origin: Origin::Live,
            mentioned: false,
            collapsible: false,
            sequence: 0,
            only_visible_to_you: false,
        },
    );
    cap_messages(conv);
}

impl EventReducer {
    /// Admits one live chat line: dedupes by msgid, persists it, caps the
    /// transcript, clears the author's typing hint and notes the arrival
    /// (`appendChat`). A missing conversation is created only for a cause that may
    /// insert one.
    pub(super) fn append_chat(&mut self, line: ChatLine<'_>) {
        let key = line.key;
        let self_author = self.facts.is_self(&key.network_id, line.author);
        let cause = if self_author {
            crate::irc::event::IrcConversationCause::InboundSelf
        } else {
            crate::irc::event::IrcConversationCause::InboundOther
        };
        if !self.ensure_key(key, line.display_target, cause) {
            return;
        }
        let Some(conv) = self.conversations.get_mut(key) else {
            return;
        };
        if !line.msgid.is_empty() {
            if conv.message_ids.contains(line.msgid) {
                return;
            }
            conv.message_ids.insert(line.msgid.clone());
        }
        let mentioned = self
            .facts
            .transcript_highlight(&key.network_id, line.author, line.kind, line.body);
        admit_message(
            conv,
            Message {
                kind: line.kind,
                author: line.author.to_owned(),
                body: line.body.to_owned(),
                time: line.timestamp,
                server_time: line.server_time,
                msgid: line.msgid.clone(),
                origin: Origin::Live,
                mentioned,
                collapsible: false,
                sequence: 0,
                only_visible_to_you: line.only_visible_to_you,
            },
        );
        let last = conv
            .messages
            .last()
            .cloned()
            .expect("a row was just admitted");
        persist_message(&self.facts, conv, &last);
        cap_messages(conv);
        let normalized = self.facts.normalize(&key.network_id, line.author);
        conv.typing.remove(&normalized);
        note_chat_arrival(
            &mut self.facts,
            conv,
            key,
            ChatArrival {
                author: line.author,
                body: line.body,
                kind: line.kind,
                msgid: line.msgid,
                sequence: last.sequence,
                server_time: last.server_time,
                origin: Origin::Live,
                history: None,
            },
        );
    }
}
