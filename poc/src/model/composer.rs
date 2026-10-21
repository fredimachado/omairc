//! The composer's memory and keyboard helpers: a draft per target, the sent
//! lines of each target for Up/Down, and Tab nick completion. Ports
//! `stashComposerDraft`, `restoreComposerDraft`, `recallComposerHistory`,
//! `rememberSentComposerLine`, `nickCompleteCandidates`, `nickMatchesForPrefix`
//! and `completeNick` in OmaircWindow.qml.
//!
//! Toolkit-free: the shell owns the text field and applies the `Edit`s these
//! functions return. The field's text and caret are passed in, never read here.

use std::collections::{HashMap, HashSet};

use super::ids::ConversationRef;

/// Sent lines kept per target (Qt `composerHistories`, capped at 50).
pub const HISTORY_LIMIT: usize = 50;

/// The target a draft and its history belong to: a conversation, or one
/// network's Status pane. Status keys by network, as Qt's `status\n<network>`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DraftKey {
    Conversation(ConversationRef),
    Status(String),
}

impl DraftKey {
    /// The key for what is on screen: Status for its network while Status
    /// shows, otherwise the selected conversation.
    pub fn for_view(status: Option<&str>, selected: &ConversationRef) -> Self {
        match status {
            Some(network_id) => Self::Status(network_id.to_string()),
            None => Self::Conversation(selected.clone()),
        }
    }
}

/// The text and caret the field should show after a composer action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub text: String,
    /// Byte offset of the caret in `text`. Always on a character boundary.
    pub cursor: usize,
}

/// The nicks a Tab can complete for what is on screen.
pub enum NickScope<'a> {
    /// Status has no nicks, and so no completion.
    Status,
    /// A direct message: its peer, then the network's channel names.
    Direct {
        peer: &'a str,
        channels: &'a [&'a str],
    },
    /// A channel: its members in panel order, then the network's channel names.
    Channel {
        members: &'a [&'a str],
        channels: &'a [&'a str],
    },
}

/// The candidate list for Tab, in Qt's `nickCompleteCandidates` order. Empty
/// names and repeats drop out, and the first occurrence keeps its place.
pub fn nick_candidates(scope: NickScope) -> Vec<String> {
    let (people, channels): (Vec<&str>, &[&str]) = match scope {
        NickScope::Status => return Vec::new(),
        NickScope::Direct { peer, channels } => (vec![peer], channels),
        NickScope::Channel { members, channels } => (members.to_vec(), channels),
    };
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    for name in people.into_iter().chain(channels.iter().copied()) {
        if !name.is_empty() && seen.insert(name) {
            names.push(name.to_string());
        }
    }
    names
}

/// The candidates that start with `prefix`, ignoring case, sorted by name.
/// Sorting is a plain string sort, as Qt's `nickMatchesForPrefix` does, so
/// "Alan" comes before "alice".
pub fn nick_matches_for_prefix(prefix: &str, candidates: &[String]) -> Vec<String> {
    let lower = prefix.to_lowercase();
    let mut hits: Vec<(usize, &String)> = candidates
        .iter()
        .enumerate()
        .filter(|(_, nick)| nick.to_lowercase().starts_with(&lower))
        .collect();
    hits.sort_by(|(left_order, left), (right_order, right)| {
        left.cmp(right).then(left_order.cmp(right_order))
    });
    hits.into_iter().map(|(_, nick)| nick.clone()).collect()
}

/// Everything the composer remembers between targets.
#[derive(Debug, Default)]
pub struct ComposerState {
    /// The target the field currently holds a draft for.
    key: Option<DraftKey>,
    drafts: HashMap<DraftKey, String>,
    histories: HashMap<DraftKey, Vec<String>>,
    /// An Up/Down walk in progress.
    browse: Option<Browse>,
    /// A Tab cycle in progress.
    completion: Option<Completion>,
    /// The draft Ctrl+F saved when find mode began. While find is active the
    /// field holds the query, so this copy is the unsent text.
    find: Option<String>,
}

/// An Up/Down walk: the history index on screen, and the draft typed before
/// the walk began. The index equal to the history length is that draft.
#[derive(Debug)]
struct Browse {
    index: usize,
    draft: String,
}

