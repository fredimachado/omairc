use std::ops::Range;
use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId, IntoElement, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine,
    SharedString, Style, TextRun, UTF16Selection, Window, actions, div, fill, point, prelude::*,
    px, relative, size,
};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::model::byte_budget::SendLimit;
use crate::model::text_edit::{
    next_word_end, previous_word_start, snap_grapheme, EditSnapshot, UndoStack,
};
use crate::util::{to_gpui, FONT_FAMILY};

actions!(
    single_line_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        SelectHome,
        SelectEnd,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        DeleteWordBackward,
        DeleteWordForward,
        Undo,
        Paste,
        Cut,
        Copy,
    ]
);

/// Half of a caret blink cycle: solid for this long, then hidden for as long.
const CARET_HALF_PERIOD: Duration = Duration::from_millis(530);

/// Whether the caret is solid `since_edit` after the last edit or cursor move.
fn solid_at(since_edit: Duration) -> bool {
    (since_edit.as_millis() / CARET_HALF_PERIOD.as_millis()).is_multiple_of(2)
}

/// Time from `since_edit` until the caret next changes state.
fn flip_after(since_edit: Duration) -> Duration {
    let half = CARET_HALF_PERIOD.as_millis();
    let into = since_edit.as_millis() % half;
    Duration::from_millis((half - into) as u64)
}

pub struct SingleLineInput {
    focus_handle: FocusHandle,
    pub content: SharedString,
    pub placeholder: SharedString,
    key_context: String,
    accent_border: bool,
    /// Find mode: an empty field shows "Find", and its text is drawn in the accent colour.
    find_mode: bool,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    /// When the text or cursor last changed; the caret blinks from here.
    caret_edit: Instant,
    /// The single-level undo step for Ctrl+Z.
    undo_stack: UndoStack,
    /// The send limit a draft must fit. `None` on fields that send nothing,
    /// such as sheet filters, so their text is never cut.
    send_limit: Option<SendLimit>,
    /// Counts the user's edits and caret moves. A nick completion cycle ends
    /// when this moves. Programmatic `set_text` does not move it.
    input_serial: u64,
    /// Draws each character as a bullet, as Qt's password echo does. Copy and cut
    /// do nothing, so the secret stays in the field.
    mask: bool,
    /// The field's height in logical pixels. The text inside is always 32 tall and
    /// sits centred in it.
    height: f32,
    /// Whether the field draws its own panel, border and corners. A frameless field
    /// sits inside a frame its owner draws, such as the composer's SEND shell.
    framed: bool,
}

/// The bullet a masked field draws for each character.
const MASK_CHAR: char = '\u{2022}';

/// The text a masked field draws for `text`: one bullet per character.
pub fn masked_text(text: &str) -> String {
    MASK_CHAR.to_string().repeat(text.chars().count())
}

/// Byte offset in `masked_text(text)` for the byte offset `offset` in `text`. An
/// offset past the end, or inside a character, counts the characters before it.
pub fn drawn_offset(text: &str, offset: usize) -> usize {
    text.char_indices().take_while(|(at, _)| *at < offset).count() * MASK_CHAR.len_utf8()
}

/// Byte offset in `text` for a byte offset `drawn` in `masked_text(text)`, the
/// inverse of `drawn_offset`. Past the last character it is the text's length.
pub fn content_offset(text: &str, drawn: usize) -> usize {
    let index = drawn / MASK_CHAR.len_utf8();
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(at, _)| at)
}

