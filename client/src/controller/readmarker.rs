//! Read-marker publishing when the transcript is caught up, and one MARKREAD get per
//! direct message.
//! (`syncReadMarkerForSelection` / `NoteTranscriptViewport`).

use std::collections::HashSet;

use chrono::{DateTime, Utc};

use crate::irc::event::ConversationKey;
use crate::irc::readmarker::read_marker_time_after;
use crate::irc::reducer::latest_server_message_time;
use crate::irc::viewnotify::ViewNotify;

use super::controller::Controller;

/// Tracks which direct messages already received a MARKREAD get this session.
pub(super) struct ReadMarkerState {
    get_sent: HashSet<ConversationKey>,
}

impl ReadMarkerState {
    pub(super) fn new() -> Self {
        Self {
            get_sent: HashSet::new(),
        }
    }

    pub(super) fn forget_network(&mut self, network_id: &str) {
        self.get_sent
            .retain(|key| key.network_id != network_id);
    }

    pub(super) fn forget_conversation(&mut self, key: &ConversationKey) {
        self.get_sent.remove(key);
    }

    fn note_get_sent(&mut self, key: &ConversationKey) -> bool {
        self.get_sent.insert(key.clone())
    }
}

impl Controller {
    /// Records whether the shell is focused and the transcript is pinned to the bottom
    /// (`NoteTranscriptViewport`).
    pub fn note_transcript_viewport(&mut self, focused: bool, follow_end: bool) {
        self.transcript_focused = focused;
        self.transcript_follow_end = follow_end;
        self.sync_read_marker_for_selection();
    }

    /// Applies a server read marker for one target (`ReadMarkerReceived`).
    pub fn read_marker_received(
        &mut self,
        network_id: &str,
        target: &str,
        marker: Option<DateTime<Utc>>,
    ) {
        if network_id.is_empty() || target.is_empty() {
            return;
        }
        let features = self.reducer.server_features(network_id);
        let key = self.reducer.conversation_key(network_id, target);
        if features.is_channel(target) {
            if self.reducer.find(&key).is_none() {
                return;
            }
        } else if marker.is_none() {
            return;
        }
        if self.reducer.apply_server_read_marker(&key, marker) {
            self.queue(vec![ViewNotify::Conversations]);
        }
    }

    pub(super) fn request_read_marker_get_once(&mut self, network_id: &str, target: &str) {
        if target.is_empty() {
            return;
        }
        let features = self.reducer.server_features(network_id);
        if features.is_channel(target) {
            return;
        }
        let key = self.reducer.conversation_key(network_id, target);
        if self.read_markers.get_sent.contains(&key) {
            return;
        }
        let Some(session) = self.manager.find(network_id) else {
            return;
        };
        let ready = session.locked(|core| !core.read_marker_command().is_empty());
        if !ready {
            return;
        }
        self.read_markers.note_get_sent(&key);
        session.request_read_marker_get(target);
    }

    pub(super) fn read_marker_forget_network(&mut self, network_id: &str) {
        self.read_markers.forget_network(network_id);
    }

    pub(super) fn sync_read_marker_for_selection(&mut self) {
        if self.selected.is_none()
            || self.console_open
            || !self.transcript_follow_end
            || !self.transcript_focused
            || !self.reducer.window_active()
        {
            return;
        }
        let key = self.selected.clone().expect("selected");
        let Some(session) = self.manager.find(&key.network_id) else {
            return;
        };
        let Some(conversation) = self.reducer.find(&key) else {
            return;
        };
        let Some(when) = latest_server_message_time(conversation) else {
            return;
        };
        if conversation.read_marker.is_some_and(|marker| !read_marker_time_after(when, marker)) {
            return;
        }
        let target = if conversation.target.is_empty() {
            self.selected_target.clone()
        } else {
            conversation.target.clone()
        };
        session.queue_read_marker_set(&target, when);
    }
}