/// A Tab cycle: the candidates for the token that starts at `origin`.
#[derive(Debug)]
struct Completion {
    /// Byte offset where the completed token starts.
    origin: usize,
    matches: Vec<String>,
    index: usize,
    /// The field's input serial when the cycle began. An edit or caret move
    /// since then ends the cycle.
    serial: u64,
}

impl ComposerState {
    /// A state whose field shows `key`'s draft, which is empty at launch.
    pub fn starting_at(key: DraftKey) -> Self {
        Self {
            key: Some(key),
            ..Self::default()
        }
    }

    /// The text the user has typed and not sent. While Up/Down walks history
    /// that is the draft saved when the walk began, not the recalled line.
    ///
    /// Every stash goes through here. While find is active, the field holds the
    /// query, so the draft Ctrl+F saved is the unsent text.
    pub fn unsent_text(&self, composer_text: &str) -> String {
        if let Some(draft) = &self.find {
            return draft.clone();
        }
        match &self.browse {
            Some(browse) => browse.draft.clone(),
            None => composer_text.to_string(),
        }
    }

    /// Enters find mode for the field's current text. Its unsent text is saved
    /// and comes back from `end_find`. An Up/Down walk ends, since the field
    /// now holds the query and not a recalled line.
    pub fn begin_find(&mut self, composer_text: &str) {
        let draft = self.unsent_text(composer_text);
        self.browse = None;
        self.completion = None;
        self.find = Some(draft);
    }

    /// Leaves find mode and returns the draft it saved, or `None` when find was
    /// not active. The caller puts that text back in the field.
    pub fn end_find(&mut self) -> Option<String> {
        self.find.take()
    }

    /// Saves the unsent text under the current target. Call this before the
    /// target changes, and before the send limit clamps the field to the next one.
    pub fn stash(&mut self, composer_text: &str) {
        let Some(key) = self.key.clone() else {
            return;
        };
        let unsent = self.unsent_text(composer_text);
        if unsent.is_empty() {
            self.drafts.remove(&key);
        } else {
            self.drafts.insert(key, unsent);
        }
    }

    /// Whether Up or Down is walking history right now. The slash popup stays
    /// closed meanwhile, even when a recalled line starts with a slash.
    pub fn is_browsing(&self) -> bool {
        self.browse.is_some()
    }

    /// Whether the field's draft already belongs to `key`. Re-selecting the
    /// target on screen is not a switch, so the caret and any walk stay put.
    #[cfg(test)]
    pub fn is_showing(&self, key: &DraftKey) -> bool {
        self.key.as_ref() == Some(key)
    }

    /// Makes `key` the target and returns the draft it holds, empty if none.
    /// Ends any history walk, Tab cycle or find. The saved find draft is
    /// dropped, so the caller stashes before it switches.
    pub fn switch_to(&mut self, key: DraftKey) -> String {
        self.browse = None;
        self.completion = None;
        self.find = None;
        let text = self.drafts.get(&key).cloned().unwrap_or_default();
        self.key = Some(key);
        text
    }

    /// Records a line sent to the current target. The line joins that target's
    /// history, the stored draft clears, and any walk or cycle ends.
    pub fn record_sent(&mut self, line: &str) {
        self.browse = None;
        self.completion = None;
        let Some(key) = self.key.clone() else {
            return;
        };
        self.drafts.remove(&key);
        let lines = self.histories.entry(key).or_default();
        lines.push(line.to_string());
        if lines.len() > HISTORY_LIMIT {
            let excess = lines.len() - HISTORY_LIMIT;
            lines.drain(..excess);
        }
    }

    /// Up (`delta` -1) or Down (`delta` +1) in the composer, as
    /// `recallComposerHistory`. Returns the text to show, with the caret at its
    /// end. `None` means the key falls through: there is no history to walk,
    /// or Down comes before any Up.
    pub fn recall(&mut self, composer_text: &str, delta: isize) -> Option<String> {
        self.completion = None;
        let len = self.history().len();
        let (index, draft) = match self.browse.take() {
            Some(browse) => (browse.index, browse.draft),
            None => {
                if delta > 0 || len == 0 {
                    return None;
                }
                (len, composer_text.to_string())
            }
        };
        let next = (index as isize + delta).max(0) as usize;
        if next >= len {
            // Down past the newest line brings the typed draft back, and the walk ends.
            return Some(draft);
        }
        let line = self.history()[next].clone();
        self.browse = Some(Browse { index: next, draft });
        Some(line)
    }

