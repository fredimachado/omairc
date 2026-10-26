//! The composer's slash-complete popup state (Qt `IrcSlashSession`, without the Qt
//! plumbing): the highlighted row, the needle Escape dismissed, and the text Tab and
//! Enter insert. The projection of composer text onto rows, and the ranking, are
//! Toolkit-free. The shell owns the field: it calls `sync` after each edit, asks `tab`
//! and `enter` what to insert, and applies the text these return.

use crate::irc::command::ComposerSurface;
pub use crate::irc::slashcomplete::SlashHit;
use crate::irc::slashcomplete::{project_slash, SlashProbe};

/// The muted text after a row's label (Qt `usageHint` in `client/src/ui/center/`).
/// A usage that starts with its label shows the rest, so `/version [nick]` shows
/// ` [nick]`. Any other usage shows in full, as `Show peer avatars` does.
pub fn usage_hint<'a>(label: &str, usage: &'a str) -> &'a str {
    usage.strip_prefix(label).unwrap_or(usage)
}

/// The popup while the composer is being edited (Qt `IrcSlashSession`, without the Qt
/// plumbing). It keeps the highlighted row, and Escape dismisses the popup for the
/// current needle until that needle changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlashSession {
    probe: Option<SlashProbe>,
    selected: usize,
    /// The needle Escape dismissed. The popup stays shut while it still matches.
    dismissed: Option<String>,
}

impl SlashSession {
    pub fn new() -> Self {
        Self::default()
    }

    /// Recomputes the popup after an edit or a surface change. Pass `browsing: true`
    /// while Up or Down is walking history: the popup stays closed then, even if the
    /// recalled line starts with a slash.
    pub fn sync(&mut self, composer_text: &str, surface: ComposerSurface, browsing: bool) {
        let probe = if browsing {
            None
        } else {
            project_slash(composer_text, surface)
        };
        self.apply(probe);
    }

    pub fn is_open(&self) -> bool {
        self.probe.is_some()
    }

    /// The rows on screen. Empty while the popup is closed.
    pub fn hits(&self) -> &[SlashHit] {
        match &self.probe {
            Some(probe) => probe.hits(),
            None => &[],
        }
    }

    /// The highlighted row's index. Meaningless while the popup is closed.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Up (`-1`) or Down (`1`). The highlight wraps at both ends. No-op when closed.
    pub fn move_selection(&mut self, delta: isize) {
        let Some(probe) = &self.probe else {
            return;
        };
        let count = probe.hits().len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(count) as usize;
    }

    /// Highlights row `index`, clamped to the rows. No-op when closed.
    pub fn select(&mut self, index: usize) {
        if let Some(probe) = &self.probe {
            self.selected = index.min(probe.hits().len() - 1);
        }
    }

    /// Tab: the text with the highlighted row inserted, and a space after it. `None`
    /// when the popup is closed, so Tab falls through to nick completion.
    pub fn tab(&self, composer_text: &str) -> Option<String> {
        self.insertion(composer_text, self.selected)
    }

    /// Enter: the inserted text when the typed word is partial. `None` means Enter
    /// should send. That is what happens when the popup is closed, or when the typed
    /// text already names the highlighted row in full, by its name or an alias.
    pub fn enter(&self, composer_text: &str) -> Option<String> {
        let probe = self.probe.as_ref()?;
        if probe.names_exactly(self.selected) {
            return None;
        }
        self.insertion(composer_text, self.selected)
    }

    /// A click on row `index`: highlight it, then insert it as Tab does.
    pub fn activate(&mut self, index: usize, composer_text: &str) -> Option<String> {
        self.select(index);
        self.insertion(composer_text, self.selected)
    }

    /// Shuts the popup without remembering a dismissed needle. Find mode uses it: the
    /// field holds a query there, so the popup stays shut, and it can open again on the
    /// same text once find ends.
    pub fn close(&mut self) {
        self.probe = None;
        self.selected = 0;
    }

    /// Escape. Closes the popup and keeps the typed text. Returns false when the popup
    /// was already closed, so Escape falls through to the other handlers.
    pub fn dismiss(&mut self) -> bool {
        let Some(probe) = self.probe.take() else {
            return false;
        };
        self.dismissed = Some(probe.needle().to_string());
        self.selected = 0;
        true
    }

