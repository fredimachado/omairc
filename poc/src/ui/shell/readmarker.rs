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
        let focused = window.is_window_active();
        self.controller.note_transcript_viewport(focused, follow_end);
    }

    pub(super) fn report_transcript_viewport_from_shell(
        &mut self,
        follow_end: bool,
        _cx: &mut Context<Self>,
    ) {
        self.controller
            .note_transcript_viewport(self.window_active, follow_end);
    }
}