/// Byte offset in `text` for a UTF-16 code-unit offset. Past the end clamps to
/// `text.len()`.
fn utf16_offset_in(text: &str, utf16: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_count = 0;
    for ch in text.chars() {
        if utf16_count >= utf16 {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_offset += ch.len_utf8();
    }
    utf8_offset
}

/// IME composition selection is UTF-16 relative to the inserted composition,
/// not the whole field. Offset both ends from the replacement start.
fn composition_selected_range(
    replacement_start: usize,
    composition: &str,
    selected_utf16: &Range<usize>,
) -> Range<usize> {
    let start = utf16_offset_in(composition, selected_utf16.start);
    let end = utf16_offset_in(composition, selected_utf16.end);
    replacement_start + start..replacement_start + end
}

/// Clamp a byte range into `text` and onto character (grapheme) boundaries.
fn clamp_char_range(text: &str, range: Range<usize>) -> Range<usize> {
    let start = snap_grapheme(text, range.start.min(text.len()));
    let end = snap_grapheme(text, range.end.min(text.len()));
    start..end.max(start)
}

impl SingleLineInput {
    pub fn new(
        placeholder: impl Into<SharedString>,
        key_context: impl Into<String>,
        focus_handle: FocusHandle,
    ) -> Self {
        Self {
            focus_handle,
            content: "".into(),
            placeholder: placeholder.into(),
            key_context: key_context.into(),
            accent_border: false,
            find_mode: false,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            caret_edit: Instant::now(),
            undo_stack: UndoStack::default(),
            send_limit: None,
            input_serial: 0,
            mask: false,
            height: 32.,
            framed: true,
        }
    }

    /// Draws the text as bullets. Used by the Connect sheet's secret fields.
    pub fn masked(mut self) -> Self {
        self.mask = true;
        self
    }

    /// Sets the field's height in logical pixels. The default is 32.
    pub fn height(mut self, px: f32) -> Self {
        self.height = px;
        self
    }

    /// Draws no panel, border or corners, so the owner's frame shows through.
    pub fn frameless(mut self) -> Self {
        self.framed = false;
        self
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    /// The byte offset in the drawn text for a byte offset in the content. The
    /// two match unless the field is masked.
    fn drawn_at(&self, offset: usize) -> usize {
        if self.mask {
            drawn_offset(&self.content, offset)
        } else {
            offset
        }
    }

    /// The content byte offset for a byte offset in the drawn text, the inverse of `drawn_at`.
    fn content_at(&self, drawn: usize) -> usize {
        if self.mask {
            content_offset(&self.content, drawn)
        } else {
            drawn
        }
    }

    /// The caret as a byte offset into `text()`. With a selection, the end the
    /// caret is on.
    pub fn cursor(&self) -> usize {
        self.cursor_offset()
    }

    /// Moves on every user edit and caret move. See `input_serial`.
    pub fn input_serial(&self) -> u64 {
        self.input_serial
    }

    /// Records a user edit or caret move for `input_serial`.
    fn note_input(&mut self) {
        self.input_serial = self.input_serial.wrapping_add(1);
    }

    /// Sets the send limit for what the shell shows: the channel or nick, or
    /// Status. Text already over the new limit is cut at its end, the way Qt
    /// clamps the composer when its target changes.
    pub fn set_send_limit(&mut self, limit: Option<SendLimit>, cx: &mut Context<Self>) {
        self.send_limit = limit;
        let keep = self.fit_len("", &self.content, "");
        if keep < self.content.len() {
            self.content = self.content[..keep].to_string().into();
            self.clamp_selection();
            self.undo_stack.clear();
        }
        self.restart_caret();
        cx.notify();
    }

    /// How many bytes of `inserted` fit between `before` and `after`. Without a
    /// send limit everything fits.
    fn fit_len(&self, before: &str, inserted: &str, after: &str) -> usize {
        match &self.send_limit {
            Some(limit) => limit.insert_fit_len(before, inserted, after),
            None => inserted.len(),
        }
    }

    /// Keeps both ends of the selection inside the text after a cut.
    fn clamp_selection(&mut self) {
        let len = self.content.len();
        self.selected_range = self.selected_range.start.min(len)..self.selected_range.end.min(len);
        self.marked_range = None;
    }

    /// The text and selection as they are now, for an undo step.
    fn snapshot(&self) -> EditSnapshot {
        EditSnapshot {
            text: self.content.to_string(),
            selected: self.selected_range.clone(),
        }
    }

    /// Whether the caret is in the solid half of its blink cycle right now.
    pub fn caret_solid(&self) -> bool {
        solid_at(self.caret_edit.elapsed())
    }

    /// How long until the caret next changes state, for the repaint timer.
    pub fn caret_flip_in(&self) -> Duration {
        flip_after(self.caret_edit.elapsed())
    }

    /// Restarts the blink so the caret is solid right after an edit or move.
    fn restart_caret(&mut self) {
        self.caret_edit = Instant::now();
    }

    /// Replaces the whole text, cut to the send limit, with the caret at the end.
    /// The undo step is dropped, since a programmatic change is not something
    /// Ctrl+Z should take back.
    pub fn set_text(&mut self, text: impl Into<SharedString>) {
        self.load_text(text.into(), None);
    }

    /// Like `set_text`, with the caret at byte offset `cursor` (clamped to the
    /// cut text). Not a user edit, so `input_serial` does not move.
    pub fn set_text_with_cursor(&mut self, text: impl Into<SharedString>, cursor: usize) {
        self.load_text(text.into(), Some(cursor));
    }

    fn load_text(&mut self, text: SharedString, cursor: Option<usize>) {
        let keep = self.fit_len("", &text, "");
        self.content = if keep == text.len() {
            text
        } else {
            text[..keep].to_string().into()
        };
        let caret = cursor.map_or(self.content.len(), |at| at.min(self.content.len()));
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range = None;
        self.undo_stack.clear();
        self.restart_caret();
    }

    pub fn clear(&mut self) {
        self.set_text("");
    }

    pub fn set_accent_border(&mut self, on: bool) {
        self.accent_border = on;
    }

    /// Find mode on or off. While it is on, an empty field shows "Find" and the
    /// text is drawn in the accent colour, so the query reads as a search.
    pub fn set_find_mode(&mut self, on: bool, cx: &mut Context<Self>) {
        self.find_mode = on;
        cx.notify();
    }

    /// Selects the whole text with the caret at its end, as Ctrl+A does. Find
    /// mode uses it so that typing replaces the draft that becomes the query.
    pub fn select_all_text(&mut self, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.select_all_text(cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, cx);
    }

    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.content.len(), cx);
    }

    /// Ctrl+Left: the start of the word before the cursor.
    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(previous_word_start(&self.content, self.cursor_offset()), cx);
    }

    /// Ctrl+Right: the end of the word after the cursor.
    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(next_word_end(&self.content, self.cursor_offset()), cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(previous_word_start(&self.content, self.cursor_offset()), cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(next_word_end(&self.content, self.cursor_offset()), cx);
    }

    fn delete_word_backward(
        &mut self,
        _: &DeleteWordBackward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(previous_word_start(&self.content, self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_word_forward(
        &mut self,
        _: &DeleteWordForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(next_word_end(&self.content, self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    /// Ctrl+Z: restores the text and selection from before the last step. The
    /// restored text is cut to the send limit, which may have changed since.
    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        let Some(step) = self.undo_stack.undo() else {
            return;
        };
        self.note_input();
        let keep = self.fit_len("", &step.text, "");
        self.content = step.text[..keep].to_string().into();
        self.selected_range = step.selected;
        self.selection_reversed = false;
        self.clamp_selection();
        self.restart_caret();
        cx.notify();
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace('\n', " "), window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.mask {
            // A secret is never copied. Nothing else takes the key either.
            return;
        }
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        } else {
            // No draft selection: let the transcript's Ctrl+C take it.
            cx.propagate();
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if self.mask {
            return;
        }
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.note_input();
        self.selected_range = offset..offset;
        self.restart_caret();
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        self.content_at(line.closest_index_for_x(position.x - bounds.left()))
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.note_input();
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.restart_caret();
        cx.notify();
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        utf16_offset_in(&self.content, offset)
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .as_ref()
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .as_ref()
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for SingleLineInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_input();
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());

        // Typed and pasted text is cut at a character boundary to the send limit.
        let keep = self.fit_len(&self.content[..range.start], new_text, &self.content[range.end..]);
        let new_text = &new_text[..keep];
        if range.is_empty() && new_text.is_empty() {
            return;
        }

        let before = self.snapshot();
        self.undo_stack.record(before, range.clone(), new_text);
        self.content = (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
            .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range = None;
        self.restart_caret();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_input();
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());

        let keep = self.fit_len(&self.content[..range.start], new_text, &self.content[range.end..]);
        let new_text = &new_text[..keep];
        self.content = (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
            .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        // selected_range is relative to the composition (Apple setMarkedText /
        // Windows GCS_CURSORPOS), not the whole field. Converting against
        // content and then adding range.end mixed two coordinate systems and
        // could land inside a multi-byte character after an ASCII prefix.
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|sel| composition_selected_range(range.start, new_text, sel))
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        self.selected_range = clamp_char_range(&self.content, self.selected_range.clone());
        self.restart_caret();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(self.drawn_at(range.start)),
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(self.drawn_at(range.end)),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let drawn = last_layout.index_for_x(point.x - line_point.x)?;
        Some(self.offset_to_utf16(self.content_at(drawn)))
    }
}

struct SingleLineInputElement {
    input: Entity<SingleLineInput>,
}

impl IntoElement for SingleLineInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

struct InputPrepaint {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl Element for SingleLineInputElement {
    type RequestLayoutState = ();
    type PrepaintState = InputPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = crate::util::style(cx);
        let mut layout_style = Style::default();
        layout_style.size.width = relative(1.).into();
        layout_style.size.height = crate::util::scaled_px(&style, 32.).into();
        (
            window.request_layout(layout_style, [], cx),
            (),
        )
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let app_style = crate::util::style(cx);
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let caret_solid = input.caret_solid();
        let mut text_style = window.text_style();
        text_style.font_family = FONT_FAMILY.into();
        text_style.font_size =
            gpui::AbsoluteLength::Pixels(crate::util::scaled_px(&app_style, 13.));

        let find_mode = input.find_mode;
        let drawn_content: SharedString = if input.mask {
            masked_text(&content).into()
        } else {
            content.clone()
        };
        let (display_text, text_color) = match (content.is_empty(), find_mode) {
            (true, true) => ("Find".into(), to_gpui(app_style.accent_color())),
            (true, false) => (input.placeholder.clone(), to_gpui(app_style.muted_color())),
            (false, true) => (drawn_content, to_gpui(app_style.accent_color())),
            (false, false) => (drawn_content, to_gpui(app_style.ink_color())),
        };

        let run = TextRun {
            len: display_text.len(),
            font: text_style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &[run], None);

        let cursor_pos = line.x_for_index(input.drawn_at(cursor));
        let selection_color = to_gpui(app_style.selection_color());
        // The glyph box that `paint_line` draws: ascent + descent, centred in the
        // element's own height. The caret and the selection span that, not the whole
        // field. The window's line height is not used: it is 1.618 em of the window's
        // 16px default, which put the 13px text about 3px above the middle of the field.
        let line_height = bounds.size.height;
        let glyph_height = line.ascent + line.descent;
        let glyph_top = bounds.top() + (line_height - glyph_height) / 2.;
        let (selection, cursor_quad) = if selected_range.is_empty() {
            (
                None,
                caret_solid.then(|| {
                    fill(
                        Bounds::new(
                            point(bounds.left() + cursor_pos, glyph_top),
                            size(px(2.), glyph_height),
                        ),
                        to_gpui(app_style.accent_color()),
                    )
                }),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(input.drawn_at(selected_range.start)),
                            glyph_top,
                        ),
                        point(
                            bounds.left() + line.x_for_index(input.drawn_at(selected_range.end)),
                            glyph_top + glyph_height,
                        ),
                    ),
                    selection_color,
                )),
                None,
            )
        };
        InputPrepaint {
            line: Some(line),
            cursor: cursor_quad,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line = prepaint.line.take().unwrap();
        line.paint(bounds.origin, bounds.size.height, window, cx)
            .unwrap();
        if focus_handle.is_focused(window) && window.is_window_active() {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }
        self.input.update(cx, |input, _| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for SingleLineInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_style = crate::util::style(cx);
        let focused = self.focus_handle.is_focused(window) && window.is_window_active();
        self.set_accent_border(focused);
        let border = if self.accent_border {
            to_gpui(app_style.accent_color())
        } else {
            to_gpui(app_style.divider_color())
        };

        div()
            .key_context(self.key_context.as_str())
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::delete_word_backward))
            .on_action(cx.listener(Self::delete_word_forward))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .h(crate::util::scaled_px(&app_style, self.height))
            .flex()
            .items_center()
            .px(crate::util::scaled_px(&app_style, 8.))
            .when(self.framed, |this| {
                this.bg(to_gpui(app_style.panel_color()))
                    .border_1()
                    .border_color(border)
                    .rounded(crate::util::scaled_px(&app_style, 7.))
            })
            .child(SingleLineInputElement {
                input: cx.entity(),
            })
    }
}

