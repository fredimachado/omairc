//! The floating `↓` chip over the transcript (`UnseenJumpButton.qml`). It shows
//! while an unseen jump is armed: a line arrived while the reader was scrolled up,
//! or a "New messages" mark is open above the end. A press runs the jump. The
//! shell draws nothing here, so the chip is part of the transcript view.

use gpui::{div, prelude::*, App, IntoElement, MouseButton, MouseDownEvent, Window};

use crate::model::{mix_colors, Style};
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The chip's diameter in logical pixels (`UnseenJumpButton.qml`).
const CHIP_SIZE: f32 = 36.;
/// The chip's inset from the transcript's bottom-right corner.
const CHIP_INSET: f32 = 16.;

/// The chip. `on_press` runs the jump. The press stops at the chip, so it does not
/// start a transcript selection underneath it.
pub fn render_unseen_jump(
    style: &Style,
    on_press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let raised = style.raised_color();
    let hover = mix_colors(
        raised,
        style.accent_color(),
        if style.dark_mode() { 0.35 } else { 0.22 },
    );
    div()
        .id("unseen-jump")
        .absolute()
        .right(scaled_px(style, CHIP_INSET))
        .bottom(scaled_px(style, CHIP_INSET))
        .size(scaled_px(style, CHIP_SIZE))
        .rounded_full()
        .border_1()
        .border_color(to_gpui(style.divider_color()))
        .bg(to_gpui(raised))
        .hover(move |chip| chip.bg(to_gpui(hover)))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |ev, window, cx| {
            cx.stop_propagation();
            on_press(ev, window, cx);
        })
        .child(
            div()
                .text_size(scaled_px(style, 16.))
                .text_color(to_gpui(style.ink_color()))
                .font_family(FONT_FAMILY)
                .child("\u{2193}"),
        )
        // The accent dot marks that the chip is armed (`unseenJumpDot`).
        .child(
            div()
                .absolute()
                .top(scaled_px(style, 2.))
                .right(scaled_px(style, 2.))
                .size(scaled_px(style, 8.))
                .rounded_full()
                .bg(to_gpui(style.accent_color())),
        )
}
