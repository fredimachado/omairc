use gpui::{div, prelude::*, MouseButton};

use crate::model::{Conversation, ConversationRef, Style};
use crate::ui::widgets::{
    avatar::avatar_with_presence, badge::unread_badge, typing_dots::typing_dots,
};
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// One channel or direct message row, a port of `ConversationRow.qml`: 36 tall,
/// inset 8. A click reports `target` to `on_select`.
pub fn conversation_row(
    target: ConversationRef,
    conv: &Conversation,
    selected: bool,
    typing_phase: u8,
    style: &Style,
    on_select: Handler<ConversationRef>,
) -> impl IntoElement {
    let ink = to_gpui(style.ink_color());
    let muted = to_gpui(style.muted_color());
    let unread = conv.unread > 0;
    let emphasized = selected || unread;
    let label_color = if emphasized { ink } else { muted };
    let label = if conv.channel {
        format!("#  {}", conv.name.replace('#', ""))
    } else {
        conv.name.clone()
    };
    let label_left = if conv.channel { 20. } else { 49. };
    // Keep the label clear of the badge and typing dots at the right edge.
    let label_right = if unread || (conv.typing && !conv.channel) {
        40.
    } else {
        8.
    };
    let typing = conv.typing && !conv.channel;

    div()
        .w_full()
        .h(scaled_px(style, 36.))
        .flex_shrink_0()
        .relative()
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            on_select(target.clone(), window, cx);
        })
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(scaled_px(style, 8.))
                .right(scaled_px(style, 8.))
                .rounded(scaled_px(style, 7.))
                .when(selected, |this| this.bg(to_gpui(style.raised_color())))
                .when(!selected, |this| {
                    this.hover(|s| s.bg(to_gpui(style.hover_color())))
                }),
        )
        .when(selected, |this| {
            this.child(
                div()
                    .absolute()
                    .left(scaled_px(style, 8.))
                    .top(scaled_px(style, 9.))
                    .w(scaled_px(style, 3.))
                    .h(scaled_px(style, 18.))
                    .rounded_full()
                    .bg(to_gpui(style.accent_color())),
            )
        })
        .when(!conv.channel, |this| {
            let fill = to_gpui(style.nick_avatar_fills()[style.nick_palette_index(&conv.name)]);
            let presence = conv
                .members
                .first()
                .map(|m| m.presence.as_str())
                .unwrap_or("online");
            this.child(
                div()
                    .absolute()
                    .left(scaled_px(style, 18.))
                    .top(scaled_px(style, 7.))
                    .child(avatar_with_presence(
                        style,
                        22.,
                        fill,
                        style.initials(&conv.name),
                        to_gpui(style.nick_color(&conv.name)),
                        11.,
                        presence,
                    )),
            )
        })
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(scaled_px(style, label_left))
                .right(scaled_px(style, label_right))
                .flex()
                .items_center()
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_size(scaled_px(style, 13.))
                        .when(emphasized, |this| {
                            this.font_weight(gpui::FontWeight::BOLD)
                        })
                        .text_color(label_color)
                        .font_family(FONT_FAMILY)
                        .child(label),
                ),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right(scaled_px(style, 17.))
                .flex()
                .items_center()
                .gap(scaled_px(style, 6.))
                .when(typing, |this| this.child(typing_dots(style, typing_phase)))
                .when(unread, |this| {
                    this.child(unread_badge(style, conv.unread, conv.mention))
                }),
        )
}
