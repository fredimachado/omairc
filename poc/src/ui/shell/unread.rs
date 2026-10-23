//! Window focus and the unseen jump, wired from the shell to the open transcript. The
//! unread rules live in the controller (`set_window_active`, `mark_all_read`); the
//! transcript's landing rules live in `model::unread`.

use gpui::{Context, Window};

use super::Shell;
use crate::model::chords::Chord;

impl Shell {
    /// The window gains or loses focus (Qt `windowFocusGained` and `windowFocusLost`).
    /// Gaining focus reads the open conversation (the controller does it), and the
    /// transcript lands on its mark when it has one. Losing focus only stops arrivals in
    /// the open conversation from counting as seen.
    pub fn set_window_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.window_active == active {
            return;
        }
        self.window_active = active;
        self.controller.set_window_active(active);
        if active {
            self.title_mark = None;
        }
        if active && !self.status_showing() {
            self.transcript.update(cx, |t, _| t.land_on_unread_mark());
        }
        let at_end = self.transcript.read(cx).at_end();
        self.report_transcript_viewport_from_shell(at_end, cx);
        self.drain_notifications(cx);
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
