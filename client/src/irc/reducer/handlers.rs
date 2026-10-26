//! The per-event folds: one handler for each translated event the reducer
//! consumes. `reduce` dispatches to them, and each handler mutates the
//! conversation it names and the facts it reads, in the order Go does.
//!
//! Port of the `reduce(...)` methods of `tui/internal/irc/reducer.go` (Go:
//! `reduceWelcome`, `reduceMessage`, `reduceNotice`, `reduceAction`,
//! `reduceJoin`, `reducePart`, `reduceQuit`, `reduceNick`, `reduceKick`,
//! `reduceTopic`, `reduceNames`, `reduceMode`, `reduceAway`, `reduceSelfAway`,
//! `reduceMemberMetadata`, `reduceAccount`, `reduceTyping`, `reduceWhoisTranscript`,
//! `reduceChannelError`, `reduceNickFacts`, `clearTyping`, `clearTypingEverywhere`,
//! `rekeyTyping`, `pruneExpiredTyping`, `forgetUnseen`) and of
//! `tui/internal/irc/reducer.go`.

use chrono::{DateTime, Utc};

use crate::irc::event::{
    AccountEvent, ActionEvent, AwayEvent, ChannelErrorEvent, ConversationKey, Event, InboxArrival,
    IrcConversationCause, JoinEvent, KickEvent, MemberMetadataEvent, MessageEvent, ModeEvent,
    MsgId, NamesEvent, NickEvent, NickFactsEvent, NoticeEvent, PartEvent, QuitEvent,
    SelfAwayEvent, TopicEvent, TypingEvent, WelcomeEvent, WhoisTranscriptEvent,
};
use crate::irc::inbox::InboxKind;
use crate::irc::membershipnoise::{join_line, nick_line, part_line, quit_line};
use crate::irc::presence;
use crate::irc::typing::{stored_typing_hint, typing_hint_retained};
use crate::irc::world::MessageKind;

use super::conversations::{start_names_sync, stop_names_sync};
use super::state::{MemberState, TranscriptAnchor};
use super::transcript::{
    append_error, append_event, append_whois, cap_messages, ChatLine,
};
use super::EventReducer;

/// The conversation a line is shown in: the event's target, or the normalized
/// target when the event carries none (`displayTarget`).
fn display_target<'a>(key: &'a ConversationKey, target: &'a str) -> &'a str {
    if target.is_empty() {
        &key.normalized_target
    } else {
        target
    }
}

impl EventReducer {
    /// Folds one translated event in (`Apply`). `now` is the caller's clock, used
    /// where the native core would read the wall clock.
    pub(super) fn reduce(&mut self, event: &Event, now: DateTime<Utc>) {
        match event {
            Event::Welcome(event) => self.reduce_welcome(event),
            Event::Message(event) => self.reduce_message(event),
            Event::Notice(event) => self.reduce_notice(event),
            Event::Action(event) => self.reduce_action(event),
            Event::Join(event) => self.reduce_join(event),
            Event::Part(event) => self.reduce_part(event),
            Event::Quit(event) => self.reduce_quit(event),
            Event::Nick(event) => self.reduce_nick(event),
            Event::Kick(event) => self.reduce_kick(event),
            Event::Topic(event) => self.reduce_topic(event),
            Event::Names(event) => self.reduce_names(event, now),
            Event::Mode(event) => self.reduce_mode(event),
            Event::Away(event) => self.reduce_away(event),
            Event::SelfAway(event) => self.reduce_self_away(event),
            Event::MemberMetadata(event) => self.reduce_member_metadata(event),
            Event::Account(event) => self.reduce_account(event),
            Event::Typing(event) => self.reduce_typing(event),
            Event::History(event) => self.reduce_history(event),
            Event::WhoisTranscript(event) => self.reduce_whois_transcript(event),
            Event::ChannelError(event) => self.reduce_channel_error(event),
            Event::NickFacts(event) => self.reduce_nick_facts(event),
        }
    }