    /// Tab (`backward` false) or Shift+Tab (`backward` true) in the composer,
    /// as `completeNick`. `text` and `cursor` are the field's, and `serial` is
    /// its input serial. `candidates` come from `nick_candidates`.
    ///
    /// The first press matches the token before the caret. Repeats cycle
    /// through the matches while `serial` is unchanged, and any edit or caret
    /// move starts over. A token at the start of the line takes `: `, and any
    /// later token takes a space. `None` means nothing to insert.
    pub fn complete_nick(
        &mut self,
        text: &str,
        cursor: usize,
        serial: u64,
        candidates: &[String],
        backward: bool,
    ) -> Option<Edit> {
        if !text.is_char_boundary(cursor) {
            self.completion = None;
            return None;
        }
        if let Some(completion) = self.completion.as_mut() {
            let origin_ok = completion.origin <= cursor && text.is_char_boundary(completion.origin);
            if completion.serial == serial && origin_ok && !completion.matches.is_empty() {
                let count = completion.matches.len();
                completion.index = if backward {
                    (completion.index + count - 1) % count
                } else {
                    (completion.index + 1) % count
                };
                let nick = completion.matches[completion.index].clone();
                return Some(insert_nick(text, completion.origin, cursor, &nick));
            }
        }

        self.completion = None;
        let origin = text[..cursor].rfind(' ').map_or(0, |space| space + 1);
        let token = &text[origin..cursor];
        if token.is_empty() {
            return None;
        }
        let matches = nick_matches_for_prefix(token, candidates);
        if matches.is_empty() {
            return None;
        }
        let index = if backward { matches.len() - 1 } else { 0 };
        let edit = insert_nick(text, origin, cursor, &matches[index]);
        self.completion = Some(Completion {
            origin,
            matches,
            index,
            serial,
        });
        Some(edit)
    }

    /// The sent lines of the current target, oldest first.
    fn history(&self) -> &[String] {
        match self.key.as_ref().and_then(|key| self.histories.get(key)) {
            Some(lines) => lines.as_slice(),
            None => &[],
        }
    }
}

