//! The bordered text button used by sheets (About's `Check for Updates` and
//! OK). It takes keyboard focus through a `FocusHandle` the owner keeps, and
//! a click focuses it first, as Qt's `forceActiveFocus` does.

use gpui::{div, prelude::*, App, FocusHandle, MouseButton, Window};

use crate::model::Style;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// A 7px-radius button. `width` of `None` fits the label with 9px padding on
/// each side. The border is accent and the fill `hover` while focused or hovered.
pub fn text_button(
    style: &Style,
    label: &str,
    width: Option<f32>,
    height: f32,
    focus: &FocusHandle,
    focused: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let hover = to_gpui(style.hover_color());
    let click_focus = focus.clone();
    let button = div()
        .h(scaled_px(style, height))
        .px(scaled_px(style, 9.))
        .rounded(scaled_px(style, 7.))
        .border_1()
        .border_color(if focused {
            to_gpui(style.accent_color())
        } else {
            to_gpui(style.divider_color())
        })
        .bg(if focused {
            hover
        } else {
            gpui::transparent_black()
        })
        .hover(|s| s.bg(hover))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .track_focus(focus)
        .tab_stop(true)
        .font_family(FONT_FAMILY)
        .text_size(scaled_px(style, 11.))
        .text_color(to_gpui(style.ink_color()))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.focus(&click_focus);
            on_click(window, cx);
        })
        .child(label.to_string());
    match width {
        Some(width) => button.w(scaled_px(style, width)),
        None => button,
    }
}
