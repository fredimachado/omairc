use gpui::{div, prelude::*};

use crate::model::{Member, Style};
use crate::ui::widgets::avatar::avatar_with_presence;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// One nick in the member list: presence glyph, prefixed nick and real name.
pub fn member_row(member: &Member, style: &Style) -> impl IntoElement {
    let nick_color = to_gpui(style.nick_color(&member.nick));
    let fill = to_gpui(style.nick_avatar_fills()[style.nick_palette_index(&member.nick)]);
    div()
        .px(scaled_px(style, 10.))
        .py(scaled_px(style, 6.))
        .flex()
        .gap(scaled_px(style, 8.))
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
