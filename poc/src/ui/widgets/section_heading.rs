use gpui::{prelude::*, div};

use crate::model::Style;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The muted caps label above a sidebar group (`CHANNELS`, `DIRECT MESSAGES`).
pub fn section_label(style: &Style, text: &str) -> impl IntoElement {
    div()
        .px(scaled_px(style, 12.))
        .pt(scaled_px(style, 8.))
        .pb(scaled_px(style, 4.))
        .text_size(scaled_px(style, 10.))
        .text_color(to_gpui(style.muted_color()))
        .font_family(FONT_FAMILY)
        .child(text.to_string())
}