    /// Resets a network on a new connection and marks open-direct restore pending
    /// (`reduceWelcome`).
    fn reduce_welcome(&mut self, event: &WelcomeEvent) {
        let network = event.network_id.as_str();
        self.drop_pending_playback(network);
        self.drop_kept_playback(network);
        self.clear_closed_network(network);
        self.facts.query_restore_pending.insert(network.to_owned());
        if let Some(current) = self.facts.current_nicks.get(network).cloned() {
            self.facts.remember_self_nick(network, &current);
        }
        self.facts.remember_self_nick(network, &event.current_nick);
        self.facts
            .current_nicks
            .insert(network.to_owned(), event.current_nick.clone());
        if let Some(presence) = self.facts.presence.get_mut(network) {
            presence.clear();
        }
        self.facts.self_away.remove(network);
        for conv in self.conversations.values_mut() {
            if conv.key.network_id != network {
                continue;
            }
            if let Some(channel) = conv.channel.as_mut() {
                channel.members.clear();
                channel.joined = false;
                channel.history_anchor = None;
                stop_names_sync(channel);
            }
            conv.history_page_cap_tail = false;
            conv.typing.clear();
        }
        self.refresh_mentions(network);
    }

    fn reduce_message(&mut self, event: &MessageEvent) {
        self.append_chat(ChatLine {
            key: &event.conversation,
            display_target: display_target(&event.conversation, &event.target),
            author: &event.author,
            body: &event.body,
            timestamp: event.timestamp,
            server_time: event.server_time,
            kind: MessageKind::Message,
            msgid: &event.msgid,
            only_visible_to_you: event.only_visible_to_you,
        });
    }

    fn reduce_notice(&mut self, event: &NoticeEvent) {
        self.append_chat(ChatLine {
            key: &event.conversation,
            display_target: display_target(&event.conversation, &event.target),
            author: &event.author,
            body: &event.body,
            timestamp: event.timestamp,
            server_time: event.server_time,
            kind: MessageKind::Notice,
            msgid: &event.msgid,
            only_visible_to_you: event.only_visible_to_you,
        });
    }

    fn reduce_action(&mut self, event: &ActionEvent) {
        self.append_chat(ChatLine {
            key: &event.conversation,
            display_target: display_target(&event.conversation, &event.target),
            author: &event.author,
            body: &event.body,
            timestamp: event.timestamp,
            server_time: event.server_time,
            kind: MessageKind::Action,
            msgid: &event.msgid,
            only_visible_to_you: event.only_visible_to_you,
        });
    }

    /// Records a JOIN, its account and realname, and a self-join's history anchor
    /// (`reduceJoin`).
    fn reduce_join(&mut self, event: &JoinEvent) {
        let network = event.network_id.as_str();
        let normalized = self.facts.normalize(network, &event.nick);
        if let Some(account) = &event.account {
            if !normalized.is_empty() {
                self.facts.set_presence_account(network, &normalized, account);
            }
        }
        if let Some(realname) = &event.realname {
            if !realname.is_empty() && !normalized.is_empty() {
                self.facts.set_presence_realname(network, &normalized, realname);
            }
        }

        let key = self.conversation_key(network, &event.channel);
        if !self.ensure_key(&key, &event.channel, IrcConversationCause::ChannelState) {
            return;
        }
        let self_join = self.facts.is_self(network, &event.nick);
        let shown = match &event.account {
            Some(_) => self.facts.display_account(network, &event.nick),
            None => String::new(),
        };
        let body = join_line(&event.nick, &shown);
        let Some(conv) = self.conversations.get_mut(&key) else {
            return;
        };
        let Some(channel) = conv.channel.as_mut() else {
            return;
        };
        let ranks = channel
            .members
            .get(&normalized)
            .map(|member| member.ranks)
            .unwrap_or_default();
        channel.members.insert(
            normalized,
            MemberState {
                display_nick: event.nick.clone(),
                ranks,
            },
        );
        if self_join {
            channel.joined = true;
            channel.history_anchor = Some(TranscriptAnchor {
                sequence: conv.trimmed as i64 + conv.messages.len() as i64,
            });
        }
        append_event(&self.facts, conv, &body, !self_join, true);
        if self_join {
            self.release_pending_playback(&key);
        }
    }

