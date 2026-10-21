//! The replay half of the reducer: where a bouncer or CHATHISTORY batch lands in
//! a transcript, the msgid and membership dedup it runs, and the queue that holds
//! a batch until its conversation (or its join) exists.
//!
//! Port of `tui/internal/irc/reducer.go` (Go: `spliceHistory`, `peekSpliceIndex`,
//! `takeSpliceIndex`, `holdPendingPlayback`, `absorbPendingQueryPlayback`,
//! `releasePendingPlayback`, `dropPendingPlayback`, `dropKeptPlayback`,
//! `ReleasePendingQueryPlayback`, `PlaybackBatchKept`, `reduceHistory`,
//! `insertMessages`) and of `src/irc/irceventreducer.cpp` (C++).

use chrono::{DateTime, Utc};

use crate::irc::casemapping::IrcCaseMapping;
use crate::irc::event::{
    ConversationKey, HistoryEvent, HistoryKind, IrcConversationCause, KeptReplay,
    MessageKindTag, RememberedQuery, ReplayLine,
};
use crate::irc::world::{Message, MessageKind, Origin};

use super::facts::Facts;
use super::state::{zero_time, ConversationState};
use super::transcript::{
    admit_membership_replay, cap_messages, cap_messages_prefer_tail, note_chat_arrival,
    persist_message, persist_messages_prepend, ChatArrival, ReplayMembership,
};
use super::EventReducer;

/// Whether a splice consumes the history anchor or keeps it, shifted by what
/// landed (`historyAnchorUse`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AnchorUse {
    Consume,
    Keep,
}

/// A splice's bookkeeping for one replay line, applied once the run is in place
/// (`pendingPlaybackNote`).
struct PendingNote {
    sequence: i64,
    server_time: Option<DateTime<Utc>>,
    keep_without_row: bool,
}

/// Where a batch's next line lands without consuming the anchor
/// (`peekSpliceIndex`). A direct message, or a joined channel with no anchor,
/// splices at the tail. An unjoined channel with no anchor cannot splice.
pub(super) fn peek_splice_index(conv: &ConversationState) -> Option<usize> {
    let len = conv.messages.len();
    let Some(channel) = conv.channel.as_ref() else {
        return Some(len);
    };
    let Some(anchor) = channel.history_anchor else {
        return if channel.joined { Some(len) } else { None };
    };
    let index = anchor.sequence - conv.trimmed as i64;
    if index < 0 || index > len as i64 {
        return None;
    }
    Some(index as usize)
}

/// Where a batch lands, dropping an anchor that no longer names a row
/// (`takeSpliceIndex`). A usable anchor stays until a line lands.
pub(super) fn take_splice_index(conv: &mut ConversationState) -> Option<usize> {
    let len = conv.messages.len();
    let trimmed = conv.trimmed as i64;
    let Some(channel) = conv.channel.as_mut() else {
        return Some(len);
    };
    let anchor = channel.history_anchor?;
    let index = anchor.sequence - trimmed;
    if index < 0 || index > len as i64 {
        channel.history_anchor = None;
        return None;
    }
    Some(index as usize)
}

/// The sequence of the first row, in the transcript or the run being built, that
/// satisfies `predicate`.
fn matched_sequence(
    conv: &ConversationState,
    run: &[Message],
    predicate: impl Fn(&Message) -> bool,
) -> Option<i64> {
    conv.messages
        .iter()
        .chain(run.iter())
        .find(|message| predicate(message))
        .map(|message| message.sequence)
}

/// Whether a stored row and a replay line are the same chat line: same kind and
/// body, the same timestamp to the millisecond, and the same author under the
/// mapping (`sameReplayLine`).
fn same_replay_line(
    message: &Message,
    line: &ReplayLine,
    kind: MessageKind,
    mapping: IrcCaseMapping,
) -> bool {
    if message.kind != kind || message.body != line.body {
        return false;
    }
    if message.time == zero_time() || line.timestamp == zero_time() {
        return false;
    }
    if message.time.timestamp_millis() != line.timestamp.timestamp_millis() {
        return false;
    }
    mapping.equals(&message.author, &line.author)
}

