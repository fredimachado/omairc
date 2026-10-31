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

/// Tracks which direct messages already received a MARKREAD get this session,
/// and which still await that get's reply before a set may publish.
pub(super) struct ReadMarkerState {
    get_sent: HashSet<ConversationKey>,
    awaiting_get: HashSet<ConversationKey>,
}

impl ReadMarkerState {
    pub(super) fn new() -> Self {
        Self {
            get_sent: HashSet::new(),
            awaiting_get: HashSet::new(),
        }
    }

    pub(super) fn forget_network(&mut self, network_id: &str) {
        self.get_sent
            .retain(|key| key.network_id != network_id);
        self.awaiting_get
            .retain(|key| key.network_id != network_id);
    }

    pub(super) fn forget_conversation(&mut self, key: &ConversationKey) {
        self.get_sent.remove(key);
        self.awaiting_get.remove(key);
    }

    fn note_get_sent(&mut self, key: &ConversationKey) {
        self.get_sent.insert(key.clone());
        self.awaiting_get.insert(key.clone());
    }

    fn clear_awaiting_get(&mut self, key: &ConversationKey) -> bool {
        self.awaiting_get.remove(key)
    }

    fn clear_awaiting_network(&mut self, network_id: &str) -> bool {
        let before = self.awaiting_get.len();
        self.awaiting_get
            .retain(|key| key.network_id != network_id);
        self.awaiting_get.len() != before
    }

    fn awaits_get(&self, key: &ConversationKey) -> bool {
        self.awaiting_get.contains(key)
    }
}

impl Controller {
    /// Records whether the shell is focused and the selected transcript is pinned to the bottom
    /// (`NoteTranscriptViewport`). Unchanged values are ignored so a redundant report after a
    /// set FAIL cannot immediately resend the same stamp.
    pub fn note_transcript_viewport(&mut self, focused: bool, follow_end: bool) {
        if self.transcript_focused == focused && self.transcript_follow_end == follow_end {
            return;
        }
        self.transcript_focused = focused;
        self.transcript_follow_end = follow_end;
        self.sync_read_marker_for_selection();
    }

    /// Drops get dedupe for `network_id` and, when a direct message on that network is
    /// selected, keeps `awaiting_get` set so a viewport sync after 001 cannot publish
    /// before Welcome's get request.
    pub(super) fn arm_read_marker_reconnect_hold(&mut self, network_id: &str) {
        if network_id.is_empty() {
            return;
        }
        self.read_markers
            .get_sent
            .retain(|key| key.network_id != network_id);
        let Some(selected) = self.selected.clone() else {
            return;
        };
        if selected.network_id != network_id {
            return;
        }
        let features = self.reducer.server_features(network_id);
        if features.is_channel(&selected.normalized_target) {
            return;
        }
        self.read_markers.awaiting_get.insert(selected);
    }

    /// Applies a server read marker for one target (`ReadMarkerReceived`). An empty
    /// `target` clears every awaiting get on the network (FAIL without a target).
    pub fn read_marker_received(
        &mut self,
        network_id: &str,
        target: &str,
        marker: Option<DateTime<Utc>>,
    ) {
        if network_id.is_empty() {
            return;
        }
        if target.is_empty() {
            if self.read_markers.clear_awaiting_network(network_id) {
                self.sync_read_marker_for_selection();
            }
            return;
        }
        let features = self.reducer.server_features(network_id);
        let key = self.reducer.conversation_key(network_id, target);
        let mut awaited = self.read_markers.clear_awaiting_get(&key);
        // Welcome may record the hold under the selected key before 005 arrives,
        // when conversation_key still uses the default casemapping.
        if !awaited {
            if let Some(selected) = self.selected.clone() {
                if selected.network_id == network_id
                    && features
                        .case_mapping()
                        .equals(&selected.normalized_target, target)
                {
                    awaited = self.read_markers.clear_awaiting_get(&selected);
                }
            }
        }
        if !awaited {
            let pending: Vec<ConversationKey> = self
                .read_markers
                .awaiting_get
                .iter()
                .filter(|pending| pending.network_id == network_id)
                .cloned()
                .collect();
            for pending in pending {
                if features
                    .case_mapping()
                    .equals(&pending.normalized_target, target)
                    && self.read_markers.clear_awaiting_get(&pending)
                {
                    awaited = true;
                    break;
                }
            }
        }
        if features.is_channel(target) {
            if self.reducer.find(&key).is_none() {
                return;
            }
        } else if marker.is_none() {
            if awaited {
                self.sync_read_marker_for_selection();
            }
            return;
        }
        if self.reducer.apply_server_read_marker(&key, marker) {
            self.queue(vec![ViewNotify::Conversations]);
        }
        if awaited {
            self.sync_read_marker_for_selection();
        }
    }

    /// Asks for one MARKREAD get, recording the hold under `key` (the selection's
    /// conversation identity) so pre-005 casemapping cannot miss it.
    pub(super) fn request_read_marker_get_with_key(
        &mut self,
        key: &ConversationKey,
        target: &str,
    ) {
        if target.is_empty() {
            return;
        }
        let features = self.reducer.server_features(&key.network_id);
        if features.is_channel(target) {
            return;
        }
        if self.read_markers.get_sent.contains(key) {
            return;
        }
        let Some(session) = self.manager.find(&key.network_id) else {
            return;
        };
        if !session.request_read_marker_get(target) {
            return;
        }
        self.read_markers.note_get_sent(key);
    }

    pub(super) fn read_marker_forget_network(&mut self, network_id: &str) {
        self.read_markers.forget_network(network_id);
    }

    pub(super) fn read_marker_clear_awaiting_network(&mut self, network_id: &str) {
        if self.read_markers.clear_awaiting_network(network_id) {
            self.sync_read_marker_for_selection();
        }
    }

    pub(super) fn request_read_marker_get_for_selected_direct(&mut self, network_id: &str) {
        let Some(selected) = self.selected.clone() else {
            return;
        };
        if selected.network_id != network_id {
            return;
        }
        let features = self.reducer.server_features(network_id);
        if features.is_channel(&selected.normalized_target) {
            return;
        }
        let Some(conversation) = self.reducer.find(&selected) else {
            return;
        };
        let target = if conversation.target.is_empty() {
            self.selected_target.clone()
        } else {
            conversation.target.clone()
        };
        self.request_read_marker_get_with_key(&selected, &target);
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
        if self.read_markers.awaits_get(&key) {
            return;
        }
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