    /// Stores a WHO or WHOIS identity fact. A `None` field is left unchanged
    /// (`reduceNickFacts`).
    fn reduce_nick_facts(&mut self, event: &NickFactsEvent) {
        let network = event.network_id.as_str();
        let normalized = self.facts.normalize(network, &event.nick);
        if normalized.is_empty() {
            return;
        }
        if let Some(realname) = &event.realname {
            self.facts.set_presence_realname(network, &normalized, realname);
        }
        if let Some(operator) = event.server_operator {
            self.facts.set_presence_operator(network, &normalized, operator);
        }
    }

    /// Stores an account-notify change (`reduceAccount`).
    fn reduce_account(&mut self, event: &AccountEvent) {
        let network = event.network_id.as_str();
        let normalized = self.facts.normalize(network, &event.nick);
        if normalized.is_empty() {
            return;
        }
        self.facts.set_presence_account(network, &normalized, &event.account);
    }

    /// Records a PART. The departing nick's presence is forgotten when no shared
    /// channel remains (`reducePart`).
    fn reduce_part(&mut self, event: &PartEvent) {
        let network = event.network_id.as_str();
        let key = self.conversation_key(network, &event.channel);
        let self_part = self.facts.is_self(network, &event.nick);
        let normalized = self.facts.normalize(network, &event.nick);
        let mut departed = vec![normalized.clone()];
        {
            let Some(conv) = self.conversations.get_mut(&key) else {
                return;
            };
            let Some(channel) = conv.channel.as_mut() else {
                return;
            };
            channel.members.remove(&normalized);
            if self_part {
                channel.joined = false;
                channel.history_anchor = None;
                conv.history_page_cap_tail = false;
                departed.extend(channel.members.keys().cloned());
                channel.members.clear();
                stop_names_sync(channel);
            }
        }
        self.forget_unseen(network, &departed);
        let Some(conv) = self.conversations.get_mut(&key) else {
            return;
        };
        append_event(&self.facts, conv, &part_line(&event.nick), true, true);
        if self_part {
            conv.typing.clear();
        } else {
            conv.typing.remove(&normalized);
        }
    }

    /// Records a QUIT in every shared channel (`reduceQuit`).
    fn reduce_quit(&mut self, event: &QuitEvent) {
        let network = event.network_id.as_str();
        let normalized = self.facts.normalize(network, &event.nick);
        let body = quit_line(&event.nick);
        for conv in self.conversations.values_mut() {
            if conv.key.network_id != network {
                continue;
            }
            let Some(channel) = conv.channel.as_mut() else {
                continue;
            };
            if channel.members.remove(&normalized).is_none() {
                continue;
            }
            append_event(&self.facts, conv, &body, true, true);
        }
        self.forget_unseen(network, std::slice::from_ref(&normalized));
        self.clear_typing_everywhere(network, &normalized);
    }

