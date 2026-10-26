use gpui::{div, prelude::*, px, relative, MouseButton, Pixels};

use crate::model::presence::{photo_for, SelfPresence};
use crate::model::{mix_colors, Color, Style};
use crate::ui::widgets::avatar::{avatar_disc, AvatarPhotos};
use crate::ui::widgets::bot_mark::bot_mark;
use crate::ui::{Handler, BOTTOM_BAR_HEIGHT};
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};
use crate::version::version;

/// The advance of one glyph in iA Writer Mono S, per point of size. The face is
/// monospaced, so a string's width is its character count times this.
const MONO_ADVANCE_EM: f32 = 0.6;

/// The inset of the inbox and version column from the footer's right edge, so the
/// right-aligned labels do not sit against that edge.
const INBOX_RIGHT_INSET: f32 = 16.;

/// The nick the identity footer shows: the live session nick when set
/// otherwise the Connect profile nick being typed.
pub fn footer_identity_nick(live_nick: &str, connect_nick: &str) -> String {
    let live = live_nick.trim();
    if !live.is_empty() {
        return live.to_owned();
    }
    connect_nick.trim().to_owned()
}

/// Whether the footer draws the bot mark beside the nick.
pub fn footer_self_bot(nick: &str, peer_bot: bool) -> bool {
    !nick.is_empty() && peer_bot
}

/// The identity footer, a. 66 tall.
/// the focused network's avatar, nick and presence, then the `inbox` link with
/// its count and the version. The presence word and the mark come from the
/// controller through `presence`: green when available, amber when away, muted when
/// offline. The avatar is the operator's own member record, and `avatars` is the
/// `avatars` preference.
pub fn identity_footer(
    footer_nick: &str,
    footer_account: &str,
    footer_avatar: Option<&str>,
    footer_bot: bool,
    presence: SelfPresence,
    inbox_count: u32,
    on_inbox: Handler<()>,
    on_about: Handler<()>,
    update_ready: bool,
    avatars: bool,
    photos: &AvatarPhotos,
    style: &Style,
) -> impl IntoElement {
    let ink = style.ink_color();
    let muted = to_gpui(style.muted_color());
    let amount = if style.dark_mode() { 0.025 } else { 0.018 };
    let photo = photo_for(footer_avatar, avatars).and_then(|reference| photos.get(reference));
    let mark = to_gpui(style.presence_color(presence.mark()));

    // Qt's identity column: the avatar sits 17 in, the text 63 in (17, the 34 avatar and a
    // 12 gap), and the text stops 8 short of the inbox column. The inbox column ends
    // `INBOX_RIGHT_INSET` in from the right, so its labels sit off the window edge. The
    // nick and account give up that extra inset, and they still elide before the column.
    div()
        .w_full()
        .h(scaled_px(style, BOTTOM_BAR_HEIGHT))
        .flex_shrink_0()
        .pl(scaled_px(style, 17.))
        .pr(scaled_px(style, INBOX_RIGHT_INSET))
        .flex()
        .items_center()
        .bg(to_gpui(mix_colors(style.panel_color(), ink, amount)))
        .border_t_1()
        .border_color(to_gpui(style.divider_color()))
        .child(
            div()
                .relative()
                .size(scaled_px(style, 34.))
                .flex_shrink_0()
                .child(avatar_disc(style, footer_nick, 34., 14., photo))
                .child(
                    div()
                        .absolute()
                        .right(scaled_px(style, -1.))
                        .bottom(scaled_px(style, -1.))
                        .size(scaled_px(style, 9.))
                        .rounded_full()
                        .border(scaled_px(style, 2.))
                        .border_color(to_gpui(style.panel_color()))
                        .bg(mark),
                ),
        )
        .child(
            div()
                .flex_grow()
                .min_w_0()
                .ml(scaled_px(style, 12.))
                .mr(scaled_px(style, 8.))
                .flex()
                .flex_col()
                .gap(scaled_px(style, 1.))
                .child(nick_and_account(footer_nick, footer_account, footer_bot, style))
                .child(
                    div()
                        // Qt's 10px line is 14 tall, so the column keeps Qt's rhythm.
                        .line_height(scaled_px(style, 14.))
                        .text_size(scaled_px(style, 10.))
                        .whitespace_nowrap()
                        .text_color(muted)
                        .font_family(FONT_FAMILY)
                        .child(presence.word()),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .items_end()
                .gap(scaled_px(style, 4.))
                .child(inbox_link(inbox_count, on_inbox, style))
                .child(version_link(
                    if update_ready {
                        "Restart to update"
                    } else {
                        version()
                    },
                    on_about,
                    style,
                )),
        )
}

/// The nick in bold, then the account in muted 13px on the same line (Qt's `selfNickLabel`
/// and `selfAccountLabel`). The account is `min(its width, max(48, 40% of the row))`: it
/// keeps its whole width when it fits, and it never shrinks, so the nick takes the rest
/// and elides first. The account elides past its cap. It is empty when the network reports
/// none, or it only repeats the nick.
fn nick_and_account(
    nick: &str,
    account: &str,
    bot: bool,
    style: &Style,
) -> impl IntoElement {
    let ink = style.ink_color();
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        // Qt's 13px line is 17 tall: the nick row keeps Qt's height, not the default 1.6 em.
        .line_height(scaled_px(style, 17.))
        .gap(scaled_px(style, 4.))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(scaled_px(style, 13.))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(to_gpui(ink))
                .font_family(FONT_FAMILY)
                .child(nick.to_owned()),
        )
        .when(!account.is_empty(), |this| {
            this.child(
                div()
                    .flex_shrink_0()
                    .min_w(account_floor(account, style))
                    .max_w(relative(0.4))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(scaled_px(style, 13.))
                    .text_color(to_gpui(style.muted_color()))
                    .font_family(FONT_FAMILY)
                    .child(account.to_owned()),
            )
        })
        .when(bot, |this| this.child(bot_mark(style)))
}

