//! The Connect sheet's window: the veil and the card. This file is the render
//! hook the sheet UI replaces. The tabbed card, the network rail, the form and
//! the Preferences switches read `ConnectState` (`model/connect.rs`), and act
//! through the `Shell` methods in `ui/shell/connect.rs`.
//!
//! Until then it draws the veil and a stub card, and tracks the sheet's focus
//! handle, so the keyboard reaches the window while the sheet shows. Escape,
//! Ctrl+Enter, Ctrl+Tab, Alt+Left/Right, Ctrl+N and Ctrl+Shift+Delete already
//! work through the shell.

use gpui::{div, prelude::*, px, AnyElement, FocusHandle};

use crate::model::connect::{ConnectState, TITLE};
use crate::model::Style;
use crate::ui::sheets::frame::veil_frame;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The placeholder card over the veil. `focus` is the sheet's focus handle.
/// The veil swallows presses, so nothing behind the sheet reacts.
pub fn render_connect(state: &ConnectState, style: &Style, focus: &FocusHandle) -> AnyElement {
    let selected = state
        .rows()
        .into_iter()
        .find(|row| row.selected)
        .map(|row| row.label)
        .unwrap_or_default();
    let card = div()
        .track_focus(focus)
        .w(scaled_px(style, 860.))
        .max_w(px(860.))
        .p(scaled_px(style, 18.))
        .rounded(scaled_px(style, 10.))
        .bg(to_gpui(style.raised_color()))
        .border_1()
        .border_color(to_gpui(style.divider_color()))
        .font_family(FONT_FAMILY)
        .text_color(to_gpui(style.ink_color()))
        .child(div().text_size(scaled_px(style, 15.)).child(TITLE))
        .child(
            div()
                .mt(scaled_px(style, 8.))
                .text_size(scaled_px(style, 11.))
                .text_color(to_gpui(style.muted_color()))
                .child(format!("{selected} · sheet UI pending")),
        );
    veil_frame(style, card, |_, _| {}).into_any_element()
}
