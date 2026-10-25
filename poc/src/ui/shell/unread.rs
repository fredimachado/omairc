//! Window focus, minimize/hide, and the unseen jump, wired from the shell to the
//! open transcript. The unread rules live in the controller (`set_window_active`,
//! `mark_all_read`); the transcript's landing rules live in `model::unread`.
//! Minimized and hidden count as away even while OS focus stays true (Qt
//! `transcriptSuspended` / `arrivalWindowActive`).

use gpui::{Context, Window};

use super::Shell;
use crate::desktop::windowvisibility::{TranscriptActivity, WindowVisibility};
use crate::model::chords::Chord;

impl Shell {
    /// Activity for unread consumption and mention notify (`arrivalWindowActive`).
    pub fn arrival_active(&self) -> bool {
        self.activity.effective()
    }

    /// The window gains or loses OS focus (Qt `windowFocusGained` / `windowFocusLost`).
    /// A focus report while minimized or hidden does not consume the backlog.
    pub fn set_window_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if let Some(effective) = self.activity.set_focus(active) {
            self.apply_arrival_active(effective, cx);
        }
    }

    /// Qt `onVisibilityChanged`. Minimized and Hidden suspend catch-up. Leaving
    /// suspension re-applies focus only when the window is actually active
    /// (`finishTranscriptSuspension`).
    pub fn set_window_visibility(&mut self, visibility: WindowVisibility, cx: &mut Context<Self>) {
        if let Some(effective) = self.activity.set_visibility(visibility) {
            self.apply_arrival_active(effective, cx);
        }
    }

    /// Pushes effective activity to the controller and lands the transcript when
    /// catch-up turns on.
    fn apply_arrival_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.applied_arrival_active == active {
            return;
        }
        self.applied_arrival_active = active;
        self.controller.set_window_active(active);
        if active {
            self.title_mark = None;
        }
        if active && !self.status_showing() {
            self.transcript
                .update(cx, |t, cx| t.land_on_unread_mark(cx));
        }
        let at_end = self.transcript.read(cx).at_end();
        self.report_transcript_viewport_from_shell(at_end, cx);
        self.drain_notifications(cx);
        cx.notify();
    }

    /// Seeds focus and visibility from the live window, then watches both.
    pub(super) fn watch_window_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.activation_watched {
            return;
        }
        self.activation_watched = true;
        self.activity = TranscriptActivity {
            window_active: window.is_window_active(),
            suspended: crate::desktop::windowvisibility::of(window).is_suspended(),
        };
        self.apply_arrival_active(self.activity.effective(), cx);

        let activation = cx.observe_window_activation(
            window,
            |shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>| {
                shell.set_window_active(window.is_window_active(), cx);
            },
        );
        let bounds = cx.observe_window_bounds(window, |shell: &mut Shell, window: &mut Window, cx| {
            shell.set_window_visibility(crate::desktop::windowvisibility::of(window), cx);
        });
        self._subscriptions.push(activation);
        self._subscriptions.push(bounds);
    }

    /// Alt+U: the unseen jump. It is live on a conversation with no sheet open.
    pub fn jump_to_unseen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::Unseen, window, cx) {
            return;
        }
        if self.transcript.update(cx, |t, cx| t.jump_to_unseen(cx)) {
            cx.notify();
        }
    }
}