    /// Moves membership, presence, typing and any direct conversation from the old
    /// nick to the new one (`reduceNick`).
    fn reduce_nick(&mut self, event: &NickEvent) {
        let network = event.network_id.as_str();
        let old_normalized = self.facts.normalize(network, &event.old_nick);
        let new_normalized = self.facts.normalize(network, &event.new_nick);
        if self.facts.is_self(network, &event.old_nick) {
            self.facts.remember_self_nick(network, &event.old_nick);
            self.facts
                .current_nicks
                .insert(network.to_owned(), event.new_nick.clone());
            self.refresh_mentions(network);
        }
        self.facts
            .rekey_presence(network, &old_normalized, &new_normalized);
        self.rekey_typing(network, &old_normalized, &new_normalized, &event.new_nick);

        let body = nick_line(&event.old_nick, &event.new_nick);
        for conv in self.conversations.values_mut() {
            if conv.key.network_id != network {
                continue;
            }
            let Some(channel) = conv.channel.as_mut() else {
                continue;
            };
            let Some(mut member) = channel.members.remove(&old_normalized) else {
                continue;
            };
            member.display_nick = event.new_nick.clone();
            channel.members.insert(new_normalized.clone(), member);
            append_event(&self.facts, conv, &body, true, true);
        }

        let old_key = ConversationKey::new(network, old_normalized);
        let new_key = ConversationKey::new(network, new_normalized);
        let Some(direct) = self.conversations.get_mut(&old_key) else {
            return;
        };
        if direct.is_channel() {
            return;
        }
        direct.target = event.new_nick.clone();
        append_event(&self.facts, direct, &body, true, true);
        if old_key == new_key {
            return;
        }

        let Some(mut moved) = self.conversations.remove(&old_key) else {
            return;
        };
        moved.key = new_key.clone();
        moved.target = event.new_nick.clone();
        if self.facts.muted_keys.remove(&old_key) {
            self.facts.muted_keys.insert(new_key.clone());
        }
        match self.conversations.get_mut(&new_key) {
            None => {
                self.conversations.insert(new_key.clone(), moved);
            }
            Some(existing) => {
                // Both sides may already hold the same msgid. Admitting a duplicate
                // would leave one id covering two messages.
                for mut message in std::mem::take(&mut moved.messages) {
                    if !message.msgid.is_empty() {
                        if existing.message_ids.contains(&message.msgid) {
                            continue;
                        }
                        existing.message_ids.insert(message.msgid.clone());
                    }
                    let old_sequence = message.sequence;
                    message.sequence = existing.next_sequence;
                    existing.next_sequence += 1;
                    if existing.unread_mark.is_none() && moved.unread_mark == Some(old_sequence) {
                        existing.unread_mark = Some(message.sequence);
                    }
                    existing.messages.push(message);
                }
                cap_messages(existing);
                existing.unread += moved.unread;
                existing.mentions += moved.mentions;
                existing.muted = existing.muted || moved.muted;
                for (nick, hint) in moved.typing {
                    existing.typing.insert(nick, hint);
                }
                existing.target = event.new_nick.clone();
            }
        }
        if self.facts.selected.as_ref() == Some(&old_key) {
            self.facts.selected = Some(new_key);
        }
    }

    /// Records a KICK. A kick of the local user raises an inbox arrival
    /// (`reduceKick`).
    fn reduce_kick(&mut self, event: &KickEvent) {
        let network = event.network_id.as_str();
        let key = self.conversation_key(network, &event.channel);
        let self_kick = self.facts.is_self(network, &event.target);
        let departed_normalized = self.facts.normalize(network, &event.target);
        let mut departed = vec![departed_normalized.clone()];
        {
            let Some(conv) = self.conversations.get_mut(&key) else {
                return;
            };
            let Some(channel) = conv.channel.as_mut() else {
                return;
            };
            channel.members.remove(&departed_normalized);
            if self_kick {
                channel.joined = false;
                channel.history_anchor = None;
                conv.history_page_cap_tail = false;
                departed.extend(channel.members.keys().cloned());
                channel.members.clear();
                stop_names_sync(channel);
            }
        }
        self.forget_unseen(network, &departed);
        let Some(conv) = self.conversations.get_mut(&key) else {
            return;
        };
        append_event(
            &self.facts,
            conv,
            &format!("{} was kicked", event.target),
            false,
            false,
        );
        if self_kick {
            conv.typing.clear();
            let preview = if event.reason.is_empty() {
                "You were kicked".to_owned()
            } else {
                event.reason.clone()
            };
            self.facts.inbox_arrival = Some(InboxArrival {
                kind: InboxKind::Kick,
                actor: event.author.clone(),
                body: preview,
                network_id: event.network_id.clone(),
                target: event.channel.clone(),
                msgid: MsgId::default(),
            });
        } else {
            conv.typing.remove(&departed_normalized);
        }
    }

