use gpui::{prelude::*, div};

use crate::model::Style;
use crate::util::{scaled_px, to_gpui};

/// Three dots with one lit, advanced by the shell's typing pulse.
pub fn typing_dots(style: &Style, phase: u8) -> impl IntoElement {
    div()
        .flex()
        .gap(scaled_px(style, 3.))
        .children((0..3).map(|i| {
            let active = (phase as usize) % 3 == i as usize;
            div()
                .size(scaled_px(style, 4.))
                .rounded_full()
                .bg(to_gpui(if active {
                    style.accent_color()
                } else {
                    style.muted_color()
                }))
        }))
}
