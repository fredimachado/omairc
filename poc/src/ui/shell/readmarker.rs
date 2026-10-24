//! Reports transcript viewport state to the controller for read-marker publishing.

use gpui::{Context, Window};

use super::Shell;

impl Shell {
    pub(super) fn report_transcript_viewport(
        &mut self,
        follow_end: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.transcript.update(cx, |t, _| t.note_reported_at_end(follow_end));
        let focused = window.is_window_active();
        self.controller.note_transcript_viewport(focused, follow_end);
    }

    /// Reports the at-end bit the shell read. The transcript keeps the same bit, so its
    /// next wheel scroll emits `TranscriptScrolled` only for a change from it.
    pub(super) fn report_transcript_viewport_from_shell(
        &mut self,
        follow_end: bool,
        cx: &mut Context<Self>,
    ) {
        self.transcript.update(cx, |t, _| t.note_reported_at_end(follow_end));
        self.controller
            .note_transcript_viewport(self.window_active, follow_end);
    }
}
