//! The selection intents: which conversation or Status the transcript shows, the
//! direct messages the user opens and closes, and the read state of the selected
//! conversation.
//! `CloseDirectMessage`, `DropConversationAndReselect`, `ClearConversationSelection`
//! and `CanCloseSelection` in `tui/internal/controller/controller.go`.

use crate::irc::command::ComposerSurface;
use crate::irc::conversation::neighbor_after_drop;
use crate::irc::event::{ConversationKey, IrcConversationCause};
use crate::irc::viewnotify::{self, ViewNotify};

use super::controller::Controller;

/// The surfaces a selection change dirties: the sidebar, the transcript, the
/// member panel and the header.
fn selection_surfaces() -> Vec<ViewNotify> {
    vec![
        ViewNotify::Conversations,
        ViewNotify::Messages,
        ViewNotify::MemberReset,
        ViewNotify::Selection,
    ]
}

impl Controller {
    /// Opens or creates a conversation and selects it, clearing any monitor row for a
    /// direct message (`RevealConversation`).
    pub fn reveal_conversation(&mut self, network_id: &str, target: &str) {
        self.pump();
        if network_id.is_empty() || target.is_empty() {
            return;
        }
        let key = self.reducer.conversation_key(network_id, target);
        if self.reducer.find(&key).is_none() {
            if self
                .reducer
                .ensure_conversation(&key, target, IrcConversationCause::UserOpen)
                .is_none()
            {
                return;
            }
            self.remember_open_direct(network_id, target);
            self.queue(vec![ViewNotify::Conversations]);
        }
        let features = self.reducer.server_features(network_id);
        if !features.is_channel(&key.normalized_target) {
            self.inbox.consume_monitor(network_id, target, features.case_mapping());
            self.queue(vec![ViewNotify::Inbox]);
        }
        self.select(network_id, target);
        self.pump();
    }

    /// Selects a conversation and leaves Status (`SelectConversation`). The
    /// conversation's inbox rows clear, its read marker is requested for a direct
    /// message, and the transcript follows it. An empty network or target is a no-op.
    pub fn select_conversation(&mut self, network_id: &str, target: &str) {
        self.pump();
        self.select(network_id, target);
        self.pump();
    }

    /// Selects `network_id`'s Status console (`OpenStatus`). The transcript shows the
    /// network's Status lines until a conversation is selected again.
    pub fn open_status(&mut self, network_id: &str) {
        self.pump();
        if network_id.is_empty() {
            return;
        }
        self.console_network = network_id.to_owned();
        self.console_open = true;
        self.console.mark_seen(network_id);
        self.queue(vec![ViewNotify::Selection, ViewNotify::Status]);
        self.pump();
    }

    /// Leaves Status for the selected conversation (`CloseStatus`). The selection is kept
    /// so the transcript shows it again. Nothing else changes.
    pub fn leave_status(&mut self) {
        self.pump();
        if !self.console_open {
            return;
        }
        self.console_open = false;
        self.queue(vec![
            ViewNotify::Selection,
            ViewNotify::Messages,
            ViewNotify::Status,
        ]);
        self.pump();
    }

    /// Clears the selection, so the transcript falls back to Status
    /// (`ClearConversationSelection`).
    #[cfg(test)]
    pub fn clear_selection(&mut self) {
        self.pump();
        self.withdraw_typing();
        self.clear_selected();
        self.pump();
    }

    /// Opens a direct message with `nick` on the selected network, creates the
    /// conversation if needed, and selects it (`OpenDirectMessage`). False when no
    /// conversation is selected to name the network.
    pub fn open_direct(&mut self, nick: &str) -> bool {
        self.pump();
        let Some(network_id) = self.selected.as_ref().map(|key| key.network_id.clone()) else {
            return false;
        };
        if nick.is_empty() {
            return false;
        }
        let key = self.reducer.conversation_key(&network_id, nick);
        if self
            .reducer
            .ensure_conversation(&key, nick, IrcConversationCause::UserOpen)
            .is_none()
        {
            return false;
        }
        self.remember_open_direct(&network_id, nick);
        self.queue(vec![ViewNotify::Conversations]);
        self.select(&network_id, nick);
        self.pump();
        true
    }

