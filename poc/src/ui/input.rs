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
        Paste,
        Cut,
        Copy,
    ]
);

/// Half of a caret blink cycle: solid for this long, then hidden for as long.
const CARET_HALF_PERIOD: Duration = Duration::from_millis(530);

/// Whether the caret is solid `since_edit` after the last edit or cursor move.
fn solid_at(since_edit: Duration) -> bool {
    (since_edit.as_millis() / CARET_HALF_PERIOD.as_millis()) % 2 == 0
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
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    /// When the text or cursor last changed; the caret blinks from here.
    caret_edit: Instant,
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
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            caret_edit: Instant::now(),
        }
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn has_selection(&self) -> bool {
        !self.selected_range.is_empty()
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

    pub fn set_text(&mut self, text: impl Into<SharedString>) {
        self.content = text.into();
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        self.restart_caret();
    }

    pub fn clear(&mut self) {
        self.set_text("");
    }

    pub fn set_accent_border(&mut self, on: bool) {
        self.accent_border = on;
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
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
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
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
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
        line.closest_index_for_x(position.x - bounds.left())
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
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
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
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
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());

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
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());

        self.content = (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
            .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
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
                bounds.left() + last_layout.x_for_index(range.start),
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end),
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
        let utf8_index = last_layout.index_for_x(point.x - line_point.x)?;
        Some(self.offset_to_utf16(utf8_index))
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

        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), to_gpui(app_style.muted_color()))
        } else {
            (content, to_gpui(app_style.ink_color()))
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

        let cursor_pos = line.x_for_index(cursor);
        let selection_color = to_gpui(app_style.selection_color());
        // The glyph box that `paint_line` draws: ascent + descent, centred in the
        // line height. The caret and the selection span that, not the whole field.
        let line_height = window.line_height();
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
                            bounds.left() + line.x_for_index(selected_range.start),
                            glyph_top,
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected_range.end),
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
        line.paint(bounds.origin, window.line_height(), window, cx)
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
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .h(crate::util::scaled_px(&app_style, 32.))
            .px(crate::util::scaled_px(&app_style, 8.))
            .bg(to_gpui(app_style.panel_color()))
            .border_1()
            .border_color(border)
            .rounded(crate::util::scaled_px(&app_style, 7.))
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

pub fn bind_input_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("backspace", Backspace, Some("Composer")),
        gpui::KeyBinding::new("delete", Delete, Some("Composer")),
        gpui::KeyBinding::new("left", Left, Some("Composer")),
        gpui::KeyBinding::new("right", Right, Some("Composer")),
        gpui::KeyBinding::new("shift-left", SelectLeft, Some("Composer")),
        gpui::KeyBinding::new("shift-right", SelectRight, Some("Composer")),
        gpui::KeyBinding::new("ctrl-a", SelectAll, Some("Composer")),
        gpui::KeyBinding::new("ctrl-v", Paste, Some("Composer")),
        gpui::KeyBinding::new("ctrl-c", Copy, Some("Composer")),
        gpui::KeyBinding::new("ctrl-x", Cut, Some("Composer")),
        gpui::KeyBinding::new("home", Home, Some("Composer")),
        gpui::KeyBinding::new("end", End, Some("Composer")),
        gpui::KeyBinding::new("backspace", Backspace, Some("SheetFilter")),
        gpui::KeyBinding::new("delete", Delete, Some("SheetFilter")),
        gpui::KeyBinding::new("left", Left, Some("SheetFilter")),
        gpui::KeyBinding::new("right", Right, Some("SheetFilter")),
        gpui::KeyBinding::new("shift-left", SelectLeft, Some("SheetFilter")),
        gpui::KeyBinding::new("shift-right", SelectRight, Some("SheetFilter")),
        gpui::KeyBinding::new("ctrl-a", SelectAll, Some("SheetFilter")),
        gpui::KeyBinding::new("ctrl-v", Paste, Some("SheetFilter")),
        gpui::KeyBinding::new("ctrl-c", Copy, Some("SheetFilter")),
        gpui::KeyBinding::new("ctrl-x", Cut, Some("SheetFilter")),
        gpui::KeyBinding::new("home", Home, Some("SheetFilter")),
        gpui::KeyBinding::new("end", End, Some("SheetFilter")),
    ]);
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
}
