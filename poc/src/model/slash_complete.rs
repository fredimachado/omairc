//! The slash-command popup: a port of `IrcSlashComplete::project`, `rank`,
//! `scoreSpec`, `scoreToken` and `projectPref`, plus the key rules of
//! `IrcSlashSession` (`src/irc/ircslashcomplete.cpp`). It covers the six-verb
//! catalog and the `/pref` names and values.
//!
//! Ranking is Qt's, not the fuzzy matcher the sheets use. A row must start with
//! the same letter as the typed word. Then it scores exact (100), prefix (80),
//! substring (50), or an ordered subsequence (20 plus a closeness term).
//!
//! Toolkit-free. The shell owns the field: it calls `sync` after each edit, asks
//! `tab` and `enter` what to insert, and applies the text these return.

// The shell wires these in Phase 4 dispatch; drop the allow then.
#![allow(dead_code)]

use super::prefs::{MembershipNoise, PrefName};
use super::slash::{Surface, CATALOG};

/// Most verb rows the popup shows (Qt `rank` keeps six). `/pref` value rows are not capped.
pub const RANK_LIMIT: usize = 6;

/// One popup row. `label` is what Tab and Enter insert, such as `/version`
/// or `/pref joins every`. `usage` is the text beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlashHit {
    pub label: String,
    pub usage: String,
}

/// The rows for what is typed, and the typed needle they match (Qt `IrcSlashProbe`).
/// It only exists when it has rows, so an open probe always has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlashProbe {
    needle: String,
    hits: Vec<SlashHit>,
}

impl SlashProbe {
    /// The typed text after the slash, lowercased. For `/pref joins `, it is `pref joins `.
    pub fn needle(&self) -> &str {
        &self.needle
    }

    pub fn hits(&self) -> &[SlashHit] {
        &self.hits
    }

    /// Whether the typed text already names row `index` in full. Enter then sends
    /// rather than inserts (Qt `tokenPassesSelected`, without aliases).
    pub fn names_exactly(&self, index: usize) -> bool {
        self.hits
            .get(index)
            .and_then(|hit| hit.label.strip_prefix('/'))
            .is_some_and(|name| name == self.needle)
    }
}

/// Projects the composer text onto the popup rows for `surface`. `None` means
/// the popup stays closed. This is `IrcSlashComplete::project`.
pub fn project(composer_text: &str, surface: Surface) -> Option<SlashProbe> {
    match openable_needle(composer_text) {
        Some(needle) => {
            let hits = rank(&needle, surface);
            open(needle, hits)
        }
        None => project_pref(composer_text),
    }
}

fn open(needle: String, hits: Vec<SlashHit>) -> Option<SlashProbe> {
    (!needle.is_empty() && !hits.is_empty()).then_some(SlashProbe { needle, hits })
}

/// The text after a leading `/` when the popup may open. It needs one non-space
/// character after the slash, and no space anywhere after it. `//` and `/ x` stay closed.
fn openable_needle(text: &str) -> Option<String> {
    let body = text.trim_start().strip_prefix('/')?;
    if body.is_empty() || body.starts_with('/') || body.contains(char::is_whitespace) {
        return None;
    }
    Some(body.to_lowercase())
}

/// How well `token` matches the typed `needle` (Qt `scoreToken`). Both are
/// lowercase. `-1` means no match.
pub fn score_token(needle: &str, token: &str) -> i32 {
    let (Some(needle_first), Some(token_first)) = (needle.chars().next(), token.chars().next())
    else {
        return -1;
    };
    if needle_first != token_first {
        return -1;
    }
    if needle == token {
        return 100;
    }
    if token.starts_with(needle) {
        return 80;
    }
    if token.contains(needle) {
        return 50;
    }
    // An ordered subsequence. `end` is one past the last matched character, so
    // a tight match scores higher than a spread-out one.
    let token_chars: Vec<char> = token.chars().collect();
    let mut end = 0usize;
    for ch in needle.chars() {
        match token_chars[end..]
            .iter()
            .position(|&candidate| candidate == ch)
        {
            Some(offset) => end += offset + 1,
            None => return -1,
        }
    }
    20 + (needle.chars().count() as i32 * 10) / end as i32
}