/// Merges a replay batch into a transcript position: dedupes known msgids and
/// matching rows, notes the arrivals, and records the lines kept
/// (`spliceHistory`). The caller has checked the conversation exists.
pub(super) fn splice_history(
    facts: &mut Facts,
    conv: &mut ConversationState,
    event: &HistoryEvent,
    anchor_use: AnchorUse,
) {
    let older_page = event.kind == HistoryKind::Chat && event.older_page;
    let mut at = match anchor_use {
        AnchorUse::Consume if older_page => 0,
        AnchorUse::Consume => match take_splice_index(conv) {
            Some(index) => index,
            None => return,
        },
        AnchorUse::Keep => match peek_splice_index(conv) {
            Some(index) => index,
            None => return,
        },
    };
    let previous_size = conv.messages.len();
    let network = event.conversation.network_id.clone();
    let mapping = facts.case_mapping(&network);

    let mut run: Vec<Message> = Vec::new();
    let mut pending_notes: Vec<PendingNote> = Vec::new();
    let mut saw_self = false;
    // A page of only join, part, quit and nick lines can leave no new row when
    // those lines are hidden or folded away. That is not an empty page: the
    // reader is still paging, so the tail cap stays armed.
    let mut saw_line = false;
    let mut membership_only = true;

    for line in &event.lines {
        saw_line = true;
        if line.kind != MessageKindTag::Event {
            membership_only = false;
        }
        if !saw_self && facts.bouncer_query_own_line(event, &line.author) {
            saw_self = true;
        }

        if !line.msgid.is_empty() && conv.message_ids.contains(&line.msgid) {
            if let Some(sequence) = matched_sequence(conv, &run, |message| {
                message.msgid == line.msgid
            }) {
                pending_notes.push(PendingNote {
                    sequence,
                    server_time: line.server_time,
                    keep_without_row: false,
                });
            }
            continue;
        }

        if line.kind == MessageKindTag::Event {
            if !line.msgid.is_empty() {
                conv.message_ids.insert(line.msgid.clone());
            }
            let admitted = admit_membership_replay(
                facts,
                conv,
                &mut run,
                &mut at,
                &ReplayMembership {
                    body: &line.body,
                    timestamp: line.timestamp,
                    msgid: &line.msgid,
                    server_time: line.server_time,
                },
            );
            if admitted.dropped {
                for note in &mut pending_notes {
                    if note.sequence == admitted.dropped_sequence {
                        note.keep_without_row = true;
                    }
                }
            }
            if admitted.added_row {
                pending_notes.push(PendingNote {
                    sequence: admitted.sequence,
                    server_time: line.server_time,
                    keep_without_row: false,
                });
            } else {
                pending_notes.push(PendingNote {
                    sequence: 0,
                    server_time: line.server_time,
                    keep_without_row: true,
                });
            }
            continue;
        }

        let kind = if line.kind == MessageKindTag::Emote {
            MessageKind::Action
        } else {
            MessageKind::Message
        };
        if let Some(sequence) = matched_sequence(conv, &run, |message| {
            if !line.msgid.is_empty() && !message.msgid.is_empty() && message.msgid != line.msgid {
                return false;
            }
            same_replay_line(message, line, kind, mapping)
        }) {
            pending_notes.push(PendingNote {
                sequence,
                server_time: line.server_time,
                keep_without_row: false,
            });
            continue;
        }
        if !line.msgid.is_empty() {
            conv.message_ids.insert(line.msgid.clone());
        }
        let sequence = conv.next_sequence;
        conv.next_sequence += 1;
        run.push(Message {
            kind,
            author: line.author.clone(),
            body: line.body.clone(),
            time: line.timestamp,
            server_time: line.server_time,
            msgid: line.msgid.clone(),
            origin: Origin::Replay,
            mentioned: facts.transcript_highlight(&network, &line.author, kind, &line.body),
            collapsible: false,
            sequence,
            only_visible_to_you: line.only_visible_to_you,
        });
        pending_notes.push(PendingNote {
            sequence,
            server_time: line.server_time,
            keep_without_row: false,
        });
    }

    if saw_self {
        let target = if event.target.is_empty() {
            conv.target.clone()
        } else {
            event.target.clone()
        };
        facts.remembered_queries.push(RememberedQuery {
            network_id: network.clone(),
            target,
        });
    }

    if !run.is_empty() {
        let run_len = run.len() as i64;
        let insert_at = at.min(conv.messages.len());
        conv.messages
            .splice(insert_at..insert_at, run.iter().cloned());
        if at == 0 {
            persist_messages_prepend(facts, conv, &run);
        } else {
            for message in &run {
                persist_message(facts, conv, message);
            }
        }
        // Consume drops the anchor only once a replay line has landed. Keep
        // still names the join line, shifted forward by what landed.
        if let Some(channel) = conv.channel.as_mut() {
            match anchor_use {
                AnchorUse::Consume if !older_page => channel.history_anchor = None,
                AnchorUse::Consume | AnchorUse::Keep => {
                    if let Some(anchor) = channel.history_anchor.as_mut() {
                        anchor.sequence += run_len;
                    }
                }
            }
        }
        if at != previous_size {
            conv.splice_epoch += 1;
        }
        if conv.history_page_cap_tail && event.kind == HistoryKind::Chat {
            conv.history_page_cap_tail = false;
            cap_messages_prefer_tail(conv);
        } else {
            cap_messages(conv);
        }
        for message in &run {
            if message.kind == MessageKind::Event {
                continue;
            }
            note_chat_arrival(
                facts,
                conv,
                &event.conversation,
                ChatArrival {
                    author: &message.author,
                    body: &message.body,
                    kind: message.kind,
                    msgid: &message.msgid,
                    sequence: message.sequence,
                    server_time: message.server_time,
                    origin: Origin::Replay,
                    history: Some(event),
                },
            );
        }
    } else if event.kind == HistoryKind::Chat
        && (conv.history_page_cap_tail || older_page)
        && !(saw_line && membership_only)
    {
        conv.history_page_cap_tail = false;
    }

    let mut kept_playback_line = false;
    for note in &pending_notes {
        let still_present = note.keep_without_row
            || conv
                .messages
                .iter()
                .any(|message| message.sequence == note.sequence);
        if !still_present {
            continue;
        }
        if let Some(server_time) = note.server_time {
            facts.kept_replay.push(KeptReplay {
                network_id: network.clone(),
                target: event.target.clone(),
                server_time,
            });
        }
        kept_playback_line = true;
    }
    if kept_playback_line && event.kind == HistoryKind::BouncerPlayback && conv.is_channel() {
        facts.kept_playback_channel.insert(conv.key.clone());
    }
}

