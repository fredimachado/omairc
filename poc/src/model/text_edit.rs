//! Toolkit-free text editing for the single-line fields (the composer and the
//! sheet filters): word boundaries and word movement, undo and redo, and
//! horizontal scroll for long single-line drafts.
//! Every offset is a byte offset into the text, and every result lands on a
//! grapheme boundary.

use std::ops::Range;

use unicode_segmentation::GraphemeCursor;

/// Snaps `offset` back to the start of the grapheme it falls inside. Offsets
/// past the end clamp to the end. An offset inside a multi-byte character is
/// first floored to that character's start, because the grapheme cursor
/// panics on those.
pub fn snap_grapheme(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    if offset == 0 || text.is_empty() {
        return 0;
    }
    if offset >= text.len() {
        return text.len();
    }
    let mut cursor = GraphemeCursor::new(offset, text.len(), true);
    if cursor.is_boundary(text, 0).unwrap_or(false) {
        return offset;
    }
    cursor.prev_boundary(text, 0).ok().flatten().unwrap_or(0)
}

/// The end of the grapheme that starts at `offset`, or the text length.
pub fn next_grapheme_end(text: &str, offset: usize) -> usize {
    let mut cursor = GraphemeCursor::new(offset, text.len(), true);
    cursor
        .next_boundary(text, 0)
        .ok()
        .flatten()
        .unwrap_or(text.len())
}

/// Letters, digits, `_` and `-` make up a word, as they do for the transcript's
/// double-click selection.
pub fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-'
}

/// The word that `offset` touches, as byte offsets. Off a word the range is
/// empty at `offset`.
pub fn word_bounds(text: &str, offset: usize) -> (usize, usize) {
    let offset = snap_grapheme(text, offset.min(text.len()));
    if text.is_empty() {
        return (0, 0);
    }
    let mut start = offset;
    while start > 0 {
        let prev = snap_grapheme(text, start - 1);
        if prev == start {
            break;
        }
        let ch = text[prev..start].chars().next().unwrap();
        if is_word_char(ch) {
            start = prev;
        } else {
            break;
        }
    }
    let mut end = offset;
    while end < text.len() {
        let next = next_grapheme_end(text, end);
        let ch = text[end..next].chars().next().unwrap_or(' ');
        if is_word_char(ch) {
            end = next;
        } else {
            break;
        }
    }
    (start, end)
}

/// Ctrl+Left: the start of the word before `offset`. Separators between the
/// cursor and that word are skipped, so from inside a word it lands on that
/// word's own start. At the start of the text it stays at 0.
pub fn previous_word_start(text: &str, offset: usize) -> usize {
    let mut pos = snap_grapheme(text, offset);
    while pos > 0 && !grapheme_before_is_word(text, pos) {
        pos = snap_grapheme(text, pos - 1);
    }
    if pos == 0 {
        0
    } else {
        word_bounds(text, pos).0
    }
}

/// Ctrl+Right: the end of the word at or after `offset`. Separators are skipped
/// first, so from a gap it lands on the end of the next word. With no word left
/// it returns the text length.
pub fn next_word_end(text: &str, offset: usize) -> usize {
    let mut pos = snap_grapheme(text, offset);
    while pos < text.len() && !grapheme_at_is_word(text, pos) {
        pos = next_grapheme_end(text, pos);
    }
    if pos >= text.len() {
        text.len()
    } else {
        word_bounds(text, pos).1
    }
}

fn grapheme_before_is_word(text: &str, pos: usize) -> bool {
    let start = snap_grapheme(text, pos - 1);
    text[start..pos].chars().next().is_some_and(is_word_char)
}

fn grapheme_at_is_word(text: &str, pos: usize) -> bool {
    let end = next_grapheme_end(text, pos);
    text[pos..end].chars().next().is_some_and(is_word_char)
}

/// The text and selection an undo step restores. `selected` is a byte range
/// into `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditSnapshot {
    pub text: String,
    pub selected: Range<usize>,
}

/// Undo and redo for a line field. Consecutive typed characters at the same
/// place share one step, so Ctrl+Z after typing a word removes the word. Every
/// other edit is a step of its own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UndoStack {
    undo_steps: Vec<EditSnapshot>,
    redo_steps: Vec<EditSnapshot>,
    /// Where the last typed character ended. The next typed character joins the
    /// open step only when it lands here.
    typing_end: Option<usize>,
}

impl UndoStack {
    /// Notes an edit that replaces `range` with `inserted`. Call it before the
    /// edit is applied, passing the text and selection as they were.
    pub fn record(&mut self, before: EditSnapshot, range: Range<usize>, inserted: &str) {
        let typed = range.is_empty() && inserted.chars().count() == 1;
        let joins = typed && self.typing_end == Some(range.start);
        if !joins {
            self.undo_steps.push(before);
            self.redo_steps.clear();
        }
        self.typing_end = typed.then(|| range.start + inserted.len());
    }