impl Focusable for SingleLineInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Text-field keys for the composer, the sheet filters and the Connect fields.
/// `secondary-` is Ctrl on Windows and Linux and Cmd on macOS, so clipboard and
/// undo follow the platform. Word movement uses Ctrl+arrows on every platform:
/// Alt+arrows walk the server list.
pub fn bind_input_keys(cx: &mut gpui::App) {
    for context in ["Composer", "SheetFilter", "ConnectField"] {
        cx.bind_keys([
            gpui::KeyBinding::new("backspace", Backspace, Some(context)),
            gpui::KeyBinding::new("delete", Delete, Some(context)),
            gpui::KeyBinding::new("left", Left, Some(context)),
            gpui::KeyBinding::new("right", Right, Some(context)),
            gpui::KeyBinding::new("shift-left", SelectLeft, Some(context)),
            gpui::KeyBinding::new("shift-right", SelectRight, Some(context)),
            gpui::KeyBinding::new("ctrl-left", WordLeft, Some(context)),
            gpui::KeyBinding::new("ctrl-right", WordRight, Some(context)),
            gpui::KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some(context)),
            gpui::KeyBinding::new("ctrl-shift-right", SelectWordRight, Some(context)),
            gpui::KeyBinding::new("ctrl-backspace", DeleteWordBackward, Some(context)),
            gpui::KeyBinding::new("ctrl-delete", DeleteWordForward, Some(context)),
            gpui::KeyBinding::new("secondary-a", SelectAll, Some(context)),
            gpui::KeyBinding::new("secondary-v", Paste, Some(context)),
            gpui::KeyBinding::new("secondary-c", Copy, Some(context)),
            gpui::KeyBinding::new("secondary-x", Cut, Some(context)),
            gpui::KeyBinding::new("secondary-z", Undo, Some(context)),
            gpui::KeyBinding::new("home", Home, Some(context)),
            gpui::KeyBinding::new("end", End, Some(context)),
            gpui::KeyBinding::new("shift-home", SelectHome, Some(context)),
            gpui::KeyBinding::new("shift-end", SelectEnd, Some(context)),
        ]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_is_solid_then_hidden_in_half_periods() {
        assert!(solid_at(Duration::ZERO));
        assert!(solid_at(Duration::from_millis(529)));
        assert!(!solid_at(Duration::from_millis(530)));
        assert!(!solid_at(Duration::from_millis(1059)));
        assert!(solid_at(Duration::from_millis(1060)));
    }

    #[test]
    fn caret_flips_at_the_next_boundary() {
        assert_eq!(flip_after(Duration::ZERO), Duration::from_millis(530));
        assert_eq!(flip_after(Duration::from_millis(500)), Duration::from_millis(30));
        assert_eq!(flip_after(Duration::from_millis(530)), Duration::from_millis(530));
    }

    #[test]
    fn a_masked_field_draws_one_bullet_per_character() {
        assert_eq!(masked_text(""), "");
        assert_eq!(masked_text("abc"), "\u{2022}\u{2022}\u{2022}");
        // A multi-byte character is still one bullet.
        assert_eq!(masked_text("é€"), "\u{2022}\u{2022}");
    }

    #[test]
    fn caret_offsets_map_between_content_and_bullets() {
        let bullet = MASK_CHAR.len_utf8();
        assert_eq!(drawn_offset("abc", 0), 0);
        assert_eq!(drawn_offset("abc", 2), 2 * bullet);
        assert_eq!(drawn_offset("abc", 3), 3 * bullet, "the end of the text");
        // Byte 2 sits after the two-byte "é", so it is the second character.
        assert_eq!(drawn_offset("é€x", 2), bullet);
        assert_eq!(drawn_offset("é€x", 5), 2 * bullet);
        assert_eq!(drawn_offset("é€x", 99), 3 * bullet, "past the end counts every character");
    }

    #[test]
    fn drawn_offsets_map_back_to_the_content() {
        let bullet = MASK_CHAR.len_utf8();
        assert_eq!(content_offset("abc", 0), 0);
        assert_eq!(content_offset("abc", 2 * bullet), 2);
        assert_eq!(content_offset("abc", 3 * bullet), 3, "the end of the text");
        assert_eq!(content_offset("é€x", bullet), 2);
        assert_eq!(content_offset("é€x", 2 * bullet), 5);
        assert_eq!(content_offset("é€x", 9 * bullet), 6, "past the end is the length");
        for offset in [0, 2, 5, 6] {
            let drawn = drawn_offset("é€x", offset);
            assert_eq!(content_offset("é€x", drawn), offset);
        }
    }

    #[test]
    fn ime_composition_selection_is_relative_to_the_insert() {
        // ASCII prefix + CJK composition: UTF-16 0..2 selects the whole
        // composition. Converting against the whole field would land on "hi".
        let prefix = "hi ";
        let composition = "世界";
        let sel = composition_selected_range(prefix.len(), composition, &(0..2));
        assert_eq!(sel, prefix.len()..prefix.len() + composition.len());
        let content = format!("{prefix}{composition}");
        assert!(content.is_char_boundary(sel.start));
        assert!(content.is_char_boundary(sel.end));
        assert_eq!(&content[sel], composition);
    }

    #[test]
    fn ime_composition_caret_stays_on_char_boundaries() {
        let prefix = "ab";
        let composition = "あい";
        // Caret after the first composed character (one UTF-16 unit).
        let sel = composition_selected_range(prefix.len(), composition, &(1..1));
        assert_eq!(sel.start, prefix.len() + "あ".len());
        assert_eq!(sel.end, sel.start);
        let content = format!("{prefix}{composition}");
        assert!(content.is_char_boundary(sel.start));
        // Both ends offset from replacement start — not the old mark's end.
        let old_mark_end = prefix.len() + 3;
        assert_ne!(
            sel.end,
            old_mark_end + utf16_offset_in(composition, 1),
            "end must not use the prior replacement range end"
        );
        let clamped = clamp_char_range(&content, sel);
        assert_eq!(clamped, prefix.len() + "あ".len()..prefix.len() + "あ".len());
    }
}