    /// Closes what the user has selected (`CloseDirectMessage`). A direct message
    /// closes; a channel closes only once it is left. A closed direct message
    /// selects its neighbour. False when the selection cannot close.
    pub fn close_direct(&mut self) -> bool {
        self.pump();
        let network_id = self.focused_network_id().to_owned();
        if !self.close_selected() {
            return false;
        }
        if self.last_error_for(&network_id).is_some() {
            self.set_last_error(&network_id, None);
            self.queue(vec![ViewNotify::Status]);
        }
        self.pump();
        true
    }

    /// Closes the named conversation, selected or not (`CloseConversation`). The
    /// middle-click and Ctrl+W paths use it. A direct message closes; a channel only
    /// once it is left. Closing a row that is not selected leaves the selection where
    /// it is. False when the conversation is unknown or cannot close.
    pub fn close_conversation(&mut self, network_id: &str, target: &str) -> bool {
        self.pump();
        if network_id.is_empty() || target.is_empty() {
            return false;
        }
        let key = self.reducer.conversation_key(network_id, target);
        if self.reducer.find(&key).is_none() {
            return false;
        }
        let closed = self.close_key(key);
        self.pump();
        closed
    }

    /// Whether the selection can close: a direct message always can, a channel only
    /// once it is no longer joined (`CanCloseSelection`).
    pub fn can_close_selection(&self) -> bool {
        let Some(key) = self.selected.as_ref() else {
            return false;
        };
        match self.reducer.find(key) {
            None => false,
            Some(conversation) if !conversation.is_channel() => true,
            Some(conversation) => conversation
                .channel
                .as_ref()
                .is_some_and(|channel| !channel.joined),
        }
    }

    /// Whether the selected channel is joined (`ChannelJoined`). False for a direct
    /// message or an empty selection.
    pub fn channel_joined(&self) -> bool {
        let Some(key) = self.selected.as_ref() else {
            return false;
        };
        self.reducer
            .find(key)
            .and_then(|conversation| conversation.channel.as_ref())
            .is_some_and(|channel| channel.joined)
    }

    /// Parts the selected channel (`leaveSelectedChannel`). The header's LEAVE takes this
    /// path, and so does a typed `/part`: the line goes through `submit`, the one path every
    /// outbound verb takes. False when the part was refused.
    pub fn leave_selected_channel(&mut self) -> bool {
        self.submit(ComposerSurface::Conversation, "/part")
    }

    /// Joins the selected channel again (`joinSelectedChannel`). It does nothing when the
    /// selection is not a channel or the channel is still joined. The JOIN goes through
    /// `submit`, as a typed `/join` does.
    pub fn join_selected_channel(&mut self) -> bool {
        if !self.is_channel() || self.channel_joined() {
            return false;
        }
        let line = format!("/join {}", self.selected_target());
        self.submit(ComposerSurface::Conversation, &line)
    }

    /// Clears every unread and mention count (`MarkAllRead`).
    pub fn mark_all_read(&mut self) {
        self.pump();
        if self.reducer.mark_all_read() {
            self.queue(vec![ViewNotify::Conversations]);
        }
    }

    /// Records whether the window has focus (`SetWindowActive`). Gaining focus on a
    /// selection reads it.
    pub fn set_window_active(&mut self, active: bool) {
        self.pump();
        self.reducer.set_window_active(active);
        if !active {
            self.transcript_focused = false;
            return;
        }
        self.transcript_focused = true;
        self.transcript_follow_end = false;
        let Some(key) = self.selected.clone() else {
            return;
        };
        if self.reducer.mark_read(&key) {
            self.queue(vec![ViewNotify::Conversations]);
        }
    }

    /// The selection as the reducer keeps it, applied without pumping. `select_conversation`
    /// and the join path both come here. Selecting a row clears its network's last
    /// command error, so the header shows the topic again (the PoC's selection rule).
    pub(super) fn select(&mut self, network_id: &str, target: &str) {
        if network_id.is_empty() || target.is_empty() {
            return;
        }
        self.set_last_error(network_id, None);
        self.console_network = network_id.to_owned();
        self.console_open = false;
        let previous = self.selected.clone();
        let key = self.reducer.conversation_key(network_id, target);
        if self.selected.as_ref() != Some(&key) {
            if let Some(previous) = previous {
                if previous != key {
                    self.reducer.clear_history_page_cap_tail(&previous);
                }
            }
            self.withdraw_typing();
        }
        self.selected = Some(key.clone());
        self.selected_target = target.to_owned();
        let features = self.reducer.server_features(network_id);
        let mapping = features.case_mapping();
        let is_channel = features.is_channel(&key.normalized_target);
        self.inbox.consume_conversation(network_id, target, mapping);
        if !is_channel {
            self.inbox.consume_monitor(network_id, target, mapping);
        }
        self.reducer.mark_selected(&key);
        let mut surfaces = selection_surfaces();
        viewnotify::merge(&mut surfaces, vec![ViewNotify::Inbox, ViewNotify::Status]);
        self.queue(surfaces);
        if self.reducer.find(&key).is_some() && !is_channel {
            self.request_read_marker_get_once(network_id, target);
        }
    }