/// Replaces the token from `origin` to `cursor` with `nick` and its suffix,
/// and keeps the text after the caret.
fn insert_nick(text: &str, origin: usize, cursor: usize, nick: &str) -> Edit {
    let suffix = if origin == 0 { ": " } else { " " };
    let insertion = format!("{nick}{suffix}");
    let end = origin + insertion.len();
    Edit {
        text: format!("{}{}{}", &text[..origin], insertion, &text[cursor..]),
        cursor: end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conversation(key: &str) -> DraftKey {
        DraftKey::Conversation(ConversationRef::new("omarchy", key))
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    /// Runs one Tab press with the field's serial fixed at `serial`.
    fn tab(
        state: &mut ComposerState,
        text: &str,
        cursor: usize,
        serial: u64,
        candidates: &[String],
        backward: bool,
    ) -> Option<Edit> {
        state.complete_nick(text, cursor, serial, candidates, backward)
    }

    #[test]
    fn a_key_is_the_conversation_or_the_status_of_the_open_view() {
        let selected = ConversationRef::new("omarchy", "#omarchy");
        assert_eq!(
            DraftKey::for_view(None, &selected),
            DraftKey::Conversation(selected.clone())
        );
        assert_eq!(
            DraftKey::for_view(Some("oftc"), &selected),
            DraftKey::Status("oftc".to_string())
        );
    }

    #[test]
    fn up_recalls_sent_lines_newest_first_and_stops_at_the_oldest() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.record_sent("one");
        state.record_sent("two");
        state.record_sent("three");

        assert_eq!(state.recall("", -1).as_deref(), Some("three"));
        assert_eq!(state.recall("three", -1).as_deref(), Some("two"));
        assert_eq!(state.recall("two", -1).as_deref(), Some("one"));
        assert_eq!(state.recall("one", -1).as_deref(), Some("one"));
    }

    #[test]
    fn down_past_the_newest_line_restores_the_typed_draft() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.record_sent("one");
        state.record_sent("two");

        assert_eq!(state.recall("half typed", -1).as_deref(), Some("two"));
        assert_eq!(state.recall("two", 1).as_deref(), Some("half typed"));
        // The walk has ended, so another Down has nothing to do.
        assert_eq!(state.recall("half typed", 1), None);
    }

    #[test]
    fn down_before_any_up_falls_through() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.record_sent("one");
        assert_eq!(state.recall("", 1), None);
        assert_eq!(state.recall("typed", 1), None);
    }

    #[test]
    fn keys_fall_through_when_there_is_no_history() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        assert_eq!(state.recall("", -1), None);
        assert_eq!(state.recall("typed", -1), None);
        assert_eq!(state.recall("", 1), None);
    }

    #[test]
    fn history_belongs_to_its_target() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.record_sent("omarchy line");
        assert_eq!(state.switch_to(conversation("anna")), "");
        assert_eq!(state.recall("", -1), None);

        state.record_sent("anna line");
        assert_eq!(state.switch_to(conversation("#omarchy")), "");
        assert_eq!(state.recall("", -1).as_deref(), Some("omarchy line"));
    }

    #[test]
    fn status_keeps_its_own_history_apart_from_conversations() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.record_sent("channel line");
        state.switch_to(DraftKey::Status("omarchy".to_string()));
        assert_eq!(state.recall("", -1), None);
    }

    #[test]
    fn history_keeps_the_last_fifty_lines() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        for n in 0..55 {
            state.record_sent(&format!("line {n}"));
        }
        let mut shown = state.recall("", -1).expect("history has lines");
        assert_eq!(shown, "line 54");
        for _ in 0..49 {
            shown = state.recall(&shown, -1).expect("still walking");
        }
        assert_eq!(shown, "line 5", "the five oldest lines fell off the end");
        assert_eq!(state.recall(&shown, -1).as_deref(), Some("line 5"));
    }

    #[test]
    fn reselecting_the_target_on_screen_is_not_a_switch() {
        let state = ComposerState::starting_at(conversation("#omarchy"));
        assert!(state.is_showing(&conversation("#omarchy")));
        assert!(!state.is_showing(&conversation("anna")));
        assert!(!state.is_showing(&DraftKey::Status("omarchy".to_string())));
    }

    #[test]
    fn the_draft_comes_back_per_target_when_switching() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.stash("draft for omarchy");
        assert_eq!(state.switch_to(conversation("anna")), "");
        state.stash("draft for anna");
        assert_eq!(state.switch_to(conversation("#omarchy")), "draft for omarchy");
        assert_eq!(state.switch_to(conversation("anna")), "draft for anna");
    }

    #[test]
    fn clearing_the_composer_forgets_the_draft() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.stash("typed");
        state.switch_to(conversation("anna"));
        state.stash("");
        assert_eq!(state.switch_to(conversation("#omarchy")), "typed");
        state.stash("");
        assert_eq!(state.switch_to(conversation("#omarchy")), "");
    }

    #[test]
    fn stashing_during_a_walk_keeps_the_typed_draft_not_the_recalled_line() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.record_sent("sent");

        assert_eq!(state.recall("typed", -1).as_deref(), Some("sent"));
        assert_eq!(state.unsent_text("sent"), "typed");
        state.stash("sent");
        state.switch_to(conversation("anna"));
        assert_eq!(state.switch_to(conversation("#omarchy")), "typed");
    }

    #[test]
    fn find_gives_back_the_draft_it_saved_not_the_query() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.begin_find("half typed");
        assert_eq!(state.unsent_text("query"), "half typed");

        // A stash while find is on keeps the saved draft for this target.
        state.stash("query");
        assert_eq!(state.end_find().as_deref(), Some("half typed"));
        assert_eq!(state.end_find(), None, "find is off once it has ended");
        assert_eq!(state.unsent_text("typed after"), "typed after");
    }

    #[test]
    fn the_saved_find_draft_is_stored_when_the_target_changes() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.begin_find("draft");
        state.stash("query");
        assert_eq!(state.switch_to(conversation("anna")), "");
        assert_eq!(state.end_find(), None, "a switch ends find");
        assert_eq!(state.switch_to(conversation("#omarchy")), "draft");
    }

    #[test]
    fn beginning_find_during_a_walk_saves_the_typed_draft() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.record_sent("sent");
        assert_eq!(state.recall("typed", -1).as_deref(), Some("sent"));

        state.begin_find("sent");
        assert!(!state.is_browsing(), "find ends the walk");
        assert_eq!(state.end_find().as_deref(), Some("typed"));
    }

    #[test]
    fn end_find_without_find_returns_nothing() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        assert_eq!(state.end_find(), None);
    }

    #[test]
    fn is_browsing_is_true_only_while_a_walk_is_in_progress() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.record_sent("one");
        assert!(!state.is_browsing());
        state.recall("typed", -1);
        assert!(state.is_browsing());
        state.recall("", 1);
        assert!(!state.is_browsing(), "Down past the newest line ends the walk");
    }

    #[test]
    fn switching_ends_a_walk() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.record_sent("one");
        assert_eq!(state.recall("typed", -1).as_deref(), Some("one"));

        state.switch_to(conversation("anna"));
        state.switch_to(conversation("#omarchy"));
        assert_eq!(state.recall("", 1), None, "no walk is left to step down");
    }

    #[test]
    fn sending_ends_a_walk_and_clears_the_stored_draft() {
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.record_sent("one");
        state.record_sent("two");
        assert_eq!(state.recall("", -1).as_deref(), Some("two"));

        state.record_sent("two");
        assert_eq!(state.recall("", 1), None);

        state.stash("half typed");
        state.record_sent("half typed");
        state.switch_to(conversation("anna"));
        assert_eq!(state.switch_to(conversation("#omarchy")), "");
    }

    #[test]
    fn candidates_put_the_members_first_then_channel_names_without_repeats() {
        let members = ["fred", "anna", "fred"];
        let channels = ["#omarchy", "#linux"];
        let scope = NickScope::Channel {
            members: &members,
            channels: &channels,
        };
        assert_eq!(
            nick_candidates(scope),
            names(&["fred", "anna", "#omarchy", "#linux"])
        );
    }

    #[test]
    fn a_direct_message_completes_its_peer_and_the_channels() {
        let channels = ["#omarchy"];
        let scope = NickScope::Direct {
            peer: "anna",
            channels: &channels,
        };
        assert_eq!(nick_candidates(scope), names(&["anna", "#omarchy"]));
    }

    #[test]
    fn status_has_no_candidates_so_tab_does_nothing() {
        assert!(nick_candidates(NickScope::Status).is_empty());
        let mut state = ComposerState::starting_at(DraftKey::Status("omarchy".to_string()));
        assert_eq!(tab(&mut state, "al", 2, 0, &[], false), None);
    }

    #[test]
    fn prefix_matches_ignore_case_and_sort_by_name() {
        let candidates = names(&["alice", "Alan", "bob", "Alex"]);
        assert_eq!(
            nick_matches_for_prefix("a", &candidates),
            names(&["Alan", "Alex", "alice"])
        );
        assert_eq!(nick_matches_for_prefix("AL", &candidates), names(&["Alan", "Alex", "alice"]));
        assert!(nick_matches_for_prefix("z", &candidates).is_empty());
    }

    #[test]
    fn tab_completes_the_first_match_with_a_colon_at_the_start_of_the_line() {
        let candidates = names(&["alice", "Alan", "bob", "Alex"]);
        let mut state = ComposerState::starting_at(conversation("#omarchy"));

        let first = tab(&mut state, "a", 1, 0, &candidates, false).expect("a matches");
        assert_eq!(first, Edit { text: "Alan: ".into(), cursor: 6 });
    }

    #[test]
    fn repeated_tab_cycles_through_the_matches_and_wraps() {
        let candidates = names(&["alice", "Alan", "bob", "Alex"]);
        let mut state = ComposerState::starting_at(conversation("#omarchy"));

        let mut edit = tab(&mut state, "a", 1, 0, &candidates, false).expect("a matches");
        let mut shown = Vec::new();
        for _ in 0..4 {
            shown.push(edit.text.clone());
            edit = tab(&mut state, &edit.text, edit.cursor, 0, &candidates, false)
                .expect("cycle continues");
        }
        assert_eq!(shown, ["Alan: ", "Alex: ", "alice: ", "Alan: "]);
        assert_eq!(edit.text, "Alex: ");
    }

    #[test]
    fn shift_tab_starts_at_the_last_match_and_walks_back() {
        let candidates = names(&["alice", "Alan", "Alex"]);
        let mut state = ComposerState::starting_at(conversation("#omarchy"));

        let last = tab(&mut state, "a", 1, 0, &candidates, true).expect("a matches");
        assert_eq!(last.text, "alice: ");
        let before = tab(&mut state, &last.text, last.cursor, 0, &candidates, true)
            .expect("cycle continues");
        assert_eq!(before.text, "Alex: ");
    }

    #[test]
    fn a_later_token_takes_a_space_instead_of_a_colon() {
        let candidates = names(&["Alan", "alice"]);
        let mut state = ComposerState::starting_at(conversation("#omarchy"));

        let first = tab(&mut state, "hi al", 5, 0, &candidates, false).expect("al matches");
        assert_eq!(first, Edit { text: "hi Alan ".into(), cursor: 8 });

        let second = tab(&mut state, &first.text, first.cursor, 0, &candidates, false)
            .expect("cycle continues");
        assert_eq!(second, Edit { text: "hi alice ".into(), cursor: 9 });
    }

    #[test]
    fn completion_keeps_the_text_after_the_caret() {
        let candidates = names(&["Alan"]);
        let mut state = ComposerState::starting_at(conversation("#omarchy"));

        let edit = tab(&mut state, "hi alice", 5, 0, &candidates, false).expect("al matches");
        assert_eq!(edit, Edit { text: "hi Alan ice".into(), cursor: 8 });
    }

    #[test]
    fn an_edit_or_caret_move_restarts_the_cycle_from_the_current_token() {
        let candidates = names(&["alan", "alice", "bob"]);
        let mut state = ComposerState::starting_at(conversation("#omarchy"));

        let first = tab(&mut state, "a", 1, 0, &candidates, false).expect("a matches");
        assert_eq!(first.text, "alan: ");

        // The user types "b" after the completion, so the serial moves on.
        let typed = format!("{}b", first.text);
        let restarted = tab(&mut state, &typed, typed.len(), 1, &candidates, false)
            .expect("b matches bob");
        assert_eq!(restarted, Edit { text: "alan: bob ".into(), cursor: 10 });
    }

    #[test]
    fn a_caret_move_after_completion_with_no_new_token_does_nothing() {
        let candidates = names(&["alan", "alice"]);
        let mut state = ComposerState::starting_at(conversation("#omarchy"));

        let first = tab(&mut state, "a", 1, 0, &candidates, false).expect("a matches");
        // Same text, but the caret moved, so the cycle is stale and the token is empty.
        assert_eq!(tab(&mut state, &first.text, first.cursor, 1, &candidates, false), None);
    }

    #[test]
    fn no_token_or_no_match_inserts_nothing() {
        let candidates = names(&["alan"]);
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        assert_eq!(tab(&mut state, "", 0, 0, &candidates, false), None);
        assert_eq!(tab(&mut state, "hi ", 3, 0, &candidates, false), None);
        assert_eq!(tab(&mut state, "zz", 2, 0, &candidates, false), None);
    }

    #[test]
    fn tab_during_a_walk_completes_the_recalled_line_and_the_walk_goes_on() {
        let candidates = names(&["alan"]);
        let mut state = ComposerState::starting_at(conversation("#omarchy"));
        state.record_sent("ping al");
        assert_eq!(state.recall("", -1).as_deref(), Some("ping al"));

        let completed = tab(&mut state, "ping al", 7, 0, &candidates, false)
            .expect("al matches alan");
        assert_eq!(completed, Edit { text: "ping alan ".into(), cursor: 10 });

        // Down still ends the walk with the draft typed before it began.
        assert_eq!(state.recall(&completed.text, 1).as_deref(), Some(""));
    }
}
