use gpui::{div, prelude::*};

use crate::model::Style;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The muted, bold caps used for sidebar group headings. Qt's letter spacing
/// (0.8) has no gpui style, so it is not reproduced.
fn caps_text(style: &Style, text: &str) -> gpui::Div {
    div()
        .text_size(scaled_px(style, 9.))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(to_gpui(style.muted_color()))
        .font_family(FONT_FAMILY)
        .child(text.to_string())
}

/// `CHANNELS`: 28 tall, text centred vertically at x 19.
pub fn channels_heading(style: &Style) -> impl IntoElement {
    div()
        .h(scaled_px(style, 28.))
        .px(scaled_px(style, 19.))
        .flex()
        .items_center()
        .child(caps_text(style, "CHANNELS"))
}

/// `DIRECT MESSAGES`: 36 tall, text sitting 8px above the bottom edge.
pub fn directs_heading(style: &Style) -> impl IntoElement {
    div()
        .h(scaled_px(style, 36.))
        .px(scaled_px(style, 19.))
        .pb(scaled_px(style, 8.))
        .flex()
        .flex_col()
        .justify_end()
        .child(caps_text(style, "DIRECT MESSAGES"))
}