    /// The composer text with the typed word replaced by row `index` and a space.
    /// Leading spaces are kept, as Qt's `replacedComposer` keeps them.
    fn insertion(&self, composer_text: &str, index: usize) -> Option<String> {
        let hit = self.probe.as_ref()?.hits().get(index)?;
        let lead = composer_text.len() - composer_text.trim_start().len();
        Some(format!("{}{} ", &composer_text[..lead], hit.label))
    }

    fn apply(&mut self, probe: Option<SlashProbe>) {
        let Some(probe) = probe else {
            self.probe = None;
            self.selected = 0;
            self.dismissed = None;
            return;
        };
        if self.dismissed.as_deref() == Some(probe.needle()) {
            self.probe = None;
            self.selected = 0;
            return;
        }
        self.dismissed = None;
        let needle_changed = self
            .probe
            .as_ref()
            .is_none_or(|open| open.needle() != probe.needle());
        self.selected = if needle_changed {
            0
        } else {
            self.selected.min(probe.hits().len() - 1)
        };
        self.probe = Some(probe);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::command::ComposerSurface::{Conversation, Status};

    #[test]
    fn tab_inserts_the_row_with_a_space_and_keeps_leading_spaces() {
        let mut session = SlashSession::new();
        session.sync("/v", Conversation, false);
        assert_eq!(session.tab("/v").as_deref(), Some("/version "));

        session.sync("  /v", Conversation, false);
        assert_eq!(session.tab("  /v").as_deref(), Some("  /version "));
    }

    #[test]
    fn enter_on_a_partial_word_inserts_and_on_an_exact_name_sends() {
        let mut session = SlashSession::new();
        session.sync("/v", Conversation, false);
        assert_eq!(session.enter("/v").as_deref(), Some("/version "));

        session.sync("/version", Conversation, false);
        assert_eq!(session.enter("/version"), None, "an exact name sends");
        assert_eq!(session.tab("/version").as_deref(), Some("/version "));

        session.sync("/pref avatars", Conversation, false);
        assert_eq!(session.enter("/pref avatars"), None);

        session.sync("/pref avatars o", Conversation, false);
        assert_eq!(
            session.enter("/pref avatars o").as_deref(),
            Some("/pref avatars on ")
        );
    }

    #[test]
    fn up_and_down_wrap_around_the_rows() {
        let mut session = SlashSession::new();
        session.sync("/pref ", Conversation, false);
        assert_eq!(session.selected(), 0);
        session.move_selection(-1);
        assert_eq!(session.selected(), 4);
        session.move_selection(1);
        assert_eq!(session.selected(), 0);
        session.move_selection(1);
        session.move_selection(1);
        assert_eq!(session.selected(), 2);
    }

    #[test]
    fn select_clamps_and_a_click_inserts_that_row() {
        let mut session = SlashSession::new();
        session.sync("/pref ", Conversation, false);
        session.select(99);
        assert_eq!(session.selected(), 4);
        assert_eq!(
            session.activate(1, "/pref ").as_deref(),
            Some("/pref avatars ")
        );
        assert_eq!(session.selected(), 1);
    }

    #[test]
    fn a_closed_popup_ignores_keys_and_falls_through() {
        let mut session = SlashSession::new();
        session.sync("hello", Conversation, false);
        assert!(!session.is_open());
        session.move_selection(1);
        session.select(3);
        assert_eq!(session.tab("hello"), None);
        assert_eq!(session.enter("hello"), None);
        assert_eq!(session.activate(0, "hello"), None);
        assert!(!session.dismiss(), "Escape is not consumed when closed");
    }

    #[test]
    fn close_shuts_the_popup_without_remembering_the_needle() {
        let mut session = SlashSession::new();
        session.sync("/v", Conversation, false);
        assert!(session.is_open());
        session.close();
        assert!(!session.is_open());
        assert!(
            !session.dismiss(),
            "nothing was dismissed, so Escape is not consumed"
        );

        // Unlike Escape, the same text opens the popup again.
        session.sync("/v", Conversation, false);
        assert!(session.is_open());
    }

    #[test]
    fn escape_closes_and_keeps_the_text_until_the_needle_changes() {
        let mut session = SlashSession::new();
        session.sync("/v", Conversation, false);
        assert!(session.dismiss());
        assert!(!session.is_open());

        session.sync("/v", Conversation, false);
        assert!(!session.is_open(), "the same needle stays shut");

        session.sync("/ve", Conversation, false);
        assert!(session.is_open(), "a new needle reopens the popup");
    }

    #[test]
    fn editing_to_a_new_needle_resets_the_highlight_but_keeping_it_is_kept() {
        let mut session = SlashSession::new();
        session.sync("/pref ", Conversation, false);
        session.select(2);
        session.sync("/pref ", Conversation, false);
        assert_eq!(session.selected(), 2, "the same needle keeps the highlight");
        session.sync("/pref a", Conversation, false);
        assert_eq!(session.selected(), 0, "a new needle starts at the top");
    }

    #[test]
    fn walking_history_keeps_the_popup_closed() {
        let mut session = SlashSession::new();
        session.sync("/v", Conversation, true);
        assert!(!session.is_open());
        session.sync("/v", Conversation, false);
        assert!(session.is_open());
    }

    #[test]
    fn the_usage_hint_drops_the_label_only_when_the_usage_starts_with_it() {
        assert_eq!(usage_hint("/version", "/version [nick]"), " [nick]");
        assert_eq!(
            usage_hint("/pref avatars", "Show peer avatars"),
            "Show peer avatars"
        );
        assert_eq!(usage_hint("/pref joins every", "/pref joins every"), "");
    }

    #[test]
    fn a_status_session_never_offers_me() {
        let mut session = SlashSession::new();
        session.sync("/", Status, false);
        assert!(!session.is_open());
        // `/m` still opens on Status for the verbs that start with it (`/mode`
        // `/monitor`), but `/me` is conversation-only and never a row there.
        session.sync("/m", Status, false);
        assert!(session.is_open());
        assert!(session.hits().iter().all(|hit| hit.label != "/me"));
        // Ties at the prefix tier keep catalog order, so /msg (row 8) leads /mode (row 21).
        assert_eq!(session.hits()[0].label, "/msg");
    }

    #[test]
    fn a_closed_or_double_slash_popup_gives_no_tab_insertion() {
        let mut session = SlashSession::new();
        session.sync("/", Conversation, false);
        assert_eq!(session.tab("/"), None);
        session.sync("/jo", Conversation, false);
        session.sync("//", Conversation, false);
        assert!(!session.is_open());
        assert_eq!(
            session.tab("//"),
            None,
            "Tab falls through to nick completion"
        );
    }

    // (`slashSessionKeys`): the keys on a verb, an alias, a
    // prefix, a whole word and a dismissed needle.

    #[test]
    fn the_session_keys_follow_the_core_matrix() {
        let mut session = SlashSession::new();
        session.sync("/j", Conversation, false);
        assert!(session.is_open());
        assert_eq!(session.selected(), 0);
        assert_eq!(
            session.tab("/j").as_deref(),
            Some("/join "),
            "Tab inserts /join with a space"
        );
        session.sync("/join ", Conversation, false);
        assert!(!session.is_open(), "the inserted text closes the popup");

        session.sync("/close", Conversation, false);
        assert!(session.is_open());
        assert_eq!(
            session.enter("/close"),
            None,
            "Enter on an exact name sends"
        );

        session.sync("/j", Conversation, false);
        assert_eq!(session.enter("/j"), None, "Enter on the alias /j sends");

        session.sync("/jo", Conversation, false);
        assert_eq!(
            session.enter("/jo").as_deref(),
            Some("/join "),
            "Enter on a partial inserts"
        );

        session.sync("  /jo", Conversation, false);
        assert_eq!(session.activate(0, "  /jo").as_deref(), Some("  /join "));

        session.sync("/jo", Conversation, false);
        assert!(session.dismiss(), "Escape closes the open popup");
        session.sync("/jo", Conversation, false);
        assert!(!session.is_open(), "the dismissed needle stays shut");

        session.sync("/j", Conversation, false);
        assert!(session.is_open());
        session.sync("/", Conversation, false);
        assert!(!session.is_open());
        session.sync("/jo", Conversation, false);
        assert!(session.is_open());
        session.sync("//", Conversation, false);
        assert!(!session.is_open());
    }

    #[test]
    fn the_listed_hit_labels_are_shown_as_inserted() {
        let mut session = SlashSession::new();
        session.sync("/q", Conversation, false);
        assert_eq!(session.hits()[0].label, "/query");
        session.sync("/quit", Conversation, false);
        assert_eq!(session.hits()[0].label, "/disconnect");
        assert_eq!(session.tab("/quit").as_deref(), Some("/disconnect "));
    }
}