impl EventReducer {
    /// Routes one replay batch (`reduce(IrcHistoryEvent)`). A bouncer channel
    /// batch waits for the self-join. A bouncer query batch joins a held one.
    /// Everything else splices now, creating a missing conversation only for a
    /// line from the peer.
    pub(super) fn reduce_history(&mut self, event: &HistoryEvent) {
        let key = event.conversation.clone();
        let network = key.network_id.clone();
        let channel_target = self.facts.is_channel(&network, &key.normalized_target);
        if event.kind == HistoryKind::BouncerPlayback && channel_target {
            let joined = self
                .conversations
                .get(&key)
                .and_then(|conv| conv.channel.as_ref())
                .is_some_and(|channel| channel.joined);
            if !joined {
                self.hold_pending_playback(event);
                return;
            }
            self.splice_into(&key, event, AnchorUse::Keep);
            return;
        }
        if event.kind == HistoryKind::BouncerPlayback
            && !channel_target
            && self.absorb_pending_query_playback(event)
        {
            return;
        }
        if !self.conversations.contains_key(&key) {
            if channel_target || event.kind == HistoryKind::Chat {
                return;
            }
            if !self.facts.replay_from_peer(event) {
                if event.kind == HistoryKind::BouncerPlayback
                    && self.facts.query_restore_pending.contains(&network)
                {
                    self.hold_pending_playback(event);
                }
                return;
            }
            if self.facts.replay_only_repeats_persisted_ids(event) {
                return;
            }
            if !self.ensure_key(&key, &event.target, IrcConversationCause::InboundOther) {
                return;
            }
        }
        self.splice_into(&key, event, AnchorUse::Consume);
    }

    /// Splices a batch into an existing conversation. A missing one is a no-op.
    fn splice_into(&mut self, key: &ConversationKey, event: &HistoryEvent, anchor_use: AnchorUse) {
        let Some(conv) = self.conversations.get_mut(key) else {
            return;
        };
        splice_history(&mut self.facts, conv, event, anchor_use);
    }

    /// Holds a batch, or extends the one already held, until its conversation
    /// can take it (`holdPendingPlayback`).
    fn hold_pending_playback(&mut self, event: &HistoryEvent) {
        if event.lines.is_empty() {
            return;
        }
        if let Some(held) = self.facts.pending_playback.get_mut(&event.conversation) {
            held.lines.extend(event.lines.iter().cloned());
            return;
        }
        self.facts
            .pending_playback
            .insert(event.conversation.clone(), event.clone());
    }

