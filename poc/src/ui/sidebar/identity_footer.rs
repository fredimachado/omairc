use gpui::{div, prelude::*, MouseButton};

use crate::model::{mix_colors, Color, Network, Style};
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// `version.pri` at compile time, so the footer matches Qt's `--version`.
const VERSION_PRI: &str = include_str!("../../../../version.pri");

/// The app version shown in the footer and the About sheet.
pub fn app_version() -> &'static str {
    parse_version(VERSION_PRI).unwrap_or("")
}

/// The `VERSION = x.y.z` value from a `version.pri` body.
pub fn parse_version(pri: &str) -> Option<&str> {
    pri.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix("VERSION")?;
        let value = rest.trim_start().strip_prefix('=')?.trim();
        (!value.is_empty()).then_some(value)
    })
}

/// The identity footer, a port of the bottom of `ServerListColumn.qml`: 66 tall,
/// the selected network's nick and presence, then the `inbox` link with its
/// count and the version.
pub fn identity_footer(
    network: &Network,
    inbox_count: u32,
    on_inbox: Handler<()>,
    on_about: Handler<()>,
    style: &Style,
) -> impl IntoElement {
    let ink = style.ink_color();
    let muted = to_gpui(style.muted_color());
    let amount = if style.dark_mode() { 0.025 } else { 0.018 };
    let fill_index = style.nick_palette_index(&network.nick);
    let glyph_fill = to_gpui(style.nick_avatar_fills()[fill_index]);
    let version = app_version();

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
                .child(
                    div()
                        .size_full()
                        .rounded_full()
                        .bg(glyph_fill)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(scaled_px(style, 12.))
                        .text_color(to_gpui(style.nick_color(&network.nick)))
                        .font_family(FONT_FAMILY)
                        .child(style.initials(&network.nick)),
                )
                .child(
                    div()
                        .absolute()
                        .right(scaled_px(style, -1.))
                        .bottom(scaled_px(style, -1.))
                        .size(scaled_px(style, 9.))
                        .rounded_full()
                        .border(scaled_px(style, 2.))
                        .border_color(to_gpui(style.panel_color()))
                        .bg(to_gpui(style.presence_color("online"))),
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
                        .child("available"),
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
                .child(version_link(version, on_about, style)),
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

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn version_comes_from_the_version_pri_value() {
        assert_eq!(parse_version("# note\nVERSION = 1.10.3\nOTHER = x\n"), Some("1.10.3"));
        assert_eq!(parse_version("VERSION=2.0.0"), Some("2.0.0"));
        assert_eq!(parse_version("VERSION = \n"), None);
        assert_eq!(parse_version("OMAIRC_VERSION_HEADER = x"), None);
    }

    #[test]
    fn the_shipped_version_pri_parses() {
        assert!(parse_version(super::VERSION_PRI).is_some());
    }
}