    /// Drops the selection without pumping (`ClearConversationSelection`).
    fn clear_selected(&mut self) {
        self.selected = None;
        self.selected_target.clear();
        self.reducer.clear_selection();
        self.queue(selection_surfaces());
    }

    /// Closes the selection (`closeSelectedConversation`). A channel closes only when
    /// it is not joined, and then dismisses the channel. A direct message or a
    /// conversation that does not exist yet drops and reselects. `/close` calls it.
    pub(super) fn close_selected(&mut self) -> bool {
        let Some(key) = self.selected.clone() else {
            return false;
        };
        self.close_key(key)
    }

    /// Closes one conversation row (`closeConversation`). The rules are the
    /// selection's: a channel closes only once it is left, and a direct message
    /// drops and reselects when its row was selected.
    fn close_key(&mut self, key: ConversationKey) -> bool {
        let channel = self
            .reducer
            .find(&key)
            .filter(|conversation| conversation.is_channel())
            .map(|conversation| {
                (
                    conversation.target.clone(),
                    conversation.channel.as_ref().map(|state| state.joined),
                )
            });
        if let Some((display, joined)) = channel {
            if joined != Some(false) {
                return false;
            }
            let display = if display.is_empty() {
                key.normalized_target.clone()
            } else {
                display
            };
            // A JOIN still in flight must not reopen the channel the user just closed.
            self.cancelled_joins.insert(key.clone());
            self.reducer.note_closed(&key);
            return self.dismiss_channel(&key.network_id, &display);
        }
        // A closed direct message is not reopened at the next launch, and its messages do not
        // bring it back (`forgetOpenDirect`).
        let display = self
            .reducer
            .find(&key)
            .map(|conversation| conversation.target.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| key.normalized_target.clone());
        self.forget_open_direct(&key.network_id, &display);
        self.drop_and_reselect(key);
        true
    }

    /// Dismisses a channel row, by its name (`dismissChannel`).
    pub(super) fn dismiss_channel(&mut self, network_id: &str, channel: &str) -> bool {
        if network_id.is_empty() || channel.is_empty() {
            return false;
        }
        let key = self.reducer.conversation_key(network_id, channel);
        match self.reducer.find(&key) {
            Some(conversation) if conversation.is_channel() => {
                self.drop_and_reselect(key);
                true
            }
            _ => false,
        }
    }

    /// Removes a conversation's row, then selects its neighbour when the dropped row
    /// was selected, or clears the selection when nothing is left
    /// (`DropConversationAndReselect`).
    pub(super) fn drop_and_reselect(&mut self, key: ConversationKey) {
        let is_channel = self
            .reducer
            .find(&key)
            .is_some_and(|conversation| conversation.is_channel());
        let was_selected = self.selected.as_ref() == Some(&key);

        let next = if was_selected {
            let ordered = self
                .reducer
                .sidebar_order_with_networks(&self.network_order);
            neighbor_after_drop(&ordered, &key)
        } else {
            None
        };

        self.reducer.set_muted(&key, false);
        if is_channel {
            self.reducer.drop_channel(&key);
        } else {
            self.read_markers.forget_conversation(&key);
            self.reducer.drop_direct_message(&key);
        }
        self.queue(viewnotify::reset_all());
        if !was_selected {
            return;
        }
        match next {
            Some(next) => {
                let target = self
                    .reducer
                    .find(&next)
                    .map(|neighbor| neighbor.target.clone())
                    .unwrap_or_else(|| next.normalized_target.clone());
                self.select(&next.network_id, &target);
            }
            None => self.clear_selected(),
        }
    }
}
