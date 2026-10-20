use gpui::{div, prelude::*};

use crate::model::{Network, Style};
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The box at the foot of the sidebar: the network nick and the inbox count.
pub fn identity_footer(network: &Network, inbox_count: u32, style: &Style) -> impl IntoElement {
    div()
        .m(scaled_px(style, 10.))
        .p(scaled_px(style, 10.))
        .rounded(scaled_px(style, 8.))
        .bg(to_gpui(style.raised_color()))
        .border_1()
        .border_color(to_gpui(style.divider_color()))
        .flex()
        .justify_between()
        .child(
            div()
                .text_size(scaled_px(style, 12.))
                .text_color(to_gpui(style.ink_color()))
                .font_family(FONT_FAMILY)
                .child(network.nick.clone()),
        )
        .child(
            div()
                .text_size(scaled_px(style, 11.))
                .text_color(to_gpui(style.muted_color()))
                .font_family(FONT_FAMILY)
                .child(format!("{} inbox", inbox_count)),
        )
}
