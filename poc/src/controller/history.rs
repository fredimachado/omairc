//! History batch replay and self-JOIN side effects (`handle_history_batch`,
//! `on_self_join`).

use crate::irc::event::{Event, HistoryKind};
use crate::irc::translator::{translate_history, HistoryBatch};
use crate::irc::viewnotify::ViewNotify;
use crate::session::events::SessionEvent;

use super::controller::Controller;

impl Controller {
    /// Folds session events that carry transcript history or chathistory playback.
    pub(super) fn on_history_event(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::HistoryBatchReceived { network_id, batch } => {
                self.handle_history_batch(&network_id, &batch);
            }
            SessionEvent::ChatHistoryRequestFinished { .. }
            | SessionEvent::ChatHistoryFailed { .. } => {}
            _ => {}
        }
    }

    /// Our own JOIN consumes the channel's invite row, and selects the channel when
    /// it answers the session's pending invite.
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
    }

    /// Replays one history batch. A TARGETS answer discovers directs, which the chathistory
    /// port (Phase 11) turns into queries, so it splices nothing here.
    fn handle_history_batch(&mut self, network_id: &str, batch: &HistoryBatch) {
        if batch.kind == Some(HistoryKind::Targets) {
            return;
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
        if let Some(event) = event {
            self.apply(Event::History(event));
        }
    }
}
