use std::ops::Range;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};

use crate::model::text_edit::snap_grapheme;

use super::SingleLineInput;

/// Byte offset in `text` for a UTF-16 code-unit offset. Past the end clamps to
pub(crate) fn utf16_offset_in(text: &str, utf16: usize) -> usize {
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

/// IME composition selection is UTF-16 relative to the inserted composition
/// not the whole field. Offset both ends from the replacement start.
pub(crate) fn composition_selected_range(
    replacement_start: usize,
    composition: &str,
    selected_utf16: &Range<usize>,
) -> Range<usize> {
    let start = utf16_offset_in(composition, selected_utf16.start);
    let end = utf16_offset_in(composition, selected_utf16.end);
    replacement_start + start..replacement_start + end
}

/// Clamp a byte range into `text` and onto character (grapheme) boundaries.
pub(crate) fn clamp_char_range(text: &str, range: Range<usize>) -> Range<usize> {
    let start = snap_grapheme(text, range.start.min(text.len()));
    let end = snap_grapheme(text, range.end.min(text.len()));
    start..end.max(start)
}

impl SingleLineInput {
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
        // selected_range is relative to the composition (Apple setMarkedText
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
        use gpui::{point, Bounds};

        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let scroll = gpui::px(self.horizontal_offset);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(self.drawn_at(range.start)) - scroll,
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(self.drawn_at(range.end)) - scroll,
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let drawn = last_layout.index_for_x(line_point.x + gpui::px(self.horizontal_offset))?;
        Some(self.offset_to_utf16(self.content_at(drawn)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        AppContext as _, Bounds, EntityInputHandler, ShapedLine, TestAppContext, TextRun, point,
        px, size,
    };

    fn shape_line(window: &mut Window, text: &str) -> ShapedLine {
        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let run = TextRun {
            len: text.len(),
            font: text_style.font(),
            color: gpui::Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window
            .text_system()
            .shape_line(text.to_string().into(), font_size, &[run], None)
    }

    #[gpui::test]
    fn ime_character_index_respects_horizontal_scroll(cx: &mut TestAppContext) {
        let cxw = cx.add_empty_window();
        let text = "0123456789";
        let input = cxw.update(|window, cx| {
            let focus = cx.focus_handle();
            let line = shape_line(window, text);
            let bounds = Bounds::new(point(px(100.), px(0.)), size(px(80.), px(20.)));
            let scroll = 40.;
            cx.new(|_| {
                let mut input = SingleLineInput::new("", "test", focus);
                input.content = text.into();
                input.last_layout = Some(line);
                input.last_bounds = Some(bounds);
                input.horizontal_offset = scroll;
                input
            })
        });

        let (near_end, near_start) = cxw.update(|window, cx| {
            input.update(cx, |input, cx| {
                let y = input.last_bounds.unwrap().center().y;
                let near_start = EntityInputHandler::character_index_for_point(
                    input,
                    point(px(110.), y),
                    window,
                    cx,
                )
                .unwrap();
                let near_end = EntityInputHandler::character_index_for_point(
                    input,
                    point(px(130.), y),
                    window,
                    cx,
                )
                .expect("click inside the scrolled field");
                (near_end, near_start)
            })
        });

        assert!(
            near_end > near_start,
            "IME point mapping must follow local x through scroll (got start={near_start}, end={near_end})"
        );
    }
}
