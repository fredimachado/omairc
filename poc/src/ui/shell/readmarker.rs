//! Reports transcript viewport state to the controller for read-marker publishing.

use gpui::{Context, Window};

use super::Shell;

impl Shell {
    pub(super) fn report_transcript_viewport(
        &mut self,
        follow_end: bool,
        window: &Window,
        _cx: &mut Context<Self>,
    ) {
        self.transcript_viewport_follow_end = follow_end;
        let focused = window.is_window_active();
        self.controller.note_transcript_viewport(focused, follow_end);
    }

    pub(super) fn report_transcript_viewport_from_shell(
        &mut self,
        follow_end: bool,
        _cx: &mut Context<Self>,
    ) {
        self.transcript_viewport_follow_end = follow_end;
        self.controller
            .note_transcript_viewport(self.window_active, follow_end);
    }

    /// Wheel and scrollbar scrolling move the list without `transcript_scroll`.
    /// Compare the live at-end bit to the last report and sync when it changes.
    pub(super) fn sync_transcript_viewport_if_at_end_changed(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let at_end = self.transcript.read(cx).at_end();
        if at_end == self.transcript_viewport_follow_end {
            return;
        }
        self.report_transcript_viewport(at_end, window, cx);
    }
}
