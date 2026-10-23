//! History batch replay, catch-up wiring, and older-page requests.

use chrono::{DateTime, Utc};

use crate::irc::event::{Event, HistoryKind};
use crate::irc::translator::{translate_history, HistoryBatch};
use crate::irc::viewnotify::ViewNotify;
use crate::irc::world::Message;
use crate::session::events::{SessionEvent, SessionState};

use super::controller::Controller;

impl Controller {
    /// Folds session events that carry transcript history or chathistory playback.
    pub(super) fn on_history_event(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::HistoryBatchReceived { network_id, batch } => {
                self.handle_history_batch(&network_id, &batch);
            }
            SessionEvent::ChatHistoryRequestFinished {
                network_id,
                target,
                failed,
                before,
            } => {
                if failed && before {
                    let key = self.reducer.conversation_key(&network_id, &target);
                    self.reducer.clear_history_page_cap_tail(&key);
                }
            }
            SessionEvent::ChatHistoryFailed {
                network_id,
                subcommand,
                target,
            } => self.on_chat_history_failed(&network_id, subcommand, &target),
            _ => {}
        }
    }

    /// Our own JOIN consumes the channel's invite row, requests channel playback,
    /// and selects the channel when it answers the session's pending invite.
    pub(super) fn on_self_join(&mut self, network_id: &str, channel: &str) {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.inbox.consume_invite(network_id, channel, mapping);
        self.queue(vec![ViewNotify::Inbox]);
        let Some(session) = self.manager.find(network_id) else {
            return;
        };
        if let Some(pending) = session.pending_invite() {
            if mapping.equals(channel, &pending.channel) {
                self.select(network_id, channel);
            }
        }
        self.playback.borrow_mut().note_joined_channel(network_id, channel);
        self.request_channel_playback(&session, channel);
    }

    /// Asks the focused network for one older CHATHISTORY page before the oldest row.
    pub fn request_older_transcript_history(&mut self) -> bool {
        if self.shows_status() {
            return false;
        }
        let key = match self.selected.clone() {
            Some(key) => key,
            None => return false,
        };
        let conversation = match self.reducer.find(&key) {
            Some(conversation) => conversation,
            None => return false,
        };
        if conversation.messages.is_empty() {
            return false;
        }
        if conversation.is_channel() {
            let joined = conversation
                .channel
                .as_ref()
                .is_some_and(|channel| channel.joined);
            if !joined {
                return false;
            }
        }
        let session = match self.manager.find(&key.network_id) {
            Some(session) => session,
            None => return false,
        };
        if session.state() != SessionState::Registered || !self.chat_history_cap(&key.network_id) {
            return false;
        }
        let (msgid, timestamp) = match oldest_history_cursor(&conversation.messages) {
            Some(cursor) => cursor,
            None => return false,
        };
        let target = if conversation.target.is_empty() {
            key.normalized_target.clone()
        } else {
            conversation.target.clone()
        };
        self.reducer.mark_history_page_cap_tail(&key);
        if !session.request_older_history(&target, &msgid, timestamp) {
            if !session.history_inflight_for(&target) {
                self.reducer.clear_history_page_cap_tail(&key);
            }
            return false;
        }
        true
    }

    /// Whether a CHATHISTORY page is already in flight for the selected conversation.
    pub fn older_transcript_history_inflight(&self) -> bool {
        let key = match self.selected.as_ref() {
            Some(key) => key,
            None => return false,
        };
        let conversation = match self.reducer.find(key) {
            Some(conversation) => conversation,
            None => return false,
        };
        let session = match self.manager.find(&key.network_id) {
            Some(session) => session,
            None => return false,
        };
        let target = if conversation.target.is_empty() {
            key.normalized_target.as_str()
        } else {
            conversation.target.as_str()
        };
        session.history_inflight_for(target)
    }

    /// Whether the focused network negotiated batch and chathistory.
    pub fn chat_history_enabled(&self) -> bool {
        let network_id = self.focused_network_id();
        self.chat_history_cap(network_id)
    }

    /// Drops tail-first trimming when the reader returns to the live end.
    pub fn clear_history_page_cap_tail(&mut self) {
        if let Some(key) = self.selected.clone() {
            self.reducer.clear_history_page_cap_tail(&key);
        }
    }

    fn on_chat_history_failed(&mut self, network_id: &str, subcommand: &str, target: &str) {
        if subcommand.eq_ignore_ascii_case("TARGETS") {
            if !self.playback.borrow_mut().retry_targets(network_id) {
                return;
            }
            if let Some(session) = self.manager.find(network_id) {
                self.request_chat_history_catch_up(&session);
            }
            return;
        }
        if subcommand.eq_ignore_ascii_case("AFTER") {
            self.drop_empty_discovered_direct(network_id, target);
        }
    }

    fn drop_empty_discovered_direct(&mut self, network_id: &str, target: &str) {
        if !self
            .playback
            .borrow()
            .was_discovered(&self.reducer, network_id, target)
        {
            return;
        }
        let mapping = self.reducer.server_features(network_id).case_mapping();
        for nick in self.open_directs.listed(network_id, mapping) {
            if mapping.equals(&nick, target) {
                return;
            }
        }
        let key = self.reducer.conversation_key(network_id, target);
        let conversation = self.reducer.find(&key);
        if conversation.is_none() || !conversation.unwrap().messages.is_empty() {
            return;
        }
        self.reducer.drop_direct_message(&key);
        self.queue(vec![ViewNotify::Conversations, ViewNotify::Messages]);
    }

    fn handle_history_batch(&mut self, network_id: &str, batch: &HistoryBatch) {
        if batch.kind == Some(HistoryKind::Targets) {
            let session = self.manager.find(network_id);
            let features = self.reducer.server_features(network_id);
            let mapping = features.case_mapping();
            let created = if let Some(session) = session.as_ref() {
                let nick = self.current_nick(network_id);
                let restored = self.open_directs.listed(network_id, mapping);
                let dismissed = self.open_directs.dismissed_listed(network_id, mapping);
                self.playback.borrow_mut().note_discovered_targets(
                    session,
                    &mut self.reducer,
                    &batch.targets,
                    &nick,
                    &restored,
                    &dismissed,
                )
            } else {
                0
            };
            if let Some(session) = session {
                self.playback.borrow_mut().note_targets_page(
                    &session,
                    &batch.targets,
                    batch.history_ended,
                    session.history_limit(),
                );
            }
            if created > 0 {
                self.queue(vec![ViewNotify::Conversations, ViewNotify::Messages]);
            }
            return;
        }
        if batch.after_request && batch.lines.is_empty() {
            self.drop_empty_discovered_direct(network_id, &batch.target);
        }
        let features = self.reducer.server_features(network_id).clone();
        let current_nick = self.current_nick(network_id);
        let now = self.clock.now();
        let event = {
            let reducer = &self.reducer;
            let joined = |channel: &str, sender: &str| {
                reducer.joined_member_channel(network_id, channel, sender)
            };
            translate_history(
                network_id,
                &current_nick,
                &features,
                batch,
                now,
                Some(&joined),
            )
        };
        if let Some(mut event) = event {
            if event.kind == HistoryKind::BouncerPlayback {
                self.playback
                    .borrow()
                    .trim_bouncer_batch(network_id, &self.reducer, &mut event);
                if event.lines.is_empty() {
                    return;
                }
            }
            self.apply(Event::History(event));
        }
    }
}

pub(crate) fn oldest_history_cursor(messages: &[Message]) -> Option<(String, Option<DateTime<Utc>>)> {
    for message in messages {
        if !message.msgid.value.is_empty() {
            return Some((message.msgid.value.clone(), Some(message.time)));
        }
    }
    for message in messages {
        if message.time != DateTime::<Utc>::MIN_UTC {
            return Some((String::new(), Some(message.time)));
        }
    }
    None
}
