use gpui::{div, prelude::*, MouseButton};

use crate::model::presence::{photo_for, SelfPresence};
use crate::model::{mix_colors, sidebar::NetworkHeader, Color, Style};
use crate::ui::widgets::avatar::avatar_disc;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};
use crate::version::version;

/// The identity footer, a port of the bottom of `ServerListColumn.qml`: 66 tall,
/// the focused network's avatar, nick and presence, then the `inbox` link with
/// its count and the version. The presence word and the mark come from the
/// controller through `presence`: green when available, amber when away, muted when
/// offline. The avatar is the operator's own member record, and `avatars` is the
/// `avatars` preference.
pub fn identity_footer(
    network: &NetworkHeader,
    presence: SelfPresence,
    inbox_count: u32,
    on_inbox: Handler<()>,
    on_about: Handler<()>,
    update_ready: bool,
    avatars: bool,
    style: &Style,
) -> impl IntoElement {
    let ink = style.ink_color();
    let muted = to_gpui(style.muted_color());
    let amount = if style.dark_mode() { 0.025 } else { 0.018 };
    let photo = photo_for(network.avatar.as_deref(), avatars);
    let mark = to_gpui(style.presence_color(presence.mark()));

    div()
        .w_full()
        .h(scaled_px(style, 66.))
        .flex_shrink_0()
        .px(scaled_px(style, 17.))
        .flex()
        .items_center()
        .gap(scaled_px(style, 12.))
        .bg(to_gpui(mix_colors(style.panel_color(), ink, amount)))
        .border_t_1()
        .border_color(to_gpui(style.divider_color()))
        .child(
            div()
                .relative()
                .size(scaled_px(style, 34.))
                .flex_shrink_0()
                .child(avatar_disc(style, &network.nick, 34., 14., photo))
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
                .flex()
                .flex_col()
                .gap(scaled_px(style, 1.))
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(scaled_px(style, 13.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(to_gpui(ink))
                        .font_family(FONT_FAMILY)
                        .child(network.nick.clone()),
                )
                .child(
                    div()
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
                .pr(scaled_px(style, 8.))
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

/// The version link under `inbox`. A press opens the About sheet. Hover turns
/// the text ink and underlines it, like Qt's `selfVersionHit`.
fn version_link(version: &str, on_about: Handler<()>, style: &Style) -> impl IntoElement {
    let ink = style.ink_color();
    div()
        .pr(scaled_px(style, 9.))
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
        .pr(scaled_px(style, 9.))
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
