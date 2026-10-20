use gpui::{prelude::*, div};

use crate::model::Style;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The unread pill: accent when the conversation has a mention, raised otherwise.
pub fn unread_badge(style: &Style, unread: u32, mention: bool) -> impl IntoElement {
    div()
        .min_w(scaled_px(style, 19.))
        .h(scaled_px(style, 19.))
        .px(scaled_px(style, 5.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(scaled_px(style, 10.))
        .bg(if mention {
            to_gpui(style.accent_color())
        } else {
            to_gpui(style.hover_color())
        })
        .text_color(to_gpui(style.ink_color()))
        .font_family(FONT_FAMILY)
        .child(unread.to_string())
}
