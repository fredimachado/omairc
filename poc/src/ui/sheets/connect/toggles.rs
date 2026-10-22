//! The switches on the Connection tab (TLS, connect on startup) and the Preferences
//! tab (`PreferenceToggleRow.qml`). A switch is a stop: a click or Space flips it,
//! and Enter walks on, as the model's focus walk says. A preference flips at once
//! through `Shell::set_connect_preference`, with no Apply.

use gpui::{div, prelude::*, transparent_black, white, FontWeight, MouseButton};

use crate::model::connect::{preference_rows, PreferenceRow, Stop, PREFERENCES_TITLE, TLS_LABEL};
use crate::ui::sheets::connect::field::{caption, help_mark};
use crate::ui::sheets::connect::ConnectView;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// A 30 by 16 track with a knob, accent when on. Focus rings the track.
pub fn switch(view: &ConnectView<'_>, stop: Stop, on: bool) -> impl IntoElement {
    let style = view.style;
    let focused = view.is_focused(&stop);
    let mut track = div()
        .relative()
        .w(scaled_px(style, 30.))
        .h(scaled_px(style, 16.))
        .flex_shrink_0()
        .rounded(scaled_px(style, 8.))
        .cursor_pointer()
        .bg(if on {
            to_gpui(style.accent_color())
        } else {
            to_gpui(style.divider_color())
        })
        .border_1()
        .border_color(if focused {
            to_gpui(style.accent_color())
        } else {
            transparent_black()
        })
        .on_mouse_down(MouseButton::Left, {
            let click = view.click(stop.clone());
            move |_, window, cx| click(window, cx)
        })
        .child(
            div()
                .absolute()
                .top(scaled_px(style, 1.))
                .left(scaled_px(style, if on { 15. } else { 1. }))
                .w(scaled_px(style, 12.))
                .h(scaled_px(style, 12.))
                .rounded(scaled_px(style, 6.))
                .bg(white()),
        );
    if let Some(handle) = view.handle(&stop) {
        track = track.track_focus(handle);
    }
    track
}

/// The TLS switch with its `TLS` label above it, as the host row shows it.
pub fn tls_switch(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    div()
        .w(scaled_px(style, 72.))
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(scaled_px(style, 4.))
        .child(
            div()
                .text_size(scaled_px(style, 10.))
                .text_color(to_gpui(style.muted_color()))
                .font_family(FONT_FAMILY)
                .child(TLS_LABEL),
        )
        .child(switch(view, Stop::Tls, view.state.tls()))
}

/// The Preferences tab: its title, then one row per toggle, 8px apart.
pub fn preferences_panel(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    let mut panel = div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .gap(scaled_px(style, 8.))
        .px(scaled_px(style, 18.))
        .pt(scaled_px(style, 18.))
        .child(
            div()
                .text_size(scaled_px(style, 15.))
                .font_weight(FontWeight::BOLD)
                .text_color(to_gpui(style.ink_color()))
                .font_family(FONT_FAMILY)
                .child(PREFERENCES_TITLE),
        );
    for row in preference_rows() {
        panel = panel.child(preference_row(view, row));
    }
    panel
}

/// One preference: its switch, its label and help mark, and the help caption
/// under them while the mark has focus.
fn preference_row(view: &ConnectView<'_>, row: PreferenceRow) -> impl IntoElement {
    let style = view.style;
    let on = view.prefs.enabled(row.name);
    let help_stop = Stop::PreferenceHelp(row.name);
    let mut column = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(scaled_px(style, 4.))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(scaled_px(style, 10.))
                .child(switch(view, Stop::Preference(row.name), on))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(scaled_px(style, 6.))
                        .child(
                            div()
                                .text_size(scaled_px(style, 11.))
                                .text_color(to_gpui(style.ink_color()))
                                .font_family(FONT_FAMILY)
                                .child(row.label),
                        )
                        .child(help_mark(view, help_stop.clone(), row.help)),
                ),
        );
    if view.is_focused(&help_stop) {
        column = column.child(
            div()
                .pl(scaled_px(style, 40.))
                .child(caption(view, row.help)),
        );
    }
    column
}
