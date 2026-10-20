use gpui::{div, prelude::*, px, MouseButton};

use crate::model::{Conversation, ConversationRef, Style};
use crate::ui::widgets::{
    avatar::avatar_with_presence, badge::unread_badge, typing_dots::typing_dots,
};
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// One channel or direct message row. A click reports `target` to `on_select`.
pub fn conversation_row(
    target: ConversationRef,
    conv: &Conversation,
    selected: bool,
    typing_phase: u8,
    style: &Style,
    on_select: Handler<ConversationRef>,
) -> impl IntoElement {
    let typing = conv.typing;
    div()
        .mx(scaled_px(style, 8.))
        .mb(scaled_px(style, 2.))
        .h(scaled_px(style, 36.))
        .rounded(scaled_px(style, 7.))
        .relative()
        .cursor_pointer()
        .bg(if selected {
            to_gpui(style.hover_color())
        } else {
            gpui::transparent_black()
        })
        .when(selected, |this| {
            this.child(
                div()
                    .absolute()
                    .left_0()
                    .top(scaled_px(style, 9.))
                    .w(px(3.))
                    .h(scaled_px(style, 18.))
                    .bg(to_gpui(style.accent_color())),
            )
        })
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            on_select(target.clone(), window, cx);
        })
        .child(
            div()
                .h_full()
                .px(scaled_px(style, 10.))
                .flex()
                .items_center()
                .gap(scaled_px(style, 8.))
                .when(!conv.channel, |this| {
                    let peer = conv.name.as_str();
                    let fill = to_gpui(style.nick_avatar_fills()[style.nick_palette_index(peer)]);
                    let presence = conv
                        .members
                        .first()
                        .map(|m| m.presence.as_str())
                        .unwrap_or("online");
                    this.child(avatar_with_presence(
                        style,
                        22.,
                        fill,
                        style.initials(peer),
                        to_gpui(style.nick_color(peer)),
                        11.,
                        presence,
                    ))
                })
                .child(
                    div()
                        .flex_grow()
                        .text_size(scaled_px(style, 13.))
                        .text_color(to_gpui(style.ink_color()))
                        .font_family(FONT_FAMILY)
                        .child(conv.name.clone()),
                )
                .when(typing, |this| this.child(typing_dots(style, typing_phase)))
                .when(conv.unread > 0, |this| {
                    this.child(unread_badge(style, conv.unread, conv.mention))
                }),
        )
}
