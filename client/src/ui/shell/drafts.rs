//! Unsent composer text across a restart. The composer's own memory (`ComposerState`) keeps a
//! draft per target while the app runs; this hands each conversation's draft to the
//! controller, which keeps it in `drafts.json` (`controller::drafts`), and asks for it back
//! when a conversation shows. Writes happen when the shell switches away, when the window
//! loses focus, when a line is sent, and on quit or close, never per keystroke. What is kept is
//! what `ComposerState` stored or `unsent_text` names, never the field's text, which may be a
//! find query or a recalled history line. Status has no persisted draft.

use gpui::{App, Context};

use crate::model::composer::DraftKey;
use crate::ui::shell::Shell;

impl Shell {
    /// Keeps what the field holds as unsent text for the conversation on screen. Blur and quit
    /// call this. It does not stash, so the composer's memory is untouched.
    pub fn persist_current_draft(&mut self, cx: &App) {
        let key = self.shown.draft_key();
        let text = self.composer_state.unsent_text(self.composer.read(cx).text());
        self.remember_draft(&key, &text);
    }

    /// Keeps what a stash stored for `key`, as the shell leaves that view.
    pub(super) fn persist_stashed_draft(&mut self, key: &DraftKey) {
        let text = self.composer_state.stored(key).unwrap_or("").to_owned();
        self.remember_draft(key, &text);
    }

    /// Removes the stored draft of the conversation on screen, as a sent line empties it.
    pub(super) fn forget_current_draft(&mut self) {
        let key = self.shown.draft_key();
        self.remember_draft(&key, "");
    }

    /// Fills the field from the stored draft when the view on screen has no draft in memory.
    /// Called for the view a launch opens on. A restart is the only way a conversation has a
    /// stored draft the composer does not know.
    pub fn restore_shown_draft(&mut self, cx: &mut Context<Self>) {
        let key = self.shown.draft_key();
        if let Some(text) = self.restored_draft(&key) {
            self.set_composer_text(text, cx);
        }
    }

    /// The text a switch to `key` should show when the composer's memory has none. It is also
    /// seeded into that memory, so a later stash sees it.
    pub(super) fn restored_draft(&mut self, key: &DraftKey) -> Option<String> {
        let DraftKey::Conversation(conversation) = key else {
            return None;
        };
        let text = self
            .controller
            .draft_for(&conversation.network_id, &conversation.key)?;
        self.composer_state.preload(key, &text);
        self.composer_state.stored(key).map(str::to_owned)
    }

    fn remember_draft(&mut self, key: &DraftKey, text: &str) {
        if let DraftKey::Conversation(conversation) = key {
            if !conversation.key.is_empty() {
                self.controller
                    .remember_draft(&conversation.network_id, &conversation.key, text);
            }
        }
    }
}