/// The verb rows that match `needle` on `surface`, best first, at most `RANK_LIMIT`.
/// Ties keep catalog order. This is `IrcSlashComplete::rank`.
fn rank(needle: &str, surface: Surface) -> Vec<SlashHit> {
    let scored: Vec<(usize, i32, SlashHit)> = CATALOG
        .iter()
        .enumerate()
        .filter(|(_, spec)| spec.allowed_on(surface))
        .filter_map(|(index, spec)| {
            let score = score_token(needle, spec.name);
            (score >= 0).then(|| {
                let hit = SlashHit {
                    label: format!("/{}", spec.name),
                    usage: spec.usage.to_string(),
                };
                (index, score, hit)
            })
        })
        .collect();
    sorted_hits(scored).into_iter().take(RANK_LIMIT).collect()
}

/// Sorts by score, best first, then by catalog index.
fn sorted_hits(mut scored: Vec<(usize, i32, SlashHit)>) -> Vec<SlashHit> {
    scored.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    scored.into_iter().map(|(_, _, hit)| hit).collect()
}

/// The `/pref` rows: names after `/pref `, then `on`/`off` or the joins levels
/// after `/pref <name> `. This is `IrcSlashComplete::projectPref`.
fn project_pref(text: &str) -> Option<SlashProbe> {
    let body = text.trim_start().strip_prefix('/')?;
    let verb_space = body.find(' ')?;
    if body[..verb_space].to_lowercase() != "pref" {
        return None;
    }
    let rest = &body[verb_space + 1..];
    if rest.starts_with(' ') {
        return None;
    }

    let needle = body.to_lowercase();
    match rest.find(' ') {
        None => pref_name_rows(needle, &rest.to_lowercase()),
        Some(name_space) => pref_value_rows(needle, &rest[..name_space], &rest[name_space + 1..]),
    }
}

/// The names that match `token`. An empty token lists all four in catalog order.
fn pref_name_rows(needle: String, token: &str) -> Option<SlashProbe> {
    let hits = if token.is_empty() {
        PrefName::ALL.iter().map(pref_name_hit).collect()
    } else {
        let scored: Vec<(usize, i32, SlashHit)> = PrefName::ALL
            .iter()
            .enumerate()
            .filter_map(|(index, name)| {
                let score = score_token(token, name.token());
                (score >= 0).then(|| (index, score, pref_name_hit(name)))
            })
            .collect();
        sorted_hits(scored)
    };
    open(needle, hits)
}

fn pref_name_hit(name: &PrefName) -> SlashHit {
    SlashHit {
        label: format!("/pref {}", name.token()),
        usage: name.label().to_string(),
    }
}

/// The values that match `value` for `name_word`. An empty value lists all of them.
/// A value with a space in it, or an unknown name, has no rows.
fn pref_value_rows(needle: String, name_word: &str, value: &str) -> Option<SlashProbe> {
    if value.contains(' ') {
        return None;
    }
    let name = PrefName::from_token(name_word)?;
    let words: Vec<&str> = match name {
        PrefName::Joins => MembershipNoise::ALL
            .iter()
            .map(|level| level.token())
            .collect(),
        _ => vec!["on", "off"],
    };
    let prefix = format!("/pref {} ", name.token());
    let folded = value.to_lowercase();

    let hits = if folded.is_empty() {
        words
            .iter()
            .map(|word| {
                let label = format!("{prefix}{word}");
                SlashHit {
                    label: label.clone(),
                    usage: label,
                }
            })
            .collect()
    } else {
        let scored: Vec<(usize, i32, SlashHit)> = words
            .iter()
            .enumerate()
            .filter_map(|(index, word)| {
                let score = score_token(&folded, word);
                (score >= 0).then(|| {
                    let label = format!("{prefix}{word}");
                    let hit = SlashHit {
                        label: label.clone(),
                        usage: label,
                    };
                    (index, score, hit)
                })
            })
            .collect();
        sorted_hits(scored)
    };
    open(needle, hits)
}

