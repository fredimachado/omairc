//! The two overlay looks Qt uses. Both cover the window, swallow clicks and
//! wheel so nothing behind reacts, and centre a card. A card never runs past the
//! window: it takes at most `CARD_MAX_HEIGHT` of the window's height and
//! `CARD_MAX_WIDTH` of its width, and a sheet whose content is taller scrolls
//! inside its own body.

use gpui::{div, prelude::*, relative, App, Div, Window};

use crate::model::Style;
use crate::util::to_gpui;

/// The largest share of the window's height a card may take.
pub const CARD_MAX_HEIGHT: f32 = 0.92;
/// The largest share of the window's width a card may take.
pub const CARD_MAX_WIDTH: f32 = 0.95;

/// A Qt modal `Popup` (Jump, Link, Nick, Inbox, Shortcuts): the card is centred
/// over the Material modal dim. Any press outside the card dismisses it.
pub fn popup_frame(
    style: &Style,
    card: Div,
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
        .child(clamped_card(card))
}

/// The About look: a full-window `overlay_veil_color` with a centred card.
/// Only a left click on the veil dismisses; right clicks and card clicks do not.
pub fn veil_frame(
    style: &Style,
    card: Div,
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
        .child(clamped_card(card))
}

/// The card, sized by the window and kept inside it. The card is the veil's own flex
/// child, not a wrapper: the veil is absolute and fills the window, so the card's
/// percentage limits resolve against the window's height and width. A wrapper of
/// auto size would leave them nothing to take a share of, and the card would run
/// past the window's edges.
///
/// Presses on the card stop here, so they do not reach the dismissing backdrop. The
/// listener sits on the card, not on a full-window shield, so presses outside the
/// card still reach the backdrop.
fn clamped_card(card: Div) -> Div {
    card.max_h(relative(CARD_MAX_HEIGHT))
        .max_w(relative(CARD_MAX_WIDTH))
        .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
}
