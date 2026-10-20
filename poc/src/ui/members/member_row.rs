use gpui::{div, prelude::*, MouseButton};

use crate::model::presence::photo_for;
use crate::model::{Member, Style};
use crate::ui::widgets::avatar::avatar_with_presence;
use crate::ui::widgets::bot_mark::bot_mark;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// Qt's dimmed glyph for an away member.
const AWAY_GLYPH_OPACITY: f32 = 0.62;

/// One 43px member row: inset 8 with a radius of 7, `hover` on hover and
/// `raised` for the keyboard cursor. The glyph sits at x 18 and the label at
/// x 56, bold for self and muted when away. A bot gets its mark after the nick.
/// The status line sits under the label, muted and 9px. It never dims the row:
/// only the glyph and the label take the away treatment. A click on anyone but
/// `self_nick` reports the nick to `on_nick`, which also gets the pointer cursor.
pub fn member_row(
    member: &Member,
    style: &Style,
    self_nick: &str,
    on_nick: Handler<String>,
    current: bool,
    avatars: bool,
) -> impl IntoElement {
    let is_self = member.nick == self_nick;
    let away = member.presence == "away";
    let clickable = !is_self;
    let nick = member.nick.clone();
    let photo = photo_for(member.avatar.as_deref(), avatars);
    let glyph = avatar_with_presence(
        style,
        &member.nick,
        28.,
        11.,
        photo,
        &member.presence,
    );
    let label_color = if away {
        to_gpui(style.muted_color())
    } else {
        to_gpui(style.ink_color())
    };
    let hover = to_gpui(style.hover_color());
    let raised = to_gpui(style.raised_color());

    let label = div()
        .min_w_0()
        .overflow_hidden()
        .text_ellipsis()
        .text_size(scaled_px(style, 12.))
        .text_color(label_color)
        .font_family(FONT_FAMILY)
        .when(is_self, |this| this.font_weight(gpui::FontWeight::BOLD))
        .child(format!("{}{}", member.prefix, member.nick));
    let label_row = div()
        .flex()
        .items_center()
        .gap(scaled_px(style, 4.))
        .min_w_0()
        .child(label)
        .when(member.bot, |this| this.child(bot_mark(style)));
    let status = member
        .status
        .as_deref()
        .filter(|status| !status.is_empty())
        .map(|status| status.to_string());

    div()
        .h(scaled_px(style, 43.))
        .w_full()
        .px(scaled_px(style, 8.))
        .when(clickable, |this| {
            this.cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    on_nick(nick.clone(), window, cx);
                })
        })
        .child(
            div()
                .size_full()
                .rounded(scaled_px(style, 7.))
                .flex()
                .items_center()
                .pl(scaled_px(style, 10.))
                .when(current, |this| this.bg(raised))
                .hover(|s| s.bg(hover))
                .child(
                    div()
                        .flex_shrink_0()
                        .when(away, |this| this.opacity(AWAY_GLYPH_OPACITY))
                        .child(glyph),
                )
                .child(
                    div()
                        .flex_grow()
                        .min_w_0()
                        .ml(scaled_px(style, 10.))
                        .mr(scaled_px(style, 10.))
                        .flex()
                        .flex_col()
                        .child(label_row)
                        .when_some(status, |this, status| {
                            this.child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_size(scaled_px(style, 9.))
                                    .text_color(to_gpui(style.muted_color()))
                                    .font_family(FONT_FAMILY)
                                    .child(status),
                            )
                        }),
                ),
        )
}
