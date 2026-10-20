use gpui::{div, prelude::*, Entity, MouseButton};

use crate::model::Style;
use crate::ui::input::SingleLineInput;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The single-line draft and its SEND button. A click calls `on_send`. Enter
/// is handled by the shell root (`ComposerSubmit`), which checks header focus.
pub fn render_composer(
    composer: Entity<SingleLineInput>,
    style: &Style,
    on_send: Handler<()>,
) -> impl IntoElement {
    div()
        .px(scaled_px(style, 16.))
        .py(scaled_px(style, 10.))
        .border_t_1()
        .border_color(to_gpui(style.divider_color()))
        .flex()
        .gap(scaled_px(style, 8.))
        .items_center()
        .child(
            div()
                .flex_grow()
                .key_context("Composer")
                .child(composer),
        )
        .child(
            div()
                .px(scaled_px(style, 12.))
                .py(scaled_px(style, 8.))
                .rounded(scaled_px(style, 7.))
                .bg(to_gpui(style.accent_color()))
                .cursor_pointer()
                .text_size(scaled_px(style, 12.))
                .text_color(to_gpui(crate::model::Color::rgb(1., 1., 1.)))
                .font_family(FONT_FAMILY)
                .child("SEND")
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    on_send((), window, cx);
                }),
        )
}
