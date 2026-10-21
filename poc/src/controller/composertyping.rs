//! The composer's outbound typing hint: port of `IrcController::notifyComposerText`
//! and the done flush of `selectConversation` and `clearConversationSelection`
//! (`tui/internal/controller/wiring.go` `NotifyComposerText`, `controller.go`
//! `SelectConversation`; `src/irc/irccontroller.cpp` (C++)).
//!
//! The pace, the `message-tags` gate and the suppressed `done` after a sent message
//! belong to the session's typing publisher (`session/typing.rs`). The controller
//! decides which composer edits count as typing and which target the hint is still
//! open on, so a `done` is sent when the user clears the composer, moves away, or
//! the selection changes. The Status console and a network without `message-tags`
//! never publish.
//!
//! Inbound typing (who is typing to us) is the reads in `reads.rs`.

use crate::irc::command::parse;
use crate::irc::typing::TypingPhase;

use super::controller::Controller;

impl Controller {
    /// The composer text for `target` on `network_id` changed (`notifyComposerText`).
    /// A live line (plain text, `//`, or `/me`) publishes `active` and holds the
    /// target open. Any other text withdraws the hint with `done`, but only when this
    /// target still holds it. Nothing happens for the Status console, for a target
    /// that is not the selection, or without `message-tags`.
    pub fn composer_edited(&mut self, network_id: &str, target: &str, text: &str) {
        self.pump();
        if self.console_open || !self.is_selected_target(network_id, target) {
            return;
        }
        if !self.has_typing() {
            return;
        }
        let Some(session) = self.session(network_id) else {
            return;
        };
        if parse(text).is_live_message() {
            session.send_typing(target, TypingPhase::Active);
            self.typing_target = Some((network_id.to_owned(), target.to_owned()));
            self.pump();
            return;
        }
        let held = self
            .typing_target
            .as_ref()
            .is_some_and(|(network, held)| network == network_id && held == target);
        if !held {
            return;
        }
        session.send_typing(target, TypingPhase::Done);
        self.typing_target = None;
        self.pump();
    }

    /// The composer's text changed while a conversation is selected. The selection supplies the
    /// network and target, so the shell only reports the text (`NotifyComposerText`). Status and
    /// find mode never publish, since the shell does not report them.
    pub fn composer_text_changed(&mut self, text: &str) {
        let Some(key) = self.selected.clone() else {
            return;
        };
        let target = self.selected_target().to_owned();
        self.composer_edited(&key.network_id, &target, text);
    }

    /// The network and target the composer's typing hint is held on, if any: the last
    /// target a `typing=active` went to and no `done` has followed yet.
    // A test seam: the typing tests read the held target; the allow comes off with a consumer.
    #[allow(dead_code)]
    pub fn typing_target(&self) -> Option<(&str, &str)> {
        self.typing_target
            .as_ref()
            .map(|(network, target)| (network.as_str(), target.as_str()))
    }

    /// Withdraws the hint the selection was holding, with a `done` on its target
    /// (the flush of `selectConversation` and `clearConversationSelection`).
    pub(super) fn withdraw_typing(&mut self) {
        let Some((network_id, target)) = self.typing_target.take() else {
            return;
        };
        if let Some(session) = self.session(&network_id) {
            session.send_typing(&target, TypingPhase::Done);
        }
    }

    /// Whether `network_id` and `target` name the selected conversation.
    fn is_selected_target(&self, network_id: &str, target: &str) -> bool {
        self.selected
            .as_ref()
            .is_some_and(|key| key.network_id == network_id)
            && self.selected_target == target
    }
}
