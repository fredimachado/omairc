use gpui::{div, prelude::*, Entity, ScrollHandle};

use crate::model::{Conversation, Style};
use crate::ui::members::member_row::member_row;
use crate::ui::scrollbar::{div_vertical_scrollbar, ScrollbarDragCell};
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The right-hand member panel for a channel. `host` is only notified by the
/// scrollbar.
pub fn render_members<V: 'static>(
    conv: &Conversation,
    style: &Style,
    scroll: &ScrollHandle,
    host: Entity<V>,
    drag: &ScrollbarDragCell,
) -> impl IntoElement {
    let people = conv.people_count();
    div()
        .w(scaled_px(style, 216.))
        .h_full()
        .flex_shrink_0()
        .bg(to_gpui(style.panel_color()))
        .border_l_1()
        .border_color(to_gpui(style.divider_color()))
        .flex()
        .flex_col()
        .min_h_0()
        .child(
            div()
                .p(scaled_px(style, 12.))
                .border_b_1()
                .border_color(to_gpui(style.divider_color()))
                .text_size(scaled_px(style, 10.))
                .text_color(to_gpui(style.muted_color()))
                .font_family(FONT_FAMILY)
                .child(format!("ONLINE - {}", people)),
        )
        .child(
            div()
                .relative()
                .flex_grow()
                .min_h_0()
                .child(
                    div()
                        .id("member-list")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(scroll)
                        .children(conv.members.iter().map(|member| member_row(member, style))),
                )
                .child(div_vertical_scrollbar(scroll, style, host, drag.clone())),
        )
}
