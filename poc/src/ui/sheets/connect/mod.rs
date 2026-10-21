//! The Connect sheet: the veil and the tabbed card, a port of `ConnectionSheet.qml`.
//! The card is the keyboard's home while Connect shows. Its key context owns Tab,
//! Enter, Space and the rail keys, and the fields under it keep their text keys.
//!
//! The model (`model/connect.rs`) holds the state and the focus walk. The shell
//! (`ui/shell/connect.rs`) acts on it. This module only draws, and reports clicks
//! as a `Stop` through `ConnectView::on_stop`.

pub mod controls;
mod field;
mod footer;
mod form;
mod rail;
mod tabs;
mod toggles;

use gpui::{actions, div, prelude::*, relative, AnyElement, App, FocusHandle, KeyBinding};

use crate::model::connect::{ConnectState, ConnectTab, Stop};
use crate::model::prefs::Preferences;
use crate::model::Style;
use crate::ui::sheets::connect::controls::ConnectControls;
use crate::ui::sheets::frame::veil_frame;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

pub use form::form_row_of;

/// The key context of the card. Its bindings reach every control on the card.
pub const CONTEXT: &str = "ConnectSheet";

actions!(
    connect_sheet,
    [
        ConnectNextStop,
        ConnectPrevStop,
        ConnectEnter,
        ConnectSpace,
        ConnectArrowLeft,
        ConnectArrowRight,
        ConnectRailUp,
        ConnectRailDown,
        ConnectRailHome,
        ConnectRailEnd,
    ]
);

/// The card's keys. Tab and Shift+Tab walk every stop, Enter walks or activates,
/// and Space activates. Left and Right step the tabs. Up, Down, Home and End move
/// through the rail. Each action passes on a key it does not use, so a text field
/// still types a space and an arrow still moves its caret.
pub fn bind_connect_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", ConnectNextStop, Some(CONTEXT)),
        KeyBinding::new("shift-tab", ConnectPrevStop, Some(CONTEXT)),
        KeyBinding::new("enter", ConnectEnter, Some(CONTEXT)),
        KeyBinding::new("space", ConnectSpace, Some(CONTEXT)),
        KeyBinding::new("left", ConnectArrowLeft, Some(CONTEXT)),
        KeyBinding::new("right", ConnectArrowRight, Some(CONTEXT)),
        KeyBinding::new("up", ConnectRailUp, Some(CONTEXT)),
        KeyBinding::new("down", ConnectRailDown, Some(CONTEXT)),
        KeyBinding::new("home", ConnectRailHome, Some(CONTEXT)),
        KeyBinding::new("end", ConnectRailEnd, Some(CONTEXT)),
    ]);
}

/// Everything the card draws in one frame, read from the shell.
pub struct ConnectView<'a> {
    pub state: &'a ConnectState,
    /// The networks that hold a conversation (Remove needs another one to show).
    pub populated: &'a [String],
    /// Whether the selected network's session is live or reconnecting, so Disconnect
    /// shows. The controller answers it (`Controller::can_disconnect`).
    pub disconnectable: bool,
    pub prefs: &'a Preferences,
    pub controls: &'a ConnectControls,
    /// The stop holding focus, if any. Captions and focus rings read it.
    pub focused: Option<&'a Stop>,
    /// The card's own handle, so keys reach the sheet when no stop has focus.
    pub card_focus: &'a FocusHandle,
    pub style: &'a Style,
    /// Called with the stop a click landed on.
    pub on_stop: Handler<Stop>,
}

impl<'a> ConnectView<'a> {
    /// Whether `stop` holds focus this frame.
    pub fn is_focused(&self, stop: &Stop) -> bool {
        self.focused == Some(stop)
    }

    /// The handle of `stop` for `track_focus`, if the shell has made one.
    pub fn handle(&self, stop: &Stop) -> Option<&'a FocusHandle> {
        self.controls.handle(stop)
    }

    /// A mouse-down handler that reports `stop` to the shell.
    pub fn click(&self, stop: Stop) -> impl Fn(&mut gpui::Window, &mut App) + 'static {
        let on_stop = self.on_stop.clone();
        move |window, cx| on_stop(stop.clone(), window, cx)
    }
}

/// The veil and the card. The veil swallows presses and the wheel, so nothing
/// behind the sheet reacts. The card holds focus, and `ConnectView::focused`
/// says which stop has it.
pub fn render_connect(view: &ConnectView<'_>) -> AnyElement {
    let style = view.style;
    let mut card = div()
        .key_context(CONTEXT)
        .track_focus(view.card_focus)
        .w(scaled_px(style, 860.))
        .max_w(relative(0.95))
        .max_h(relative(0.92))
        .rounded(scaled_px(style, 10.))
        .bg(to_gpui(style.raised_color()))
        .border_1()
        .border_color(to_gpui(style.divider_color()))
        .font_family(FONT_FAMILY)
        .text_color(to_gpui(style.ink_color()))
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(tabs::tab_strip(view))
        .child(divider(style));
    card = match view.state.tab() {
        ConnectTab::Connection => card.child(connection_body(view)),
        ConnectTab::Preferences => card.child(toggles::preferences_panel(view)),
    };
    veil_frame(style, card, |_, _| {}).into_any_element()
}

/// The Connection tab: the rail and the form side by side, then the footer.
fn connection_body(view: &ConnectView<'_>) -> impl IntoElement {
    let style = view.style;
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_row()
                .gap(scaled_px(style, 16.))
                .px(scaled_px(style, 18.))
                .pt(scaled_px(style, 14.))
                .pb(scaled_px(style, 12.))
                .child(rail::render_rail(view))
                .child(form::render_form(view)),
        )
        .child(divider(style))
        .child(footer::render_footer(view))
}

/// A 1px rule in the divider colour.
fn divider(style: &Style) -> impl IntoElement {
    div()
        .h(scaled_px(style, 1.))
        .w_full()
        .flex_shrink_0()
        .bg(to_gpui(style.divider_color()))
}
