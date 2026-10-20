//! The tab strip across the top of the card (`ConnectionTabButton.qml`). A tab is
//! a stop: a click shows it, Enter and Space show it, and Left and Right step
//! between the tabs from the keyboard (`ConnectArrowLeft` and `ConnectArrowRight`).

use gpui::{div, prelude::*, FontWeight, MouseButton};

use crate::model::connect::{ConnectTab, Stop};
use crate::model::Color;
use crate::ui::sheets::connect::ConnectView;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The strip, 52px tall, with the tabs bottom-aligned from a 10px left margin.
pub fn tab_strip(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    let mut strip = div()
        .h(scaled_px(style, 52.))
        .w_full()
        .flex_shrink_0()
        .flex()
        .flex_row()
        .items_end()
        .pl(scaled_px(style, 10.))
        .gap(scaled_px(style, 2.));
    for tab in ConnectTab::ALL {
        strip = strip.child(tab_button(view, tab));
    }
    strip
}

/// One tab: its glyph and label in ink when it is showing, muted otherwise. The
/// showing tab has a 2px accent rule along its bottom edge.
fn tab_button(view: &ConnectView<'_>, tab: ConnectTab) -> impl IntoElement {
    let style = view.style;
    let stop = Stop::Tab(tab);
    let current = view.state.tab() == tab;
    let focused = view.is_focused(&stop);
    let tone = if current {
        style.ink_color()
    } else {
        style.muted_color()
    };
    let hover = to_gpui(style.hover_color());

    let mut button = div()
        .relative()
        .h(scaled_px(style, 52.))
        .px(scaled_px(style, 13.))
        .flex()
        .flex_row()
        .items_center()
        .gap(scaled_px(style, 7.))
        .font_family(FONT_FAMILY)
        .text_size(scaled_px(style, 12.))
        .text_color(to_gpui(tone))
        .cursor_pointer()
        .bg(if focused {
            hover
        } else {
            gpui::transparent_black()
        })
        .hover(move |s| s.bg(hover))
        .on_mouse_down(MouseButton::Left, {
            let click = view.click(stop.clone());
            move |_, window, cx| click(window, cx)
        })
        .child(glyph(view, tab, tone));
    if let Some(handle) = view.handle(&stop) {
        button = button.track_focus(handle);
    }
    let button = button.child(div().child(tab.label()));
    if current {
        button.child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(scaled_px(style, 2.))
                .bg(to_gpui(style.accent_color())),
        )
    } else {
        button
    }
}

/// The glyph before the label: `#` for Connection, and a small drawn sliders
/// mark for Preferences, which has no text glyph (Qt draws it with rectangles).
fn glyph(view: &ConnectView<'_>, tab: ConnectTab, tone: Color) -> impl IntoElement {
    let style = view.style;
    match tab.glyph() {
        Some(text) => div()
            .font_weight(FontWeight::BOLD)
            .text_size(scaled_px(style, 12.))
            .text_color(to_gpui(tone))
            .child(text)
            .into_any_element(),
        None => {
            let ink = to_gpui(tone);
            let rule = |top: f32| {
                div()
                    .absolute()
                    .left_0()
                    .top(scaled_px(style, top))
                    .w(scaled_px(style, 12.))
                    .h(scaled_px(style, 1.))
                    .bg(ink)
            };
            let knob = |left: f32, top: f32| {
                div()
                    .absolute()
                    .left(scaled_px(style, left))
                    .top(scaled_px(style, top))
                    .w(scaled_px(style, 5.))
                    .h(scaled_px(style, 5.))
                    .rounded(scaled_px(style, 1.))
                    .bg(ink)
            };
            div()
                .relative()
                .w(scaled_px(style, 12.))
                .h(scaled_px(style, 15.))
                .child(rule(4.))
                .child(knob(7., 2.))
                .child(rule(10.))
                .child(knob(1., 8.))
                .into_any_element()
        }
    }
}
