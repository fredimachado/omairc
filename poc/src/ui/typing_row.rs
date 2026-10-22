//! The typing dots that end a DM transcript (Qt's `typingRow` in
//! `OmaircWindow.qml`). With `continues` they sit in the body column under the
//! peer's last line. Otherwise they take the avatar and nick of a group of their
//! own. The dots are the last item of the transcript list, after the last row,
//! so they are not a transcript row: find, copy and pointer hits never see them.

use gpui::{div, prelude::*, Div};

use crate::model::presence::photo_for;
use crate::model::Style;
use crate::ui::message_row::RowPeers;
use crate::ui::widgets::avatar::avatar_disc;
use crate::ui::widgets::bot_mark::bot_mark;
use crate::ui::widgets::typing_dots::typing_dots_sized;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// Qt's transcript `TypingDots` is a 16px face.
const DOTS_FACE_PX: f32 = 16.;

/// The peer's header takes the peer's photo and bot mark from `peers`, as Qt's
/// typing row takes them from `peerAvatar` and `peerBot`.
pub fn render_typing_row(
    nick: &str,
    continues: bool,
    phase: u8,
    peers: &RowPeers,
    style: &Style,
) -> impl IntoElement {
    let facts = peers.facts.get(nick);
    let bot = facts.is_some_and(|facts| facts.bot);
    let photo = photo_for(facts.and_then(|facts| facts.avatar.as_deref()), peers.avatars)
        .and_then(|reference| peers.photos.get(reference));
    let left_pad = scaled_px(style, 24.);
    let top_pad = scaled_px(style, 9.);
    // The same slot a message takes (`render_message_row`): the header 9 down, the
    // dots' line 29 down (4 in a group), and the row's minimum height.
    div()
        .w_full()
        .relative()
        .min_h(scaled_px(style, if continues { 22. } else { 58. }))
        .when(!continues, |this| {
            this.child(
                avatar_disc(style, nick, 34., 13., photo)
                    .absolute()
                    .left(left_pad)
                    .top(top_pad),
            )
        })
        .child(
            div()
                .pl(scaled_px(style, 70.))
                .pr(scaled_px(style, 34.))
                .pt(scaled_px(style, if continues { 4. } else { 9. }))
                .pb(scaled_px(style, if continues { 4. } else { 10. }))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .when(!continues, |this| {
                            this.child(nick_header(nick, bot, style))
                        })
                        .child(
                            div()
                                .when(!continues, |this| this.pt(scaled_px(style, 4.)))
                                .flex()
                                .items_center()
                                // The dots centre in one message line, 17 tall.
                                .h(scaled_px(style, 17.))
                                .child(typing_dots_sized(style, phase, DOTS_FACE_PX)),
                        ),
                ),
        )
}

/// The nick over the dots, in its bold nick colour: a message header without a time.
/// A bot gets its mark after the nick. It is the header's 16-tall line.
fn nick_header(nick: &str, bot: bool, style: &Style) -> Div {
    div()
        .h(scaled_px(style, 16.))
        .line_height(scaled_px(style, 16.))
        .flex()
        .items_center()
        .child(
            div()
                .text_size(scaled_px(style, 12.))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(to_gpui(style.nick_color(nick)))
                .font_family(FONT_FAMILY)
                .child(nick.to_string()),
        )
        .when(bot, |this| {
            this.child(div().ml(scaled_px(style, 4.)).child(bot_mark(style)))
        })
}
