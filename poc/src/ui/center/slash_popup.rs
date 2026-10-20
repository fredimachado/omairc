//! The slash-complete popup: the verb and `/pref` rows above the composer, a port of
//! `slashCompleteList` in `src/qml/ConversationColumn.qml`. Its rows are styled
//! like the Jump and Link lists' `text_row`: 12px, radius 7, and the selected row
//! on the hover colour. The shell owns the rows and the highlight (`SlashSession`).
//! A click reports the row's index through `on_pick`.

use gpui::{div, prelude::*, transparent_black, MouseButton};

use crate::model::slash_complete::{usage_hint, SlashHit};
use crate::model::Style;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// One row's height in logical pixels (Qt `slashHit` height 28).
const ROW_HEIGHT: f32 = 28.;

/// The popup as an overlay over the transcript, anchored to the top of the
/// composer's input. It is absolute, so it does not push the transcript, and it
/// occludes the mouse so a click on a row does not start a transcript selection.
pub fn render_slash_popup(
    hits: &[SlashHit],
    selected: usize,
    style: &Style,
    on_pick: Handler<usize>,
) -> impl IntoElement {
    let ink = to_gpui(style.ink_color());
    let muted = to_gpui(style.muted_color());
    let hover = to_gpui(style.hover_color());

    let rows = hits.iter().enumerate().map(|(index, hit)| {
        let on_pick = on_pick.clone();
        let hint = usage_hint(&hit.label, &hit.usage).to_string();
        div()
            .id(("slash-hit", index))
            .h(scaled_px(style, ROW_HEIGHT))
            .px(scaled_px(style, 8.))
            .rounded(scaled_px(style, 7.))
            .flex()
            .items_center()
            .gap(scaled_px(style, 12.))
            .overflow_hidden()
            .font_family(FONT_FAMILY)
            .text_size(scaled_px(style, 12.))
            .cursor_pointer()
            .bg(if index == selected {
                hover
            } else {
                transparent_black()
            })
            .hover(move |row| row.bg(hover))
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(ink)
                    .child(hit.label.clone()),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_grow()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_color(muted)
                    .child(hint),
            )
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                on_pick(index, window, cx);
            })
    });

    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_full()
        .mb(scaled_px(style, 6.))
        .p(scaled_px(style, 6.))
        .rounded(scaled_px(style, 10.))
        .bg(to_gpui(style.raised_color()))
        .border_1()
        .border_color(to_gpui(style.divider_color()))
        .flex()
        .flex_col()
        .gap(scaled_px(style, 2.))
        .occlude()
        .children(rows)
}
