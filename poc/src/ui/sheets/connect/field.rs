//! A labelled text field on the Connection tab (`ConnectionField.qml`), and the
//! small `?` help mark that `FieldHelpMark.qml` draws beside a label. The input is
//! the shared `SingleLineInput`, so typing, selection and undo are the composer's.

use gpui::{div, prelude::*, MouseButton, SharedString};

use crate::model::connect::{ConnectField, Stop};
use crate::model::mix_colors;
use crate::ui::sheets::connect::ConnectView;
use crate::ui::widgets::tooltip::text_tooltip;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// One field: its label, a badge and help mark when it has them, the input, and
/// the help text under it while its mark holds focus. The field fills its parent.
pub fn text_field(view: &ConnectView<'_>, field: ConnectField) -> impl IntoElement {
    let style = view.style;
    let mut label_row = div()
        .h(scaled_px(style, 15.))
        .flex()
        .flex_row()
        .items_center()
        .gap(scaled_px(style, 6.))
        .child(
            div()
                .text_size(scaled_px(style, 10.))
                .text_color(to_gpui(style.muted_color()))
                .child(field.label()),
        );
    if let Some(badge) = field.badge() {
        label_row = label_row.child(badge_chip(view, badge));
    }
    let help = field.help();
    if let Some(text) = help {
        label_row = label_row.child(help_mark(view, Stop::FieldHelp(field), text));
    }

    let mut column = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(scaled_px(style, 4.))
        .child(label_row)
        .child(view.controls.input(field).clone());
    if let Some(text) = help {
        if view.is_focused(&Stop::FieldHelp(field)) {
            column = column.child(caption(view, text));
        }
    }
    column
}

/// The `PASS` chip beside a label (`ConnectionField.qml` `badge`).
fn badge_chip(view: &ConnectView<'_>, text: &'static str) -> impl IntoElement {
    let style = view.style;
    div()
        .h(scaled_px(style, 15.))
        .px(scaled_px(style, 5.))
        .flex()
        .items_center()
        .rounded(scaled_px(style, 4.))
        .bg(to_gpui(style.panel_color()))
        .border_1()
        .border_color(to_gpui(style.divider_color()))
        .text_size(scaled_px(style, 9.))
        .text_color(to_gpui(style.muted_color()))
        .font_family(FONT_FAMILY)
        .child(text)
}

/// A muted line of help text. Used under a field or a preference while its mark
/// has focus, where Qt shows a tooltip.
pub fn caption(view: &ConnectView<'_>, text: &str) -> impl IntoElement {
    let style = view.style;
    div()
        .w_full()
        .text_size(scaled_px(style, 10.))
        .text_color(to_gpui(style.muted_color()))
        .font_family(FONT_FAMILY)
        .child(text.to_string())
}

/// The 12px `?` circle. Hovering it shows `help` as a tooltip (Qt's
/// `ToolTip`). A click or Enter on it focuses it, and a focused mark shows its
/// help as a `caption` under the field, for keyboard users. Its stop is `stop`, a
/// `FieldHelp` or `PreferenceHelp`, and its element id comes from that stop.
pub fn help_mark(view: &ConnectView<'_>, stop: Stop, help: &str) -> impl IntoElement {
    let style = view.style;
    let focused = view.is_focused(&stop);
    let resting = mix_colors(
        style.page_color(),
        style.ink_color(),
        if style.dark_mode() { 0.64 } else { 0.47 },
    );
    let accent = to_gpui(style.accent_color());
    let mut mark = div()
        .id(SharedString::from(format!("help-mark-{stop:?}")))
        .w(scaled_px(style, 12.))
        .h(scaled_px(style, 12.))
        .flex_shrink_0()
        .rounded(scaled_px(style, 6.))
        .border_1()
        .border_color(if focused {
            accent
        } else {
            to_gpui(style.divider_color())
        })
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .font_family(FONT_FAMILY)
        .text_size(scaled_px(style, 9.))
        .text_color(if focused { accent } else { to_gpui(resting) })
        .on_mouse_down(MouseButton::Left, {
            let click = view.click(stop.clone());
            move |_, window, cx| click(window, cx)
        })
        .tooltip(text_tooltip(vec![help.to_owned()]))
        .child("?");
    if let Some(handle) = view.handle(&stop) {
        mark = mark.track_focus(handle);
    }
    mark
}
