use gpui::{div, prelude::*, MouseButton};

use crate::model::presence::bundled_path;
use crate::model::status::network_mark_online;
use crate::model::{sidebar::NetworkHeader, Color, Style};
use crate::ui::widgets::avatar::photo_layer;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The network header, a port of `NetworkSection.qml`: 64 tall, with the
/// collapse chevron, the icon, the name with an unread dot, and the status.
/// The header itself is inert. `on_toggle` receives the network id when the
/// chevron is clicked; `on_open` receives it when the icon or name is clicked,
/// which opens that network's Status. `on_edit` receives it when the `edit`
/// label is clicked, which opens the Connect sheet on that network. The icon is
/// the network's own image when it has one, and the initial otherwise. Qt shows
/// it whatever the `avatars` preference says.
pub fn network_header(
    network: &NetworkHeader,
    focused: bool,
    collapsed: bool,
    style: &Style,
    on_toggle: Handler<String>,
    on_open: Handler<String>,
    on_edit: Handler<String>,
) -> impl IntoElement {
    let ink = to_gpui(style.ink_color());
    let muted = to_gpui(style.muted_color());
    let accent = to_gpui(style.accent_color());
    let (unread, mention) = (network.unread, network.mention);
    let has_dot = unread || mention;
    let icon_fill = to_gpui(
        network
            .icon_color
            .and_then(|ix| style.nick_palette().get(ix as usize).copied())
            .unwrap_or(style.accent_color()),
    );
    let initial = network
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());
    let icon_photo = network.icon.as_deref().and_then(bundled_path);
    // The mark is green only while the status line reads Connected (Qt's `liveStatus`).
    let mark = if network_mark_online(&network.status) {
        "online"
    } else {
        "offline"
    };
    let id = network.id.clone();
    let open_id = id.clone();
    let edit_id = id.clone();

    div()
        .relative()
        .w_full()
        .h(scaled_px(style, 64.))
        .flex_shrink_0()
        // Highlight: inset 8, radius 7. Raised when focused, hover otherwise.
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(scaled_px(style, 8.))
                .right(scaled_px(style, 8.))
                .rounded(scaled_px(style, 7.))
                .when(focused, |this| this.bg(to_gpui(style.raised_color())))
                .when(!focused, |this| {
                    this.hover(|s| s.bg(to_gpui(style.hover_color())))
                }),
        )
        .when(focused, |this| {
            this.child(
                div()
                    .absolute()
                    .left(scaled_px(style, 8.))
                    .top(scaled_px(style, 23.))
                    .w(scaled_px(style, 3.))
                    .h(scaled_px(style, 18.))
                    .rounded_full()
                    .bg(accent),
            )
        })
        // Chevron hit area: 14 x 28 at x 12, centred vertically.
        .child(
            div()
                .absolute()
                .left(scaled_px(style, 12.))
                .top(scaled_px(style, 18.))
                .w(scaled_px(style, 14.))
                .h(scaled_px(style, 28.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .text_size(scaled_px(style, 11.))
                .text_color(muted)
                .font_family(FONT_FAMILY)
                .hover(|s| s.text_color(ink))
                .child(if collapsed { "▸" } else { "▾" })
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    on_toggle(id.clone(), window, cx);
                }),
        )
        // Icon: 28 square, radius 8, at x 30. A click opens the network's Status.
        .child({
            let on_open = on_open.clone();
            let open_id = open_id.clone();
            div()
                .absolute()
                .left(scaled_px(style, 30.))
                .top(scaled_px(style, 18.))
                .size(scaled_px(style, 28.))
                .rounded(scaled_px(style, 8.))
                .bg(icon_fill)
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .text_size(scaled_px(style, 14.))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(to_gpui(Color::rgb(1., 1., 1.)))
                .font_family(FONT_FAMILY)
                .child(initial)
                .when_some(icon_photo, |this, path| {
                    this.child(photo_layer(path).rounded(scaled_px(style, 8.)))
                })
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    on_open(open_id.clone(), window, cx);
                })
        })
        // Name and status, at x 68, centred vertically.
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(scaled_px(style, 68.))
                .right(scaled_px(style, 46.))
                .flex()
                .flex_col()
                .justify_center()
                .gap(scaled_px(style, 2.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(scaled_px(style, 6.))
                        .min_w_0()
                        .child(
                            div()
                                .flex_grow()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_size(scaled_px(style, 13.))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(ink)
                                .font_family(FONT_FAMILY)
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                    on_open(open_id.clone(), window, cx);
                                })
                                .child(network.name.clone()),
                        )
                        .when(has_dot, |this| {
                            this.child(
                                div()
                                    .flex_shrink_0()
                                    .size(scaled_px(style, 7.))
                                    .rounded_full()
                                    .bg(if mention { accent } else { ink }),
                            )
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(scaled_px(style, 6.))
                        .min_w_0()
                        .child(
                            div()
                                .flex_shrink_0()
                                .size(scaled_px(style, 7.))
                                .rounded_full()
                                .bg(to_gpui(style.presence_color(mark))),
                        )
                        .child(
                            div()
                                .flex_grow()
                                .min_w_0()
                                .truncate()
                                .text_size(scaled_px(style, 10.))
                                .text_color(muted)
                                .font_family(FONT_FAMILY)
                                .child(network.status.clone()),
                        ),
                ),
        )
        // The `edit` label at the right edge. A click opens Connect on this network.
        .child(
            div()
                .absolute()
                .right(scaled_px(style, 10.))
                .top(scaled_px(style, 18.))
                .w(scaled_px(style, 28.))
                .h(scaled_px(style, 28.))
                .flex()
                .items_center()
                .justify_center()
                .text_size(scaled_px(style, 9.))
                .text_color(muted)
                .font_family(FONT_FAMILY)
                .cursor_pointer()
                .hover(|s| s.text_color(ink))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    on_edit(edit_id.clone(), window, cx);
                })
                .child("edit"),
        )
}
