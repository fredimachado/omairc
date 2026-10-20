use gpui::{prelude::*, Hsla};

use crate::model::Style;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// Initials on a coloured disc with a presence dot in the bottom-right corner.
pub fn avatar_with_presence(
    style: &Style,
    avatar_px: f32,
    fill: Hsla,
    initials: String,
    initials_color: Hsla,
    initials_size: f32,
    presence: &str,
) -> impl IntoElement {
    let avatar_sz = scaled_px(style, avatar_px);
    let dot_sz = scaled_px(style, 7.);
    let inset = avatar_sz - dot_sz;
    gpui::div()
        .relative()
        .size(avatar_sz)
        .flex_shrink_0()
        .child(
            gpui::div()
                .size_full()
                .rounded_full()
                .bg(fill)
                .flex()
                .items_center()
                .justify_center()
                .text_size(scaled_px(style, initials_size))
                .text_color(initials_color)
                .font_family(FONT_FAMILY)
                .child(initials),
        )
        .child(
            gpui::div()
                .absolute()
                .left(inset)
                .top(inset)
                .size(dot_sz)
                .rounded_full()
                .border(scaled_px(style, 2.))
                .border_color(to_gpui(style.panel_color()))
                .bg(to_gpui(style.presence_color(presence))),
        )
}
