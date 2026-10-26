//! The bot mark beside a bot's nick (Qt `BotMark`): a short antenna over a
//! rounded box, drawn in the muted ink. It is 10 px square and never shrinks.

use gpui::{div, prelude::*, Div};

use crate::model::Style;
use crate::util::{scaled_px, to_gpui};

pub fn bot_mark(style: &Style) -> Div {
    let muted = to_gpui(style.muted_color());
    let stroke = scaled_px(style, 1.);
    div()
        .relative()
        .size(scaled_px(style, 10.))
        .flex_shrink_0()
        // The antenna stem, centred at the top.
        .child(
            div()
                .absolute()
                .top_0()
                .left(scaled_px(style, 4.5))
                .w(stroke)
                .h(scaled_px(style, 3.))
                .bg(muted),
        )
        // The rounded cap on top of the stem.
        .child(
            div()
                .absolute()
                .top_0()
                .left(scaled_px(style, 3.5))
                .w(scaled_px(style, 3.))
                .h(stroke)
                .rounded_full()
                .bg(muted),
        )
        // The box: 8 by 6 at the bottom, outlined in a 1 px stroke.
        .child(
            div()
                .absolute()
                .bottom_0()
                .left(scaled_px(style, 1.))
                .w(scaled_px(style, 8.))
                .h(scaled_px(style, 6.))
                .rounded(scaled_px(style, 1.))
                .border(stroke)
                .border_color(muted),
        )
}
