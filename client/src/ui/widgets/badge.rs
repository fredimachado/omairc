use gpui::{div, prelude::*, Hsla};

use crate::model::{Color, Style};
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The unread pill, as `ConversationRow.qml`: accent with white text when the
/// conversation has a mention, raised with ink text otherwise.
pub fn unread_badge(style: &Style, unread: u32, mention: bool) -> impl IntoElement {
    let (bg, fg): (Hsla, Hsla) = if mention {
        (to_gpui(style.accent_color()), to_gpui(Color::rgb(1., 1., 1.)))
    } else {
        (to_gpui(style.raised_color()), to_gpui(style.ink_color()))
    };
    div()
        .min_w(scaled_px(style, 19.))
        .h(scaled_px(style, 19.))
        .px(scaled_px(style, 5.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(scaled_px(style, 10.))
        .font_weight(gpui::FontWeight::BOLD)
        .bg(bg)
        .text_color(fg)
        .font_family(FONT_FAMILY)
        .child(unread.to_string())
}
