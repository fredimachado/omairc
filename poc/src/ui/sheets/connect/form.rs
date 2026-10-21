//! The Connection form (`ConnectionSheet.qml`'s `connectionFormArea`): the profile
//! fields in Qt's order, the switches, and the lines that say why Apply is off.
//!
//! The form scrolls in a list whose children are fixed rows, numbered by
//! `form_row_of`. Each stop maps to one row, so `Shell::reveal_connect` can bring a
//! focused field into view.

use gpui::{div, prelude::*};

use crate::model::connect::{
    ConnectField, Stop, CONNECT_ON_STARTUP_LABEL, SWITCH_REFUSED_TEXT, TITLE,
};
use crate::ui::sheets::connect::field::text_field;
use crate::ui::sheets::connect::toggles::{switch, tls_switch};
use crate::ui::sheets::connect::ConnectView;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The form column: the title, then the scrolling rows.
pub fn render_form(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    let rows = div()
        .id("connect-form")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(&view.controls.form_scroll)
        .flex()
        .flex_col()
        .gap(scaled_px(style, 10.))
        .child(text_field(view, ConnectField::Name))
        .child(host_row(view))
        .child(nick_row(view))
        .child(text_field(view, ConnectField::RealName))
        .child(text_field(view, ConnectField::Autojoin))
        .child(startup_row(view))
        .child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(scaled_px(style, 10.))
                .child(
                    div()
                        .h(scaled_px(style, 1.))
                        .w_full()
                        .bg(to_gpui(style.divider_color())),
                )
                .child(text_field(view, ConnectField::ServerPassword)),
        )
        .child(text_field(view, ConnectField::NickServPassword))
        .child(notices(view));

    div()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .gap(scaled_px(style, 10.))
        .child(
            div()
                .text_size(scaled_px(style, 15.))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(to_gpui(style.ink_color()))
                .font_family(FONT_FAMILY)
                .child(TITLE),
        )
        .child(rows)
}

/// The form row a stop lives in, for scrolling it into view. Stops with no row
/// (the rail, the footer, the tabs) give `None`.
pub fn form_row_of(stop: &Stop) -> Option<usize> {
    match stop {
        Stop::Field(field) | Stop::FieldHelp(field) => match field {
            ConnectField::Name => Some(0),
            ConnectField::Host | ConnectField::Port => Some(1),
            ConnectField::Nick | ConnectField::Username => Some(2),
            ConnectField::RealName => Some(3),
            ConnectField::Autojoin => Some(4),
            ConnectField::ServerPassword => Some(6),
            ConnectField::NickServPassword => Some(7),
        },
        Stop::Tls => Some(1),
        Stop::ConnectOnStartup => Some(5),
        _ => None,
    }
}

/// Host, Port and TLS on one line. The host takes the rest of the width.
fn host_row(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    div()
        .w_full()
        .flex()
        .flex_row()
        .items_end()
        .gap(scaled_px(style, 12.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(text_field(view, ConnectField::Host)),
        )
        .child(
            div()
                .w(scaled_px(style, 104.))
                .flex_shrink_0()
                .child(text_field(view, ConnectField::Port)),
        )
        .child(tls_switch(view))
}

/// Nick and Username side by side.
fn nick_row(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    div()
        .w_full()
        .flex()
        .flex_row()
        .gap(scaled_px(style, 12.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(text_field(view, ConnectField::Nick)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(text_field(view, ConnectField::Username)),
        )
}

/// The connect-on-startup switch and its label.
fn startup_row(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    div()
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(scaled_px(style, 10.))
        .child(switch(
            view,
            Stop::ConnectOnStartup,
            view.state.connect_on_startup(),
        ))
        .child(
            div()
                .text_size(scaled_px(style, 11.))
                .text_color(to_gpui(style.ink_color()))
                .font_family(FONT_FAMILY)
                .child(CONNECT_ON_STARTUP_LABEL),
        )
}

/// The lines under the form, in Qt's order: where the password is kept, why Apply is off, why a
/// switch was refused, and what the last Apply saved. The credential line is muted, and the rest
/// are accent-coloured, as Qt's are.
fn notices(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    let accent = to_gpui(style.accent_color());
    let mut column = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(scaled_px(style, 4.))
        .text_size(scaled_px(style, 11.))
        .font_family(FONT_FAMILY);
    let credential = view.state.credential_status();
    if !credential.is_empty() {
        column = column.child(
            div()
                .text_size(scaled_px(style, 10.))
                .text_color(to_gpui(style.muted_color()))
                .child(credential),
        );
    }
    if let Some(problem) = view.state.problem() {
        column = column.child(div().text_color(accent).child(problem.text()));
    }
    if view.state.switch_refused() {
        column = column.child(div().text_color(accent).child(SWITCH_REFUSED_TEXT));
    }
    let persistence = view.state.persistence_status();
    if !persistence.is_empty() {
        column = column.child(div().text_color(accent).child(persistence));
    }
    column
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::connect::{demo_seeds, ConnectState};

    /// Every form stop has a row, and the rows never run backwards along the Tab
    /// order. Scrolling a focused field into view depends on both.
    #[test]
    fn form_rows_follow_the_tab_order() {
        let populated = vec!["omarchy".to_string(), "oftc".to_string()];
        let state = ConnectState::seed(&demo_seeds());
        let rows: Vec<usize> = state
            .stops(&populated, false)
            .iter()
            .filter(|stop| !matches!(stop, Stop::Tab(_)))
            .filter_map(|stop| match stop {
                Stop::Field(_) | Stop::FieldHelp(_) | Stop::Tls | Stop::ConnectOnStartup => {
                    Some(form_row_of(stop).expect("every form stop has a row"))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            rows.len(),
            13,
            "nine fields, two help marks, TLS and the startup switch"
        );
        assert!(rows.windows(2).all(|pair| pair[0] <= pair[1]), "{rows:?}");
        assert_eq!(form_row_of(&Stop::Network("omarchy".into())), None);
        assert_eq!(form_row_of(&Stop::Apply), None);
    }
}
