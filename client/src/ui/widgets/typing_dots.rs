use gpui::{prelude::*, div};

use crate::model::Style;
use crate::util::{scaled_px, to_gpui};

/// The sidebar's and member list's dots: sized for a 12px face, so each dot is 4px
/// apart by 3px.
const SIDEBAR_FACE_PX: f32 = 12.;

/// Three dots with one lit, advanced by the shell's typing pulse. The sidebar and
/// member-list size.
pub fn typing_dots(style: &Style, phase: u8) -> impl IntoElement {
    typing_dots_sized(style, phase, SIDEBAR_FACE_PX)
}

/// The same dots at a text size of `face_px` (Qt `TypingDots.pixelSize`): each dot
/// is a third of that, with a quarter of it between dots. The transcript's
/// footer dots use 16.
pub fn typing_dots_sized(style: &Style, phase: u8, face_px: f32) -> impl IntoElement {
    let dot = face_px / 3.;
    let gap = face_px / 4.;
    div()
        .flex()
        .gap(scaled_px(style, gap))
        .children((0..3).map(|i| {
            let active = (phase as usize) % 3 == i as usize;
            div()
                .size(scaled_px(style, dot))
                .rounded_full()
                .bg(to_gpui(if active {
                    style.accent_color()
                } else {
                    style.muted_color()
                }))
        }))
}
