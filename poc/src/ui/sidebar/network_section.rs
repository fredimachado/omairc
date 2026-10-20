use gpui::{div, prelude::*};

use crate::model::{Network, Style};
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The server header at the top of the sidebar: icon, name and connection state.
pub fn network_header(network: &Network, style: &Style) -> impl IntoElement {
    div()
        .p(scaled_px(style, 12.))
        .border_b_1()
        .border_color(to_gpui(style.divider_color()))
        .flex()
        .items_center()
        .gap(scaled_px(style, 8.))
        .child(
            div()
                .size(scaled_px(style, 28.))
                .rounded_full()
                .bg(to_gpui(style.accent_color()))
                .flex()
                .items_center()
                .justify_center()
                .text_size(scaled_px(style, 11.))
                .text_color(to_gpui(crate::model::Color::rgb(1., 1., 1.)))
                .font_family(FONT_FAMILY)
                .child(network.display_name.chars().next().unwrap_or('?').to_string()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(scaled_px(style, 13.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(to_gpui(style.ink_color()))
                        .font_family(FONT_FAMILY)
                        .child(network.display_name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(scaled_px(style, 4.))
                        .child(
                            div()
                                .size(scaled_px(style, 6.))
                                .rounded_full()
                                .bg(to_gpui(style.presence_color("online"))),
                        )
                        .child(
                            div()
                                .text_size(scaled_px(style, 10.))
                                .text_color(to_gpui(style.muted_color()))
                                .font_family(FONT_FAMILY)
                                .child("Connected"),
                        ),
                ),
        )
        .child(
            div()
                .flex_grow()
                .flex()
                .justify_end()
                .text_size(scaled_px(style, 10.))
                .text_color(to_gpui(style.muted_color()))
                .font_family(FONT_FAMILY)
                .child("edit"),
        )
}
