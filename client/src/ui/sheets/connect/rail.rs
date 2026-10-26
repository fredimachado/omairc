//! The network rail on the left of the Connection tab (`client/src/ui/sheets/connect/`
//! `networkRail`): the saved networks and the draft, the `+ Add network` row, and
//! the `Shortcuts` hint at its foot.
//!
//! The rows scroll inside their own list. Its children are the rail stops in
//! order, the rows and then the Add row, so `ScrollHandle::scroll_to_item` with a
//! stop's index brings it into view (`Shell::reveal_connect`).

use gpui::{div, prelude::*, FontWeight, MouseButton};

use crate::model::connect::{
    RailRow, Stop, ADD_NETWORK_LABEL, NETWORKS_LABEL, SHORTCUTS_KEYS, SHORTCUTS_LABEL,
};
use crate::model::mix_colors;
use crate::model::shortcuts::shortcut_keys;
use crate::ui::sheets::connect::ConnectView;
use crate::ui::sheets::shortcuts::host_platform;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The rail, 170px wide. Its list takes the height the rail has, and the hint sits
/// below it.
pub fn render_rail(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    let mut list = div()
        .id("connect-rail-list")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(&view.controls.rail_scroll)
        .flex()
        .flex_col()
        .gap(scaled_px(style, 6.));
    for row in view.state.rows() {
        list = list.child(network_row(view, &row));
    }
    if view.state.can_add() {
        list = list.child(add_row(view));
    }

    div()
        .w(scaled_px(style, 170.))
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(scaled_px(style, 6.))
        .child(
            div()
                .text_size(scaled_px(style, 10.))
                .font_weight(FontWeight::BOLD)
                .text_color(to_gpui(style.muted_color()))
                .font_family(FONT_FAMILY)
                .child(NETWORKS_LABEL),
        )
        .child(list)
        .child(shortcuts_hint(view))
}

/// A saved network or the draft. The selected row takes the selection tint, and a
/// focused one the hover tint. A row's label is cut with an ellipsis.
fn network_row(view: &ConnectView<'_>, row: &RailRow) -> impl IntoElement {
    let style = view.style;
    let stop = Stop::Network(row.network_id.clone());
    let focused = view.is_focused(&stop);
    let hover = to_gpui(style.hover_color());
    let selected_fill = mix_colors(
        style.selection_color(),
        style.raised_color(),
        if style.dark_mode() { 0.45 } else { 0.35 },
    );
    let fill = if row.selected {
        to_gpui(selected_fill)
    } else if focused {
        hover
    } else {
        gpui::transparent_black()
    };
    let mut item = div()
        .h(scaled_px(style, 32.))
        .w_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .px(scaled_px(style, 8.))
        .rounded(scaled_px(style, 6.))
        .bg(fill)
        .cursor_pointer()
        .font_family(FONT_FAMILY)
        .text_size(scaled_px(style, 11.))
        .text_color(to_gpui(style.ink_color()))
        .on_mouse_down(MouseButton::Left, {
            let click = view.click(stop.clone());
            move |_, window, cx| click(window, cx)
        })
        .child(div().w_full().truncate().child(row.label.clone()));
    if !row.selected {
        item = item.hover(move |s| s.bg(hover));
    }
    if let Some(handle) = view.handle(&stop) {
        item = item.track_focus(handle);
    }
    item
}

/// `+ Add network`, shown while the selection is saved and clean. A focus ring
/// marks it, as Qt's border does.
fn add_row(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    let stop = Stop::AddNetwork;
    let focused = view.is_focused(&stop);
    let hover = to_gpui(style.hover_color());
    let mut item = div()
        .h(scaled_px(style, 28.))
        .w_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(scaled_px(style, 6.))
        .border_1()
        .border_color(if focused {
            to_gpui(style.accent_color())
        } else {
            gpui::transparent_black()
        })
        .bg(if focused {
            hover
        } else {
            gpui::transparent_black()
        })
        .hover(move |s| s.bg(hover))
        .cursor_pointer()
        .font_family(FONT_FAMILY)
        .text_size(scaled_px(style, 11.))
        .text_color(to_gpui(style.muted_color()))
        .on_mouse_down(MouseButton::Left, {
            let click = view.click(stop.clone());
            move |_, window, cx| click(window, cx)
        })
        .child(ADD_NETWORK_LABEL);
    if let Some(handle) = view.handle(&stop) {
        item = item.track_focus(handle);
    }
    item
}

/// `Shortcuts` with the `Ctrl + /` chip at its right, the Qt hint that points new
/// users at the shortcuts sheet. A click or Enter opens that sheet.
fn shortcuts_hint(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    let stop = Stop::ShortcutsHint;
    let focused = view.is_focused(&stop);
    let hover = to_gpui(style.hover_color());
    let keys = shortcut_keys(SHORTCUTS_KEYS, host_platform());
    let chip = div()
        .h(scaled_px(style, 18.))
        .px(scaled_px(style, 5.))
        .flex()
        .items_center()
        .rounded(scaled_px(style, 4.))
        .bg(to_gpui(style.raised_color()))
        .border_1()
        .border_color(to_gpui(style.divider_color()))
        .text_size(scaled_px(style, 10.))
        .text_color(to_gpui(style.ink_color()))
        .child(keys);
    let mut hint = div()
        .h(scaled_px(style, 30.))
        .w_full()
        .flex_shrink_0()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .pl(scaled_px(style, 9.))
        .pr(scaled_px(style, 7.))
        .rounded(scaled_px(style, 7.))
        .border_1()
        .border_color(if focused {
            to_gpui(style.accent_color())
        } else {
            to_gpui(style.divider_color())
        })
        .bg(to_gpui(style.panel_color()))
        .hover(move |s| s.bg(hover))
        .cursor_pointer()
        .font_family(FONT_FAMILY)
        .on_mouse_down(MouseButton::Left, {
            let click = view.click(stop.clone());
            move |_, window, cx| click(window, cx)
        })
        .child(
            div()
                .text_size(scaled_px(style, 10.))
                .text_color(to_gpui(style.muted_color()))
                .child(SHORTCUTS_LABEL),
        )
        .child(chip);
    if let Some(handle) = view.handle(&stop) {
        hint = hint.track_focus(handle);
    }
    hint
}
