use gpui::{div, prelude::*, MouseButton};

use crate::model::{Conversation, Network, Style};
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The conversation title, topic, and the `N PEOPLE` pill for channels.
pub fn render_header(
    conv: &Conversation,
    network: &Network,
    style: &Style,
    on_toggle_members: Handler<()>,
) -> impl IntoElement {
    let header_label = if conv.channel {
        format!("{} · {} · {}", conv.name, network.display_name, network.nick)
    } else {
        conv.name.clone()
    };
    let topic = conv.topic.clone().unwrap_or_default();
    let people = conv.people_count();
    let show_people = conv.channel;

    div()
        .px(scaled_px(style, 16.))
        .py(scaled_px(style, 10.))
        .border_b_1()
        .border_color(to_gpui(style.divider_color()))
        .flex()
        .flex_col()
        .gap(scaled_px(style, 4.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(scaled_px(style, 8.))
                .child(
                    div()
                        .flex_grow()
                        .text_size(scaled_px(style, 14.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(to_gpui(style.ink_color()))
                        .font_family(FONT_FAMILY)
                        .child(header_label),
                )
                .when(show_people, |this| {
                    this.child(
                        div()
                            .px(scaled_px(style, 8.))
                            .py(scaled_px(style, 3.))
                            .rounded_full()
                            .border_1()
                            .border_color(to_gpui(style.divider_color()))
                            .cursor_pointer()
                            .text_size(scaled_px(style, 10.))
                            .text_color(to_gpui(style.muted_color()))
                            .font_family(FONT_FAMILY)
                            .child(format!("{} PEOPLE", people))
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                on_toggle_members((), window, cx);
                            }),
                    )
                }),
        )
        .child(
            div()
                .text_size(scaled_px(style, 11.))
                .text_color(to_gpui(style.muted_color()))
                .font_family(FONT_FAMILY)
                .child(topic),
        )
}
