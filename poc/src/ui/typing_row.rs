//! The typing dots that end a DM transcript (Qt's `typingRow` in
//! `OmaircWindow.qml`). With `continues` they sit in the body column under the
//! peer's last line. Otherwise they take the avatar and nick of a group of their
//! own. The dots are the last item of the transcript list, after the last row,
//! so they are not a transcript row: find, copy and pointer hits never see them.

use gpui::{div, prelude::*, Div};

use crate::model::Style;
use crate::ui::message_row::avatar_disc;
use crate::ui::widgets::typing_dots::typing_dots_sized;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// Qt's transcript `TypingDots` is a 16px face.
const DOTS_FACE_PX: f32 = 16.;

pub fn render_typing_row(
    nick: &str,
    continues: bool,
    phase: u8,
    style: &Style,
) -> impl IntoElement {
    let left_pad = scaled_px(style, 24.);
    let top_pad = scaled_px(style, 9.);
    let avatar_size = scaled_px(style, 34.);
    div()
        .w_full()
        .relative()
        .when(!continues, |this| {
            this.child(
                avatar_disc(style, nick, avatar_size)
                    .absolute()
                    .left(left_pad)
                    .top(top_pad),
            )
        })
        .child(
            div()
                .pl(scaled_px(style, 70.))
                .pr(scaled_px(style, 34.))
                .pt(scaled_px(style, if continues { 4. } else { 6. }))
                .pb(scaled_px(style, 6.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(scaled_px(style, 2.))
                        .when(!continues, |this| this.child(nick_header(nick, style)))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .h(scaled_px(style, 18.))
                                .child(typing_dots_sized(style, phase, DOTS_FACE_PX)),
                        ),
                ),
        )
}

/// The nick over the dots, in its bold nick colour: a message header without a time.
fn nick_header(nick: &str, style: &Style) -> Div {
    div().flex().items_center().child(
        div()
            .text_size(scaled_px(style, 12.))
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(to_gpui(style.nick_color(nick)))
            .font_family(FONT_FAMILY)
            .child(nick.to_string()),
    )
}