/// Qt's floor on the account: `max(48, 40%)` caps it from below at 48, but never past the
/// account's own width, so a short account is not padded out to 48. GPUI's min width wins
/// over its max width, so this floor and the 40% cap combine as Qt's `min` does.
fn account_floor(account: &str, style: &Style) -> Pixels {
    let font_px = style.scaled(13.) as f32;
    let natural = account.chars().count() as f32 * MONO_ADVANCE_EM * font_px;
    px(natural.min(style.scaled(48.) as f32))
}

/// The version link under `inbox`. A press opens the About sheet. Hover turns
/// the text ink and underlines it, like Qt's `selfVersionHit`.
fn version_link(version: &str, on_about: Handler<()>, style: &Style) -> impl IntoElement {
    let ink = style.ink_color();
    div()
        .cursor_pointer()
        .text_size(scaled_px(style, 10.))
        .text_color(to_gpui(style.muted_color()))
        .font_family(FONT_FAMILY)
        .whitespace_nowrap()
        .hover(|s| s.text_color(to_gpui(ink)).underline())
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            on_about((), window, cx);
        })
        .child(version.to_string())
}

/// `inbox` with the accent count pill in front of it, right-aligned in the
/// footer column. A press opens or closes the Inbox sheet.
fn inbox_link(count: u32, on_inbox: Handler<()>, style: &Style) -> impl IntoElement {
    let ink = style.ink_color();
    div()
        .flex()
        .items_center()
        .gap(scaled_px(style, 4.))
        .cursor_pointer()
        .group("footer-inbox")
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            on_inbox((), window, cx);
        })
        .when(count > 0, |this| {
            this.child(
                div()
                    .min_w(scaled_px(style, 19.))
                    .h(scaled_px(style, 19.))
                    .px(scaled_px(style, 5.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(to_gpui(style.accent_color()))
                    .text_size(scaled_px(style, 10.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(to_gpui(Color::rgb(1., 1., 1.)))
                    .font_family(FONT_FAMILY)
                    .child(count.to_string()),
            )
        })
        .child(
            div()
                .text_size(scaled_px(style, 10.))
                .text_color(to_gpui(style.muted_color()))
                .font_family(FONT_FAMILY)
                .whitespace_nowrap()
                .group_hover("footer-inbox", |s| s.text_color(to_gpui(ink)).underline())
                .child("inbox"),
        )
}

#[cfg(test)]
mod tests {
    use super::{footer_identity_nick, footer_self_bot};

    #[test]
    fn identity_footer_nick_prefers_live_over_connect() {
        assert_eq!(footer_identity_nick("oak", "draft"), "oak");
    }

    #[test]
    fn identity_footer_nick_falls_back_to_connect_draft() {
        assert_eq!(footer_identity_nick("", "draft"), "draft");
        assert_eq!(footer_identity_nick("  ", " draft "), "draft");
    }

    #[test]
    fn identity_footer_bot_follows_peer_facts_with_a_nick() {
        assert!(!footer_self_bot("", true));
        assert!(!footer_self_bot("me", false));
        assert!(footer_self_bot("me", true));
    }
}
