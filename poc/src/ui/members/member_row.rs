use gpui::{div, prelude::*, MouseButton};

use crate::model::{Member, Style};
use crate::ui::widgets::avatar::avatar_with_presence;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// One nick in the member list: presence glyph, prefixed nick and real name.
/// A click on anyone but `self_nick` reports the nick to `on_nick`. `current`
/// draws the keyboard cursor's row raised.
pub fn member_row(
    member: &Member,
    style: &Style,
    self_nick: &str,
    on_nick: Handler<String>,
    current: bool,
) -> impl IntoElement {
    let nick_color = to_gpui(style.nick_color(&member.nick));
    let fill = to_gpui(style.nick_avatar_fills()[style.nick_palette_index(&member.nick)]);
    let clickable = member.nick != self_nick;
    let nick = member.nick.clone();
    div()
        .px(scaled_px(style, 10.))
        .py(scaled_px(style, 6.))
        .flex()
        .gap(scaled_px(style, 8.))
        .when(current, |this| this.bg(to_gpui(style.raised_color())))
        .when(clickable, |this| {
            this.cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    on_nick(nick.clone(), window, cx);
                })
        })
        .child(avatar_with_presence(
            style,
            28.,
            fill,
            style.initials(&member.nick),
            nick_color,
            11.,
            &member.presence,
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(scaled_px(style, 12.))
                        .text_color(nick_color)
                        .font_family(FONT_FAMILY)
                        .child(format!("{}{}", member.prefix, member.nick)),
                )
                .child(
                    div()
                        .text_size(scaled_px(style, 10.))
                        .text_color(to_gpui(style.muted_color()))
                        .font_family(FONT_FAMILY)
                        .child(member.realname.clone()),
                ),
        )
}