/// The popup while the composer is being edited (Qt `IrcSlashSession`, without
/// the Qt plumbing). It keeps the highlighted row, and Escape dismisses the
/// popup for the current needle until that needle changes.
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

    /// Recomputes the popup after an edit or a surface change. Pass
    /// `browsing: true` while Up or Down is walking history: the popup stays
    /// closed then, even if the recalled line starts with a slash.
    pub fn sync(&mut self, composer_text: &str, surface: Surface, browsing: bool) {
        let probe = if browsing {
            None
        } else {
            project(composer_text, surface)
        };
        self.apply(probe);
    }

    pub fn is_open(&self) -> bool {
        self.probe.is_some()
    }

    /// The rows on screen. Empty while the popup is closed.
    pub fn hits(&self) -> &[SlashHit] {
        match &self.probe {
            Some(probe) => &probe.hits,
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
        let count = probe.hits.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(count) as usize;
    }

    /// Highlights row `index`, clamped to the rows. No-op when closed.
    pub fn select(&mut self, index: usize) {
        if let Some(probe) = &self.probe {
            self.selected = index.min(probe.hits.len() - 1);
        }
    }

    /// Tab: the text with the highlighted row inserted, and a space after it.
    /// `None` when the popup is closed, so Tab falls through to nick completion.
    pub fn tab(&self, composer_text: &str) -> Option<String> {
        self.insertion(composer_text, self.selected)
    }

    /// Enter: the inserted text when the typed word is partial. `None` means
    /// Enter should send. That is what happens when the popup is closed, or
    /// when the typed text already names the highlighted row in full.
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

    /// Escape. Closes the popup and keeps the typed text. Returns false when the
    /// popup was already closed, so Escape falls through to the other handlers.
    pub fn dismiss(&mut self) -> bool {
        let Some(probe) = self.probe.take() else {
            return false;
        };
        self.dismissed = Some(probe.needle);
        self.selected = 0;
        true
    }

    /// The composer text with the typed word replaced by row `index` and a space.
    /// Leading spaces are kept, as Qt's `replacedComposer` keeps them.
    fn insertion(&self, composer_text: &str, index: usize) -> Option<String> {
        let hit = self.probe.as_ref()?.hits.get(index)?;
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
            .is_none_or(|open| open.needle != probe.needle);
        self.selected = if needle_changed {
            0
        } else {
            self.selected.min(probe.hits.len() - 1)
        };
        self.probe = Some(probe);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(probe: &Option<SlashProbe>) -> Vec<String> {
        probe
            .as_ref()
            .map(|probe| probe.hits.iter().map(|hit| hit.label.clone()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_single_letter_opens_only_the_verbs_it_starts() {
        let probe = project("/v", Surface::Conversation).expect("/v opens");
        assert_eq!(probe.needle(), "v");
        assert_eq!(
            probe.hits(),
            [SlashHit {
                label: "/version".to_string(),
                usage: "/version [nick]".to_string(),
            }]
        );
        assert_eq!(labels(&project("/V", Surface::Conversation)), ["/version"]);
    }

    #[test]
    fn p_opens_pref_with_its_full_usage() {
        let probe = project("/p", Surface::Conversation).expect("/p opens");
        assert_eq!(probe.hits().len(), 1);
        assert_eq!(probe.hits()[0].label, "/pref");
        assert_eq!(
            probe.hits()[0].usage,
            "/pref [directs|avatars|unread] [on|off] | joins [folded|every|hidden]"
        );
    }

    #[test]
    fn a_pref_name_prefix_lists_matching_names_with_their_labels() {
        let probe = project("/pref a", Surface::Conversation).expect("opens");
        assert_eq!(probe.needle(), "pref a");
        assert_eq!(
            probe.hits(),
            [SlashHit {
                label: "/pref avatars".to_string(),
                usage: "Show peer avatars".to_string(),
            }]
        );
    }

    #[test]
    fn pref_with_a_space_lists_every_name_in_catalog_order() {
        let probe = project("/pref ", Surface::Conversation).expect("opens");
        assert_eq!(
            probe
                .hits()
                .iter()
                .map(|hit| hit.label.as_str())
                .collect::<Vec<_>>(),
            [
                "/pref directs",
                "/pref avatars",
                "/pref unread",
                "/pref joins"
            ]
        );
        assert_eq!(probe.hits()[3].usage, "Join, part, quit, and nick lines");
    }

    #[test]
    fn pref_joins_with_a_space_lists_the_three_levels() {
        let probe = project("/pref joins ", Surface::Conversation).expect("opens");
        assert_eq!(
            probe.hits(),
            [
                SlashHit {
                    label: "/pref joins folded".to_string(),
                    usage: "/pref joins folded".to_string(),
                },
                SlashHit {
                    label: "/pref joins every".to_string(),
                    usage: "/pref joins every".to_string(),
                },
                SlashHit {
                    label: "/pref joins hidden".to_string(),
                    usage: "/pref joins hidden".to_string(),
                },
            ]
        );
    }

    #[test]
    fn toggles_offer_on_and_off_and_rank_by_the_typed_letter() {
        assert_eq!(
            labels(&project("/pref avatars ", Surface::Conversation)),
            ["/pref avatars on", "/pref avatars off"]
        );
        assert_eq!(
            labels(&project("/pref avatars o", Surface::Conversation)),
            ["/pref avatars on", "/pref avatars off"]
        );
        assert_eq!(
            labels(&project("/pref avatars of", Surface::Conversation)),
            ["/pref avatars off"]
        );
    }

    #[test]
    fn joins_levels_match_a_typed_prefix() {
        assert_eq!(
            labels(&project("/pref joins f", Surface::Conversation)),
            ["/pref joins folded"]
        );
    }

    #[test]
    fn the_popup_stays_closed_on_blank_double_and_spaced_input() {
        for text in [
            "",
            "/",
            "//x",
            "//",
            "/ v",
            "/me ",
            "/me hi",
            "/pref  a",
            "/pref avatars on x",
            "v",
            "  ",
        ] {
            assert!(
                project(text, Surface::Conversation).is_none(),
                "{text:?} must stay closed"
            );
        }
    }

    #[test]
    fn unknown_names_and_values_have_no_rows() {
        assert!(project("/zzz", Surface::Conversation).is_none());
        assert!(project("/pref notify", Surface::Conversation).is_none());
        assert!(project("/pref joins x", Surface::Conversation).is_none());
        assert!(project("/pref avatars xyz", Surface::Conversation).is_none());
    }

    #[test]
    fn a_bare_pref_word_opens_the_verb_row() {
        let probe = project("/pref", Surface::Conversation).expect("opens");
        assert_eq!(probe.needle(), "pref");
        assert_eq!(labels(&Some(probe)), ["/pref"]);
    }

    #[test]
    fn status_hides_me_but_keeps_the_rest() {
        assert!(project("/m", Surface::Status).is_none());
        assert!(project("/me", Surface::Status).is_none());
        assert_eq!(labels(&project("/m", Surface::Conversation)), ["/me"]);
        assert_eq!(labels(&project("/cl", Surface::Status)), ["/clear"]);
        assert_eq!(labels(&project("/pref ", Surface::Status)).len(), 4);
    }

    #[test]
    fn score_tiers_follow_qt() {
        assert_eq!(score_token("version", "version"), 100);
        assert_eq!(score_token("ver", "version"), 80);
        assert_eq!(score_token("ion", "idion"), 50);
        assert_eq!(score_token("ls", "list"), 26);
        assert_eq!(score_token("lt", "list"), 25);
        assert_eq!(score_token("on", "version"), -1);
        assert_eq!(score_token("ion", "version"), -1);
        assert_eq!(score_token("", "version"), -1);
        assert_eq!(score_token("v", ""), -1);
    }

    #[test]
    fn the_case_of_the_catalog_and_the_needle_does_not_change_the_match() {
        assert_eq!(labels(&project("/LIST", Surface::Conversation)), ["/list"]);
        assert_eq!(
            labels(&project("/Pref Av", Surface::Conversation)),
            ["/pref avatars"]
        );
    }

    #[test]
    fn tab_inserts_the_row_with_a_space_and_keeps_leading_spaces() {
        let mut session = SlashSession::new();
        session.sync("/v", Surface::Conversation, false);
        assert_eq!(session.tab("/v").as_deref(), Some("/version "));

        session.sync("  /v", Surface::Conversation, false);
        assert_eq!(session.tab("  /v").as_deref(), Some("  /version "));
    }

    #[test]
    fn enter_on_a_partial_word_inserts_and_on_an_exact_name_sends() {
        let mut session = SlashSession::new();
        session.sync("/v", Surface::Conversation, false);
        assert_eq!(session.enter("/v").as_deref(), Some("/version "));

        session.sync("/version", Surface::Conversation, false);
        assert_eq!(session.enter("/version"), None, "an exact name sends");
        assert_eq!(session.tab("/version").as_deref(), Some("/version "));

        session.sync("/pref avatars", Surface::Conversation, false);
        assert_eq!(session.enter("/pref avatars"), None);

        session.sync("/pref avatars o", Surface::Conversation, false);
        assert_eq!(
            session.enter("/pref avatars o").as_deref(),
            Some("/pref avatars on ")
        );
    }

    #[test]
    fn up_and_down_wrap_around_the_rows() {
        let mut session = SlashSession::new();
        session.sync("/pref ", Surface::Conversation, false);
        assert_eq!(session.selected(), 0);
        session.move_selection(-1);
        assert_eq!(session.selected(), 3);
        session.move_selection(1);
        assert_eq!(session.selected(), 0);
        session.move_selection(1);
        session.move_selection(1);
        assert_eq!(session.selected(), 2);
    }

    #[test]
    fn select_clamps_and_a_click_inserts_that_row() {
        let mut session = SlashSession::new();
        session.sync("/pref ", Surface::Conversation, false);
        session.select(99);
        assert_eq!(session.selected(), 3);
        assert_eq!(
            session.activate(1, "/pref ").as_deref(),
            Some("/pref avatars ")
        );
        assert_eq!(session.selected(), 1);
    }

    #[test]
    fn a_closed_popup_ignores_keys_and_falls_through() {
        let mut session = SlashSession::new();
        session.sync("hello", Surface::Conversation, false);
        assert!(!session.is_open());
        session.move_selection(1);
        session.select(3);
        assert_eq!(session.tab("hello"), None);
        assert_eq!(session.enter("hello"), None);
        assert_eq!(session.activate(0, "hello"), None);
        assert!(!session.dismiss(), "Escape is not consumed when closed");
    }

    #[test]
    fn escape_closes_and_keeps_the_text_until_the_needle_changes() {
        let mut session = SlashSession::new();
        session.sync("/v", Surface::Conversation, false);
        assert!(session.dismiss());
        assert!(!session.is_open());

        session.sync("/v", Surface::Conversation, false);
        assert!(!session.is_open(), "the same needle stays shut");

        session.sync("/ve", Surface::Conversation, false);
        assert!(session.is_open(), "a new needle reopens the popup");
    }

    #[test]
    fn editing_to_a_new_needle_resets_the_highlight_but_keeping_it_is_kept() {
        let mut session = SlashSession::new();
        session.sync("/pref ", Surface::Conversation, false);
        session.select(2);
        session.sync("/pref ", Surface::Conversation, false);
        assert_eq!(session.selected(), 2, "the same needle keeps the highlight");
        session.sync("/pref a", Surface::Conversation, false);
        assert_eq!(session.selected(), 0, "a new needle starts at the top");
    }

    #[test]
    fn walking_history_keeps_the_popup_closed() {
        let mut session = SlashSession::new();
        session.sync("/v", Surface::Conversation, true);
        assert!(!session.is_open());
        session.sync("/v", Surface::Conversation, false);
        assert!(session.is_open());
    }

    #[test]
    fn a_status_session_never_offers_me() {
        let mut session = SlashSession::new();
        session.sync("/", Surface::Status, false);
        assert!(!session.is_open());
        session.sync("/m", Surface::Status, false);
        assert!(!session.is_open());
    }
}
