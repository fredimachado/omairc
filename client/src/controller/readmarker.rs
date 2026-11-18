//! Read-marker publishing when the transcript is caught up, and one MARKREAD get per
//! direct message.
//! (`publishReadMarker` / `NoteTranscriptViewport`).

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::irc::event::ConversationKey;
use crate::irc::readmarker::read_marker_time_after;
use crate::irc::reducer::latest_server_message_time;
use crate::irc::viewnotify::ViewNotify;

use super::controller::Controller;

/// One GET phase per conversation. Absent from the map means `NotSent`. Only
/// `ReadMarkerState::note_get_sent` stores `Awaiting`, after the session accepted
/// the GET, so awaiting a GET that was never sent cannot be represented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GetPhase {
    NotSent,
    Awaiting,
    /// The GET was sent and its reply or a FAIL arrived. Keeps the "do not GET
    /// again" role until the conversation or network is forgotten.
    Released,
}

/// The GET phase of each direct message this controller session.
pub(super) struct ReadMarkerState {
    gets: HashMap<ConversationKey, GetPhase>,
}

/// What the shell reports about the transcript. Where `follow_end` comes from
/// may change; this type does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ReadMarkerViewport {
    pub focused: bool,
    pub follow_end: bool,
}

impl ReadMarkerViewport {
    /// Whether the viewport is focused and caught up to the end.
    pub(super) fn open(self) -> bool {
        self.focused && self.follow_end
    }
}

impl ReadMarkerState {
    pub(super) fn new() -> Self {
        Self {
            gets: HashMap::new(),
        }
    }

    pub(super) fn forget_network(&mut self, network_id: &str) {
        self.gets.retain(|key, _| key.network_id != network_id);
    }

    pub(super) fn forget_conversation(&mut self, key: &ConversationKey) {
        self.gets.remove(key);
    }

    pub(super) fn phase(&self, key: &ConversationKey) -> GetPhase {
        self.gets.get(key).copied().unwrap_or(GetPhase::NotSent)
    }

    /// The only constructor of `Awaiting`; call it after the session accepted the GET.
    fn note_get_sent(&mut self, key: &ConversationKey) {
        self.gets.insert(key.clone(), GetPhase::Awaiting);
    }

    /// Moves an awaiting GET to `Released`; true iff it was awaiting.
    fn release_get(&mut self, key: &ConversationKey) -> bool {
        match self.gets.get_mut(key) {
            Some(phase @ GetPhase::Awaiting) => {
                *phase = GetPhase::Released;
                true
            }
            _ => false,
        }
    }

    /// Releases every awaiting GET on the network (FAIL without a target); true
    /// iff any was awaiting.
    fn release_network(&mut self, network_id: &str) -> bool {
        let mut released = false;
        for (key, phase) in &mut self.gets {
            if key.network_id == network_id && *phase == GetPhase::Awaiting {
                *phase = GetPhase::Released;
                released = true;
            }
        }
        released
    }

    fn awaiting_keys(&self, network_id: &str) -> Vec<ConversationKey> {
        self.gets
            .iter()
            .filter(|(key, phase)| key.network_id == network_id && **phase == GetPhase::Awaiting)
            .map(|(key, _)| key.clone())
            .collect()
    }
}

impl Controller {
    /// Records whether the shell is focused and the selected transcript is pinned to the bottom
    /// (`NoteTranscriptViewport`).
    pub fn note_transcript_viewport(&mut self, focused: bool, follow_end: bool) {
        self.viewport = ReadMarkerViewport {
            focused,
            follow_end,
        };
        self.publish_read_marker();
    }

    /// The bottom-pin bit the shell last reported, which gates the read marker.
    #[cfg(test)]
    pub fn transcript_follow_end(&self) -> bool {
        self.viewport.follow_end
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
            if self.read_markers.release_network(network_id) {
                self.publish_read_marker();
            }
            return;
        }
        let features = self.reducer.server_features(network_id);
        let key = self.reducer.conversation_key(network_id, target);
        let mut awaited = self.read_markers.release_get(&key);
        // Welcome may record the hold under the selected key before 005 arrives,
        // when conversation_key still uses the default casemapping.
        if !awaited {
            if let Some(selected) = self.selected.clone() {
                if selected.network_id == network_id
                    && features
                        .case_mapping()
                        .equals(&selected.normalized_target, target)
                {
                    awaited = self.read_markers.release_get(&selected);
                }
            }
        }
        if !awaited {
            for pending in self.read_markers.awaiting_keys(network_id) {
                if features
                    .case_mapping()
                    .equals(&pending.normalized_target, target)
                    && self.read_markers.release_get(&pending)
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
                self.publish_read_marker();
            }
            return;
        }
        if self.reducer.apply_server_read_marker(&key, marker) {
            self.queue(vec![ViewNotify::Conversations]);
        }
        if awaited {
            self.publish_read_marker();
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
        if self.read_markers.phase(key) != GetPhase::NotSent {
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
        if self.read_markers.release_network(network_id) {
            self.publish_read_marker();
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

    /// The only caller that asks a session to send a read marker SET. Inputs: the
    /// selection, the GET phase, the viewport the shell reported, and the
    /// reducer's latest message time; the session's `SetSlot` decides in-flight
    /// dedupe. `bin/check-conventions` enforces the single caller.
    pub(super) fn publish_read_marker(&mut self) {
        if self.selected.is_none()
            || self.console_open
            || !self.viewport.open()
            || !self.reducer.window_active()
        {
            return;
        }
        let key = self.selected.clone().expect("selected");
        if self.read_markers.phase(&key) == GetPhase::Awaiting {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn key(network: &str, target: &str) -> ConversationKey {
        ConversationKey {
            network_id: network.to_owned(),
            normalized_target: target.to_owned(),
        }
    }

    #[test]
    fn get_phase_transitions() {
        let mut state = ReadMarkerState::new();
        let alice = key("net", "alice");
        let bob = key("net", "bob");
        let other = key("other", "alice");
        assert_eq!(state.phase(&alice), GetPhase::NotSent);
        assert!(!state.release_get(&alice), "a GET never sent cannot release");
        assert_eq!(state.phase(&alice), GetPhase::NotSent);
        for key in [&alice, &bob, &other] {
            state.note_get_sent(key);
        }
        assert_eq!(state.phase(&alice), GetPhase::Awaiting);
        assert!(state.release_get(&alice));
        assert_eq!(state.phase(&alice), GetPhase::Released);
        assert!(!state.release_get(&alice), "a Released GET is not awaiting");
        assert!(state.release_network("net"));
        assert_eq!(state.phase(&bob), GetPhase::Released, "no re-GET after release");
        assert_eq!(state.phase(&other), GetPhase::Awaiting);
        state.forget_conversation(&alice);
        assert_eq!(state.phase(&alice), GetPhase::NotSent);
        state.forget_network("other");
        assert_eq!(state.phase(&other), GetPhase::NotSent);
    }

    #[test]
    fn viewport_opens_only_when_focused_and_following() {
        let open = |focused, follow_end| ReadMarkerViewport { focused, follow_end }.open();
        assert!(open(true, true));
        assert!(!open(true, false));
        assert!(!open(false, true));
        assert!(!open(false, false));
    }
}
