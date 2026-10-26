//! Find mode (Ctrl+F) in the shell. The composer holds the query and the
//! transcript shows its match. Ports `beginOrAdvanceFind`, `advanceFind`,
//! `revealFindMatch` and `leaveFind` in `client/src/ui/shell`. The search itself
//! is `model::find`.
//!
//! Find mode starts in `begin_or_advance_find` and ends in `leave_find` (Escape)
//! or `end_find_mode` (a change of conversation or Status). Those changes stash
//! the composer first, because the field holds the query, not the draft, while
//! find is on.

use gpui::{Context, Window};

use crate::model::chords::Chord;
use crate::model::find::Find;
use crate::ui::shell::Shell;

impl Shell {
    /// Ctrl+F, and Enter while find mode is on. The first press saves the draft,
    /// selects the field as the query and searches from the top. Later presses
    /// move to the next match, wrapping round. The composer keeps focus.
    pub fn begin_or_advance_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::Find, window, cx) {
            return;
        }
        self.focus_composer(window, cx);
        if self.find.is_some() {
            self.search_find(false, cx);
        } else {
            self.start_find(cx);
        }
        cx.notify();
    }

    /// Escape with find mode on and no sheet open: leaves find mode, restores the
    /// draft and returns focus to the composer (Qt `leaveFind`). Returns false when
    /// find was not on.
    pub fn leave_find(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(draft) = self.end_find_mode(cx) else {
            return false;
        };
        self.composer.update(cx, |c, cx| {
            c.set_text(draft);
            cx.notify();
        });
        self.focus_composer(window, cx);
        cx.notify();
        true
    }

    /// Turns find mode off: the match highlight clears and the field loses its find
    /// look. Returns the draft that find saved, or `None` when find was not on.
    /// Callers that change the target stash the composer first, because stashing
    /// while find is on saves the saved draft and not the query.
    pub fn end_find_mode(&mut self, cx: &mut Context<Self>) -> Option<String> {
        self.find.take()?;
        self.transcript.update(cx, |t, cx| {
            t.set_find_row(None);
            cx.notify();
        });
        self.composer.update(cx, |c, cx| c.set_find_mode(false, cx));
        self.composer_state.end_find()
    }

    /// Typing in find mode searches again from the top, as Qt's `handleComposerText`
    /// does. Called on every composer change. Only a query that differs from the
    /// last one searches, so moving the caret or the match does not.
    pub fn follow_find_query(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().to_string();
        let changed = self.find.as_ref().is_some_and(|find| find.query() != text);
        if changed {
            self.search_find(true, cx);
        }
    }

    /// Enters find mode. The draft is saved, the slash popup shuts, and the field
    /// selects its text as the query before the first search from the top.
    fn start_find(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().to_string();
        self.composer_state.begin_find(&text);
        self.find = Some(Find::new());
        self.slash.close();
        self.composer.update(cx, |c, cx| {
            c.set_find_mode(true, cx);
            c.select_all_text(cx);
        });
        self.search_find(true, cx);
    }

    /// Runs the search for the field's text. `from_start` searches from the top,
    /// otherwise after the current match. A match is revealed at the top of the
    /// list, and the highlight moves with it. No match keeps the old highlight.
    pub fn search_find(&mut self, from_start: bool, cx: &mut Context<Self>) {
        let query = self.composer.read(cx).text().to_string();
        let status_view = self.status_showing();
        let (reveal, current) = {
            let rows = self.transcript.read(cx).rows();
            let Some(find) = self.find.as_mut() else {
                return;
            };
            let reveal = find.advance(rows, status_view, &query, from_start);
            (reveal, find.current())
        };
        self.transcript.update(cx, |t, cx| {
            t.set_find_row(current);
            if let Some(row) = reveal {
                t.scroll_to_row(row, cx);
            }
            cx.notify();
        });
    }
}
