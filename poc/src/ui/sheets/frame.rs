//! The two overlay looks Qt uses. Both cover the window, swallow clicks and
//! wheel so nothing behind reacts, and centre a card.

use gpui::{div, prelude::*, App, Window};

use crate::model::Style;
use crate::util::to_gpui;

/// A Qt modal `Popup` (Jump, Link, Nick, Inbox, Shortcuts): the card is centred
/// over the Material modal dim. Any press outside the card dismisses it.
pub fn popup_frame(
    style: &Style,
    card: impl IntoElement,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .absolute()
        .size_full()
        .occlude()
        .bg(to_gpui(style.popup_dim_color()))
        .flex()
        .items_center()
        .justify_center()
        .on_any_mouse_down(move |_, window, cx| on_dismiss(window, cx))
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(card_shield(card))
}

/// The About look: a full-window `overlay_veil_color` with a centred card.
/// Only a left click on the veil dismisses; right clicks and card clicks do not.
pub fn veil_frame(
    style: &Style,
    card: impl IntoElement,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .absolute()
        .size_full()
        .occlude()
        .bg(to_gpui(style.overlay_veil_color()))
        .flex()
        .items_center()
        .justify_center()
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            on_dismiss(window, cx)
        })
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(card_shield(card))
}

/// Keeps presses on the card from reaching the dismissing backdrop.
fn card_shield(card: impl IntoElement) -> impl IntoElement {
    div()
        .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
        .child(card)
}
