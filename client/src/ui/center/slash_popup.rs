//! The slash-complete popup: the verb and `/pref` rows above the composer, a.
//! `slashCompleteList` in `client/src/ui/center/`. Its rows are styled
//! like the Jump and Link lists' `text_row`: 12px, radius 7, and the selected row
//! on the hover colour. The shell owns the rows and the highlight (`SlashSession`).
//! A click reports the row's index through `on_pick`.

use gpui::{div, prelude::*, transparent_black, MouseButton};

use crate::model::slash_complete::{usage_hint, SlashHit};
use crate::model::{mix_colors, Style};
use crate::ui::center::composer::SHELL_INSET;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// One row's height in logical pixels (Qt `slashHit` height 28).
const ROW_HEIGHT: f32 = 28.;

/// The gap between the popup's bottom edge and the composer strip's top rule (Qt's
/// `slashCompleteList` `anchors.bottomMargin`, measured from the rule rather than the
/// shell, which sits below the rule now that the shell is centred in the strip).
const SHELL_GAP: f32 = 6.;

/// The popup as an overlay over the transcript, standing `SHELL_GAP` above the
/// composer strip's rule and spanning the shell's side edges (Qt's
/// `slashCompleteList`). Anchoring to the rule keeps the popup clear of it, so the
/// popup never covers the divider. It is absolute, so it does not push the transcript
/// and it occludes the mouse so a click on a row does not start a transcript selection.
/// `render_composer` places it in the composer strip.
pub fn render_slash_popup(
    hits: &[SlashHit],
    selected: usize,
    style: &Style,
    on_pick: Handler<usize>,
) -> impl IntoElement {
    let ink = to_gpui(style.ink_color());
    let muted = to_gpui(style.muted_color());
    let hover = to_gpui(style.hover_color());
    // Qt's selected row: the selection colour mixed toward the raised panel, a little
    // stronger on a dark theme (`slashCompleteList` in `client/src/ui/center/`).
    let selected_fill = to_gpui(mix_colors(
        style.selection_color(),
        style.raised_color(),
        if style.dark_mode() { 0.45 } else { 0.35 },
    ));

    let rows = hits.iter().enumerate().map(|(index, hit)| {
        let on_pick = on_pick.clone();
        let hint = usage_hint(&hit.label, &hit.usage).to_string();
        let is_selected = index == selected;
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
            .bg(if is_selected {
                selected_fill
            } else {
                transparent_black()
            })
            // The selected row keeps its fill under the pointer; only the others take the hover.
            .hover(move |row| if is_selected { row } else { row.bg(hover) })
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
        .left(scaled_px(style, SHELL_INSET))
        .right(scaled_px(style, SHELL_INSET))
        .bottom_full()
        .mb(scaled_px(style, SHELL_GAP))
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