    /// Takes the prior snapshot to restore, or `None` when there is nothing to
    /// undo. `current` is pushed onto the redo stack.
    pub fn undo(&mut self, current: EditSnapshot) -> Option<EditSnapshot> {
        self.typing_end = None;
        let Some(prev) = self.undo_steps.pop() else {
            return None;
        };
        self.redo_steps.push(current);
        Some(prev)
    }

    /// Restores a snapshot from redo, or `None` when the redo stack is empty.
    /// `current` is pushed back onto undo.
    pub fn redo(&mut self, current: EditSnapshot) -> Option<EditSnapshot> {
        self.typing_end = None;
        let Some(next) = self.redo_steps.pop() else {
            return None;
        };
        self.undo_steps.push(current);
        Some(next)
    }

    /// Forgets every step. Used after a change that did not come from typing.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Keeps `focus_start_x`..`focus_end_x` visible inside a viewport of
/// `viewport_width` when `line_width` exceeds it.
pub fn horizontal_scroll_offset(
    line_width: f32,
    viewport_width: f32,
    focus_start_x: f32,
    focus_end_x: f32,
    current_offset: f32,
) -> f32 {
    if line_width <= viewport_width {
        return 0.;
    }
    let max_offset = line_width - viewport_width;
    let mut offset = current_offset;
    let visible_start = focus_start_x - offset;
    let visible_end = focus_end_x - offset;
    if visible_end > viewport_width {
        offset += visible_end - viewport_width;
    }
    if visible_start < 0. {
        offset += visible_start;
    }
    offset.clamp(0., max_offset)
}

/// Selection after repeated clicks at `offset`, matching the transcript's
/// double-click word and triple-click line convention. A single-line field
/// treats a third click as select-all.
pub fn click_selection(click_count: u32, text: &str, offset: usize) -> Range<usize> {
    if click_count >= 3 {
        0..text.len()
    } else if click_count == 2 {
        let (start, end) = word_bounds(text, offset);
        start..end
    } else {
        offset..offset
    }
}

/// The text covered by a byte `range` in `content`. Empty when the range is
/// empty or not a valid slice of `content`.
pub fn selection_text(content: &str, range: Range<usize>) -> &str {
    if range.is_empty() {
        ""
    } else {
        content.get(range).unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(text: &str, selected: Range<usize>) -> EditSnapshot {
        EditSnapshot {
            text: text.to_string(),
            selected,
        }
    }

    /// Applies one edit to `text` the way the input does, recording it first.
    fn edit(
        text: &mut String,
        undo: &mut UndoStack,
        range: Range<usize>,
        inserted: &str,
    ) {
        undo.record(snapshot(text, range.clone()), range.clone(), inserted);
        text.replace_range(range, inserted);
    }

    #[test]
    fn word_bounds_cover_the_word_under_the_offset() {
        assert_eq!(word_bounds("foo bar", 5), (4, 7));
        assert_eq!(word_bounds("foo bar", 4), (4, 7));
        assert_eq!(word_bounds("foo bar", 3), (0, 3));
        assert_eq!(word_bounds("foo bar", 0), (0, 3));
        assert_eq!(word_bounds("foo  bar", 4), (4, 4));
        assert_eq!(word_bounds("", 0), (0, 0));
    }

    #[test]
    fn hyphen_and_underscore_belong_to_words() {
        assert_eq!(word_bounds("a-b_c d", 1), (0, 5));
        assert_eq!(next_word_end("a-b_c d", 0), 5);
    }

    #[test]
    fn previous_word_start_walks_back_one_word_at_a_time() {
        let text = "foo bar baz";
        assert_eq!(previous_word_start(text, 11), 8);
        assert_eq!(previous_word_start(text, 8), 4);
        assert_eq!(previous_word_start(text, 4), 0);
        assert_eq!(previous_word_start(text, 0), 0);
    }

    #[test]
    fn previous_word_start_from_inside_a_word_lands_on_its_start() {
        assert_eq!(previous_word_start("foo bar baz", 6), 4);
        assert_eq!(previous_word_start("foo bar baz", 9), 8);
    }

    #[test]
    fn previous_word_start_skips_a_run_of_separators() {
        assert_eq!(previous_word_start("foo   bar", 6), 0);
        assert_eq!(previous_word_start("foo   bar", 9), 6);
        assert_eq!(previous_word_start("  ", 2), 0);
    }

    #[test]
    fn next_word_end_walks_forward_one_word_at_a_time() {
        let text = "foo bar baz";
        assert_eq!(next_word_end(text, 0), 3);
        assert_eq!(next_word_end(text, 3), 7);
        assert_eq!(next_word_end(text, 7), 11);
        assert_eq!(next_word_end(text, 11), 11);
    }

    #[test]
    fn next_word_end_from_a_gap_lands_on_the_next_words_end() {
        assert_eq!(next_word_end("foo   bar", 4), 9);
        assert_eq!(next_word_end("  ", 0), 2);
        assert_eq!(next_word_end("", 0), 0);
    }

    #[test]
    fn word_movement_respects_multi_byte_text() {
        let text = "héllo wörld";
        // "héllo" is six bytes and the space is byte 6, so "wörld" starts at 7.
        assert_eq!(next_word_end(text, 0), 6);
        assert_eq!(previous_word_start(text, text.len()), 7);
        assert_eq!(next_word_end(text, 7), text.len());
        // Offset 2 sits inside the two-byte é. It snaps to a boundary.
        assert_eq!(previous_word_start(text, 2), 0);
        assert_eq!(word_bounds(text, 3), (0, 6));
    }

    #[test]
    fn offsets_inside_a_character_do_not_panic() {
        let text = "aé b";
        for offset in 0..=text.len() + 2 {
            let _ = previous_word_start(text, offset);
            let _ = next_word_end(text, offset);
            let _ = word_bounds(text, offset);
        }
    }

    #[test]
    fn typed_characters_undo_as_one_step() {
        let mut text = String::new();
        let mut undo = UndoStack::default();
        for (at, ch) in "abc".chars().enumerate() {
            edit(&mut text, &mut undo, at..at, &ch.to_string());
        }
        assert_eq!(text, "abc");

        let step = undo.undo(snapshot("abc", 3..3)).expect("a step");
        assert_eq!(step, snapshot("", 0..0));
        assert_eq!(undo.undo(snapshot("", 0..0)), None);
    }

    #[test]
    fn a_move_or_a_deletion_starts_a_new_step() {
        let mut text = String::new();
        let mut undo = UndoStack::default();
        edit(&mut text, &mut undo, 0..0, "a");
        edit(&mut text, &mut undo, 1..1, "b");
        // Backspace removes "b" and is its own step. Its selection is the byte it removes.
        edit(&mut text, &mut undo, 1..2, "");
        assert_eq!(text, "a");

        assert_eq!(undo.undo(snapshot("a", 1..1)), Some(snapshot("ab", 1..2)));
        assert_eq!(undo.undo(snapshot("ab", 1..2)), Some(snapshot("", 0..0)));
    }

    #[test]
    fn typing_after_a_cursor_move_starts_a_new_step() {
        let mut text = String::new();
        let mut undo = UndoStack::default();
        edit(&mut text, &mut undo, 0..0, "a");
        edit(&mut text, &mut undo, 0..0, "b");
        assert_eq!(text, "ba");
        // The second "b" landed at 0, not at the typing end (1), so it stands alone.
        assert_eq!(undo.undo(snapshot("ba", 1..1)), Some(snapshot("a", 0..0)));
    }

    #[test]
    fn a_paste_is_a_step_of_its_own() {
        let mut text = String::new();
        let mut undo = UndoStack::default();
        edit(&mut text, &mut undo, 0..0, "a");
        edit(&mut text, &mut undo, 1..1, "pasted");
        assert_eq!(undo.undo(snapshot("apasted", 7..7)), Some(snapshot("a", 1..1)));
    }

    #[test]
    fn clear_forgets_the_step() {
        let mut text = String::new();
        let mut undo = UndoStack::default();
        edit(&mut text, &mut undo, 0..0, "a");
        undo.clear();
        assert_eq!(undo.undo(snapshot("a", 1..1)), None);
    }

    #[test]
    fn replacing_a_selection_is_not_typing() {
        let mut text = "abc".to_string();
        let mut undo = UndoStack::default();
        edit(&mut text, &mut undo, 0..3, "x");
        assert_eq!(undo.undo(snapshot("x", 1..1)), Some(snapshot("abc", 0..3)));
    }

    #[test]
    fn two_distinct_edits_undo_twice_then_redo() {
        let mut text = String::new();
        let mut undo = UndoStack::default();
        edit(&mut text, &mut undo, 0..0, "a");
        // Inserts at 0, not at the typing end after "a", so this is its own step.
        edit(&mut text, &mut undo, 0..0, "b");
        assert_eq!(text, "ba");

        assert_eq!(undo.undo(snapshot("ba", 1..1)), Some(snapshot("a", 0..0)));
        assert_eq!(undo.redo(snapshot("a", 0..0)), Some(snapshot("ba", 1..1)));
    }

    #[test]
    fn long_line_scrolls_when_caret_passes_viewport() {
        let viewport = 100.;
        let line = 300.;
        let caret = 250.;
        let offset = horizontal_scroll_offset(line, viewport, caret, caret, 0.);
        assert!(offset > 0., "caret past the viewport must scroll");
        assert!(caret - offset <= viewport);
    }

    #[test]
    fn double_click_selects_the_word_under_the_offset() {
        assert_eq!(click_selection(2, "foo bar", 5), 4..7);
        assert_eq!(click_selection(1, "foo bar", 5), 5..5);
        assert_eq!(click_selection(3, "foo bar", 5), 0..7);
    }

    #[test]
    fn selection_text_returns_the_covered_slice() {
        assert_eq!(selection_text("hello", 1..4), "ell");
        assert_eq!(selection_text("hello", 2..2), "");
        assert_eq!(selection_text("hello", 0..5), "hello");
        assert_eq!(selection_text("aéx", 1..3), "é");
        assert_eq!(selection_text("hello", 3..99), "");
    }
}
