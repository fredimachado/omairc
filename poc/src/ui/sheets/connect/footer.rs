//! The footer of the Connection tab (`ConnectionSheet.qml`'s `connectionFooter`):
//! the selected network's name, then Remove, Discard and Apply. Apply is accent
//! when the draft is complete, and muted while `problem` says why it is not.
//! Remove is offered only when `can_remove` allows it, and its first press arms it.

use gpui::{div, prelude::*, transparent_black, white, Div, FontWeight, MouseButton};

use crate::model::connect::{remove_label, Stop, APPLY_LABEL, DISCARD_LABEL};
use crate::ui::sheets::connect::ConnectView;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The footer, 52px tall. The name is cut when the window is narrow.
pub fn render_footer(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    let name = view
        .state
        .rows()
        .into_iter()
        .find(|row| row.selected)
        .map(|row| row.label)
        .unwrap_or_default();
    let mut actions = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(scaled_px(style, 8.))
        .flex_shrink_0();
    if view.state.can_remove(view.populated) {
        actions = actions.child(remove_button(view, &name));
    }
    actions = actions
        .child(discard_button(view))
        .child(apply_button(view));

    div()
        .h(scaled_px(style, 52.))
        .w_full()
        .flex_shrink_0()
        .flex()
        .flex_row()
        .items_center()
        .gap(scaled_px(style, 8.))
        .px(scaled_px(style, 18.))
        .child(
            div()
                .w(scaled_px(style, 6.))
                .h(scaled_px(style, 6.))
                .flex_shrink_0()
                .rounded(scaled_px(style, 3.))
                .bg(to_gpui(style.accent_color())),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_family(FONT_FAMILY)
                .text_size(scaled_px(style, 11.))
                .text_color(to_gpui(style.muted_color()))
                .child(name),
        )
        .child(actions)
}

/// A 30px button with a 7px radius, its label centred. Its handle and click come
/// from `stop`. The caller sets the fill, the border and the text.
fn base(view: &ConnectView<'_>, stop: Stop, width: f32) -> Div {
    let style = view.style;
    let mut button = div()
        .w(scaled_px(style, width))
        .h(scaled_px(style, 30.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(scaled_px(style, 7.))
        .border_1()
        .cursor_pointer()
        .font_family(FONT_FAMILY)
        .text_size(scaled_px(style, 11.))
        .on_mouse_down(MouseButton::Left, {
            let click = view.click(stop.clone());
            move |_, window, cx| click(window, cx)
        });
    if let Some(handle) = view.handle(&stop) {
        button = button.track_focus(handle);
    }
    button
}

/// The label inside a button: cut with an ellipsis, so a long name stays in its box.
fn label(text: impl Into<gpui::SharedString>) -> impl IntoElement {
    div()
        .w_full()
        .px(gpui::px(8.))
        .truncate()
        .child(text.into())
}

/// `Remove`, or `Remove <name>?` once armed. Armed, the button widens to fit.
fn remove_button(view: &ConnectView<'_>, name: &str) -> impl IntoElement {
    let style = view.style;
    let armed = view.state.remove_armed();
    let focused = view.is_focused(&Stop::Remove);
    let hover = to_gpui(style.hover_color());
    base(view, Stop::Remove, if armed { 148. } else { 88. })
        .bg(if focused { hover } else { transparent_black() })
        .hover(move |s| s.bg(hover))
        .border_color(if focused {
            to_gpui(style.accent_color())
        } else {
            to_gpui(style.divider_color())
        })
        .text_color(to_gpui(style.accent_color()))
        .child(label(remove_label(armed, name)))
}

/// `Discard`, which drops the draft's edits back to the saved network.
fn discard_button(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    let focused = view.is_focused(&Stop::Discard);
    let hover = to_gpui(style.hover_color());
    base(view, Stop::Discard, 88.)
        .bg(if focused { hover } else { transparent_black() })
        .hover(move |s| s.bg(hover))
        .border_color(if focused {
            to_gpui(style.accent_color())
        } else {
            to_gpui(style.divider_color())
        })
        .text_color(to_gpui(style.muted_color()))
        .child(label(DISCARD_LABEL))
}

/// `Apply`. Filled in accent when the draft can be applied. Otherwise it is raised
/// and muted, and a press does nothing. The problem line above says why.
fn apply_button(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    let ready = view.state.problem().is_none();
    let focused = view.is_focused(&Stop::Apply);
    let ink = to_gpui(style.ink_color());
    let accent = to_gpui(style.accent_color());
    let border = match (focused, ready) {
        (false, _) => transparent_black(),
        (true, true) => ink,
        (true, false) => accent,
    };
    base(view, Stop::Apply, 88.)
        .bg(if ready {
            accent
        } else {
            to_gpui(style.raised_color())
        })
        .border_color(border)
        .font_weight(FontWeight::BOLD)
        .text_color(if ready {
            white()
        } else {
            to_gpui(style.muted_color())
        })
        .child(label(APPLY_LABEL))
}