    /// Records a channel topic (`reduceTopic`).
    fn reduce_topic(&mut self, event: &TopicEvent) {
        let key = self.conversation_key(&event.network_id, &event.channel);
        if !self.ensure_key(&key, &event.channel, IrcConversationCause::ChannelState) {
            return;
        }
        // RPL_TOPICWHOTIME (`333`) sets `keep_existing_topic` so inventing the
        // channel cannot clear a prior `332`/`TOPIC`. A real TOPIC with empty
        // text still clears (keep is false).
        if event.keep_existing_topic {
            return;
        }
        if let Some(channel) = self
            .conversations
            .get_mut(&key)
            .and_then(|conv| conv.channel.as_mut())
        {
            channel.topic = event.topic.clone();
        }
    }

    /// Merges a NAMES batch into membership, starting and finishing the sync
    /// (`reduceNames`).
    fn reduce_names(&mut self, event: &NamesEvent, now: DateTime<Utc>) {
        let network = event.network_id.as_str();
        let key = self.conversation_key(network, &event.channel);
        if !self.ensure_key(&key, &event.channel, IrcConversationCause::ChannelState) {
            return;
        }
        let names: Vec<(String, MemberState)> = event
            .names
            .iter()
            .map(|name| {
                (
                    self.facts.normalize(network, &name.nick),
                    MemberState {
                        display_nick: name.nick.clone(),
                        ranks: name.ranks,
                    },
                )
            })
            .collect();
        let Some(conv) = self.conversations.get_mut(&key) else {
            return;
        };
        let Some(channel) = conv.channel.as_mut() else {
            return;
        };
        if !channel.names_syncing {
            start_names_sync(channel, now);
        }
        for (normalized, member) in names {
            channel.members.insert(normalized, member);
        }
        if event.complete {
            stop_names_sync(channel);
        }
    }

    /// Records a channel MODE and applies prefix rank changes to existing members
    /// only (`reduceMode`).
    fn reduce_mode(&mut self, event: &ModeEvent) {
        let network = event.network_id.as_str();
        let key = self.conversation_key(network, &event.target);
        if !self.facts.is_channel(network, &key.normalized_target) {
            return;
        }
        if !self.ensure_key(&key, &event.target, IrcConversationCause::ChannelState) {
            return;
        }
        let changes = self
            .facts
            .features(network)
            .prefix_changes(&event.mode, &event.arguments);
        let Some(conv) = self.conversations.get_mut(&key) else {
            return;
        };
        append_event(
            &self.facts,
            conv,
            &format!("{} set mode {}", event.author, event.mode),
            false,
            false,
        );
        let Some(channel) = conv.channel.as_mut() else {
            return;
        };
        let features = self.facts.features(network);
        for change in changes {
            let normalized = self.facts.normalize(network, change.nick());
            let Some(member) = channel.members.get_mut(&normalized) else {
                continue;
            };
            member.ranks = features.apply(member.ranks, &change);
        }
    }

    /// Records an away-notify change. The fact is one per nick on the network,
    /// so every shared channel shows it (`reduceAway`).
    fn reduce_away(&mut self, event: &AwayEvent) {
        let network = event.network_id.as_str();
        let normalized = self.facts.normalize(network, &event.nick);
        if !event.realname_only {
            self.facts
                .set_presence_away(network, &normalized, event.away.clone());
        }
        if let Some(incoming) = &event.realname {
            // A placeholder must not erase a name a later WHOIS can match. An empty or
            // placeholder stored name still takes the incoming value.
            let stored = self.facts.nick_presence(network, &event.nick).realname;
            if presence::meaningful_realname(incoming, &event.nick)
                || !presence::meaningful_realname(&stored, &event.nick)
            {
                self.facts
                    .set_presence_realname(network, &normalized, incoming);
            }
        }
    }