    /// Appends a query batch onto a held one and splices when the combined batch
    /// has a peer line. Returns whether it handled the event
    /// (`absorbPendingQueryPlayback`).
    fn absorb_pending_query_playback(&mut self, event: &HistoryEvent) -> bool {
        let key = event.conversation.clone();
        let network = key.network_id.clone();
        let Some(held_entry) = self.facts.pending_playback.get_mut(&key) else {
            return false;
        };
        if !event.lines.is_empty() {
            held_entry.lines.extend(event.lines.iter().cloned());
        }
        let held = held_entry.clone();
        if !self.facts.replay_from_peer(&held) {
            if !self.facts.query_restore_pending.contains(&network) {
                self.facts.pending_playback.remove(&key);
            }
            return true;
        }
        if !self.conversations.contains_key(&key) {
            if self.facts.replay_only_repeats_persisted_ids(&held) {
                if !self.facts.query_restore_pending.contains(&network) {
                    self.facts.pending_playback.remove(&key);
                }
                return true;
            }
            if !self.ensure_key(&key, &held.target, IrcConversationCause::InboundOther) {
                return true;
            }
        }
        self.facts.pending_playback.remove(&key);
        self.splice_into(&key, &held, AnchorUse::Consume);
        true
    }

    /// Splices a held batch into a conversation that now exists, keeping the
    /// anchor (`releasePendingPlayback`). Called when the self-join lands.
    pub(super) fn release_pending_playback(&mut self, key: &ConversationKey) {
        let Some(event) = self.facts.pending_playback.remove(key) else {
            return;
        };
        self.splice_into(key, &event, AnchorUse::Keep);
    }

    /// Forgets every held batch for one network (`dropPendingPlayback`).
    pub(super) fn drop_pending_playback(&mut self, network_id: &str) {
        if network_id.is_empty() {
            return;
        }
        self.facts
            .pending_playback
            .retain(|key, _| key.network_id != network_id);
    }

    /// Forgets the kept-playback channels of one network (`dropKeptPlayback`).
    pub(super) fn drop_kept_playback(&mut self, network_id: &str) {
        if network_id.is_empty() {
            return;
        }
        self.facts
            .kept_playback_channel
            .retain(|key| key.network_id != network_id);
    }

    /// Marks the MOTD-end restore finished for a network and splices the held
    /// query batches into the directs that now exist. Returns whether anything
    /// changed (`ReleasePendingQueryPlayback`). A self-only batch with no
    /// conversation is finished and does not move the clock.
    pub fn release_pending_query_playback(&mut self, network_id: &str) -> bool {
        if network_id.is_empty() {
            return false;
        }
        self.facts.query_restore_pending.remove(network_id);
        let keys: Vec<ConversationKey> = self
            .facts
            .pending_playback
            .keys()
            .filter(|key| {
                key.network_id == network_id
                    && !self.facts.is_channel(network_id, &key.normalized_target)
            })
            .cloned()
            .collect();
        let mut changed = false;
        for key in keys {
            let Some(event) = self.facts.pending_playback.remove(&key) else {
                continue;
            };
            if !self.conversations.contains_key(&key) {
                if !self.facts.replay_from_peer(&event) {
                    continue;
                }
                if self.facts.replay_only_repeats_persisted_ids(&event) {
                    continue;
                }
                if !self.ensure_key(&key, &event.target, IrcConversationCause::InboundOther) {
                    continue;
                }
            }
            self.splice_into(&key, &event, AnchorUse::Consume);
            changed = true;
        }
        changed
    }

    /// Whether a bouncer playback batch for this channel was spliced or deduped
    /// on the current connection (`PlaybackBatchKept`).
    pub fn playback_batch_kept(&self, network_id: &str, target: &str) -> bool {
        if network_id.is_empty() || target.is_empty() {
            return false;
        }
        self.facts
            .kept_playback_channel
            .contains(&self.conversation_key(network_id, target))
    }

    /// Takes the replay lines kept since the last call (`TakeKeptReplay`).
    pub fn take_kept_replay(&mut self) -> Vec<KeptReplay> {
        std::mem::take(&mut self.facts.kept_replay)
    }

    /// Takes the bouncer queries worth remembering since the last call
    /// (`TakeRememberedQueries`).
    pub fn take_remembered_queries(&mut self) -> Vec<RememberedQuery> {
        std::mem::take(&mut self.facts.remembered_queries)
    }
}
