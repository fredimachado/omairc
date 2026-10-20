//! Arrivals, window focus and the unseen jump, wired from the shell to the open
//! transcript. The rules live in `model::unread`.

use gpui::{Context, Window};

use super::Shell;
use crate::model::chords::Chord;
use crate::model::unread::{arrive_in, find_target, focus_gained, mark_line};
use crate::model::MockMessage;

impl Shell {
    /// Lands a chat line from `network` in `conversation`, as the live path would.
    /// It counts as unread unless the reader sees it now: the conversation is the
    /// selected one and the window is active. Status keeps the selection, as Qt's
    /// `openStatus` does, so the transcript shows the line only while the
    /// conversation is on screen. A conversation the world does not hold is left
    /// alone, and so is the world.
    // Dormant: the demo world has no live arrivals; model::unread tests cover the rules.
    #[allow(dead_code)]
    pub fn ingest(
        &mut self,
        network: &str,
        conversation: &str,
        line: MockMessage,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = find_target(&self.world, network, conversation) else {
            return false;
        };
        let selected = target == self.selected;
        let seen = selected && self.window_active;
        let showing = selected && self.status.is_none();
        let Some(arrival) = arrive_in(&mut self.world, &target, line.clone(), seen) else {
            return false;
        };
        if showing {
            let mut lines = Vec::with_capacity(2);
            if arrival.marked {
                lines.push(mark_line());
            }
            lines.push(line);
            self.transcript.update(cx, |t, _| t.append_arrival(&lines));
        }
        cx.notify();
        true
    }

    /// The window gains or loses focus (Qt `windowFocusGained` and
    /// `windowFocusLost`). Gaining focus reads the open conversation, and the
    /// transcript lands on its mark when it has one. Losing focus only stops
    /// arrivals in the open conversation from counting as seen.
    pub fn set_window_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.window_active == active {
            return;
        }
        self.window_active = active;
        if active {
            if let Some(conv) = self.world.find_mut(&self.selected) {
                focus_gained(conv);
            }
            if self.status.is_none() {
                self.transcript.update(cx, |t, _| t.land_on_unread_mark());
            }
        }
        cx.notify();
    }

    /// Alt+U: the unseen jump. It is live on a conversation with no sheet open.
    pub fn jump_to_unseen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::Unseen, window, cx) {
            return;
        }
        if self.transcript.update(cx, |t, _| t.jump_to_unseen()) {
            cx.notify();
        }
    }
}