    /// Records the local user's own away state, network-wide (`reduceSelfAway`).
    fn reduce_self_away(&mut self, event: &SelfAwayEvent) {
        if event.away {
            self.facts.self_away.insert(event.network_id.clone());
        } else {
            self.facts.self_away.remove(&event.network_id);
        }
    }

    /// Records one metadata value for a nick (`reduceMemberMetadata`).
    fn reduce_member_metadata(&mut self, event: &MemberMetadataEvent) {
        let network = event.network_id.as_str();
        let normalized = self.facts.normalize(network, &event.nick);
        self.facts
            .set_presence_metadata(network, &normalized, &event.key, &event.value);
    }

    /// Records or clears one typing hint (`reduceTyping`). A typing event never
    /// creates a conversation.
    fn reduce_typing(&mut self, event: &TypingEvent) {
        let network = event.conversation.network_id.clone();
        let normalized = self.facts.normalize(&network, &event.nick);
        let self_nick = self.facts.is_self(&network, &event.nick);
        let Some(conv) = self.conversations.get_mut(&event.conversation) else {
            return;
        };
        if self_nick {
            conv.typing.remove(&normalized);
            return;
        }
        let Some(hint) = stored_typing_hint(event.phase, event.received_at, &event.nick) else {
            conv.typing.remove(&normalized);
            return;
        };
        conv.typing
            .retain(|_, existing| typing_hint_retained(existing, event.received_at));
        conv.typing.insert(normalized, hint);
    }

    /// Appends a formatted WHOIS block to its conversation (`reduceWhoisTranscript`).
    fn reduce_whois_transcript(&mut self, event: &WhoisTranscriptEvent) {
        if let Some(conv) = self.conversations.get_mut(&event.destination) {
            append_whois(conv, &event.formatted_body);
        }
    }

    /// Appends a channel-scoped error line (`reduceChannelError`).
    fn reduce_channel_error(&mut self, event: &ChannelErrorEvent) {
        let key = self.conversation_key(&event.network_id, &event.channel);
        if let Some(conv) = self.conversations.get_mut(&key) {
            append_error(conv, &event.body);
        }
    }

    /// Drops one nick's typing hint on every conversation of a network
    /// (`clearTypingEverywhere`).
    fn clear_typing_everywhere(&mut self, network_id: &str, normalized_nick: &str) {
        for conv in self.conversations.values_mut() {
            if conv.key.network_id == network_id {
                conv.typing.remove(normalized_nick);
            }
        }
    }

    /// Moves a typing hint to a nick's new spelling (`rekeyTyping`).
    fn rekey_typing(
        &mut self,
        network_id: &str,
        old_normalized: &str,
        new_normalized: &str,
        new_display: &str,
    ) {
        for conv in self.conversations.values_mut() {
            if conv.key.network_id != network_id {
                continue;
            }
            let Some(mut hint) = conv.typing.remove(old_normalized) else {
                continue;
            };
            hint.display_nick = new_display.to_owned();
            conv.typing.insert(new_normalized.to_owned(), hint);
        }
    }

    /// Drops a nick's presence when it is no longer visible in any channel
    /// (`forgetUnseen`).
    fn forget_unseen(&mut self, network_id: &str, normalized_nicks: &[String]) {
        let Some(presence) = self.facts.presence.get_mut(network_id) else {
            return;
        };
        for nick in normalized_nicks {
            if presence.knows(nick) && !super::facts::is_visible(&self.conversations, network_id, nick) {
                presence.forget(nick);
            }
        }
    }
}
