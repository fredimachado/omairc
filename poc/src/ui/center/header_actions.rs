//! Port of `src/qml/ConversationHeaderActions.qml`: the LEAVE, JOIN and CLOSE buttons at
//! the right of the conversation header. A joined channel offers LEAVE. A channel you have
//! left offers JOIN. A selection that can close (a direct message, or a channel you have
//! left) offers CLOSE.
//!
//! The buttons draw and ask. A click calls the handler the shell gives, and the shell calls a
//! controller intent (`leave_selected_channel`, `join_selected_channel`, `close_direct`). The
//! file decides which buttons show and nothing else.

use gpui::{div, prelude::*, AnyElement, FontWeight, MouseButton};

use crate::model::Style;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// Which buttons the header offers for the selection. Each flag is the QML property of the
/// same name (`showLeave`, `showJoin`, `showClose`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderOffers {
    pub leave: bool,
    pub join: bool,
    pub close: bool,
}

impl HeaderOffers {
    /// The offers for a selection that is `channel`, with `joined` set for a channel the
    /// user is in, and `can_close` set when the controller says the selection closes.
    pub fn for_selection(channel: bool, joined: bool, can_close: bool) -> Self {
        Self {
            leave: channel && joined,
            join: channel && !joined,
            close: can_close,
        }
    }

    /// Whether any button shows. The row is hidden when none does.
    pub fn any(self) -> bool {
        self.leave || self.join || self.close
    }
}

/// The header's buttons: what they offer and what a click asks the shell to do.
pub struct HeaderActions {
    pub offers: HeaderOffers,
    pub on_leave: Handler<()>,
    pub on_join: Handler<()>,
    pub on_close: Handler<()>,
}

/// The offered buttons in a row, or `None` when the selection offers none.
pub fn render_header_actions(actions: HeaderActions, style: &Style) -> Option<AnyElement> {
    let HeaderActions {
        offers,
        on_leave,
        on_join,
        on_close,
    } = actions;
    if !offers.any() {
        return None;
    }
    let row = div()
        .flex()
        .items_center()
        .gap(scaled_px(style, 8.))
        .when(offers.leave, |this| {
            this.child(action_button(style, "LEAVE", 58., on_leave))
        })
        .when(offers.join, |this| {
            this.child(action_button(style, "JOIN", 58., on_join))
        })
        .when(offers.close, |this| {
            this.child(action_button(style, "CLOSE", 64., on_close))
        });
    Some(row.into_any_element())
}

/// One 30-tall button. It is transparent until hovered, when it takes the raised fill and
/// the divider border, as the QML button does.
fn action_button(
    style: &Style,
    label: &str,
    width: f32,
    on_click: Handler<()>,
) -> impl IntoElement {
    let raised = to_gpui(style.raised_color());
    let divider = to_gpui(style.divider_color());
    div()
        .w(scaled_px(style, width))
        .h(scaled_px(style, 30.))
        .rounded(scaled_px(style, 7.))
        .border_1()
        .border_color(gpui::transparent_black())
        .hover(move |this| this.bg(raised).border_color(divider))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .font_family(FONT_FAMILY)
        .font_weight(FontWeight::BOLD)
        .text_size(scaled_px(style, 9.))
        .text_color(to_gpui(style.ink_color()))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            on_click((), window, cx);
        })
        .child(label.to_string())
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use chrono::{TimeZone, Utc};

    use super::HeaderOffers;
    use crate::controller::controller::Controller;
    use crate::session::clock::FakeClock;

    const LEAVE_ONLY: HeaderOffers = HeaderOffers {
        leave: true,
        join: false,
        close: false,
    };
    const JOIN_AND_CLOSE: HeaderOffers = HeaderOffers {
        leave: false,
        join: true,
        close: true,
    };
    const CLOSE_ONLY: HeaderOffers = HeaderOffers {
        leave: false,
        join: false,
        close: true,
    };

    #[test]
    fn a_joined_channel_offers_leave_only() {
        assert_eq!(HeaderOffers::for_selection(true, true, false), LEAVE_ONLY);
    }

    #[test]
    fn a_left_channel_offers_join_and_close() {
        assert_eq!(
            HeaderOffers::for_selection(true, false, true),
            JOIN_AND_CLOSE
        );
    }

    #[test]
    fn a_direct_message_offers_close_only() {
        assert_eq!(HeaderOffers::for_selection(false, false, true), CLOSE_ONLY);
    }

    #[test]
    fn a_joined_channel_that_cannot_close_offers_leave_and_nothing_else() {
        // `can_close` is false for a joined channel, so CLOSE stays off there.
        assert!(!HeaderOffers::for_selection(true, true, false).close);
        assert!(!HeaderOffers::for_selection(false, false, false).any());
    }

    fn demo() -> Controller {
        let clock = Rc::new(FakeClock::new(
            Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap(),
        ));
        Controller::new_demo_with(clock).expect("the demo starts")
    }

    fn offers_of(controller: &Controller) -> HeaderOffers {
        HeaderOffers::for_selection(
            controller.is_channel(),
            controller.channel_joined(),
            controller.can_close_selection(),
        )
    }

    #[test]
    fn a_part_turns_leave_into_join_and_close_on_the_same_channel() {
        let mut controller = demo();
        controller.select_conversation("omarchy", "#omarchy");
        assert_eq!(offers_of(&controller), LEAVE_ONLY);

        assert!(controller.leave_selected_channel());
        assert_eq!(offers_of(&controller), JOIN_AND_CLOSE);

        assert!(controller.join_selected_channel());
        assert_eq!(offers_of(&controller), LEAVE_ONLY);
    }

    #[test]
    fn a_direct_message_offers_close_from_the_controller_reads() {
        let mut controller = demo();
        controller.select_conversation("omarchy", "anna");
        assert_eq!(offers_of(&controller), CLOSE_ONLY);
    }
}
