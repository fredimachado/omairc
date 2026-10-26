//! The floating `↓` chip over the transcript (`UnseenJumpButton`). It shows
//! while an unseen jump is armed: a line arrived while the reader was scrolled up
//! or a "New messages" mark is open above the end. A press runs the jump. The
//! shell draws nothing here, so the chip is part of the transcript view.

use std::time::{Duration, Instant};

use gpui::{div, prelude::*, px, App, IntoElement, MouseButton, MouseDownEvent, Window};

use crate::model::{mix_colors, Style};
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// One half of the bounce (`UnseenJumpButton` `NumberAnimation` duration).
const BOUNCE_HALF_MS: u64 = 700;
/// Five up/down cycles before the arrow rests.
const BOUNCE_LOOPS: u32 = 5;
const BOUNCE_AMPLITUDE: f32 = 3.;

/// The chip's diameter in logical pixels (`UnseenJumpButton`).
const CHIP_SIZE: f32 = 36.;
/// The chip's inset from the transcript's bottom-right corner.
const CHIP_INSET: f32 = 16.;

/// Micro-feedback when a line arrives while the reader is scrolled up (`bounceBurst`).
#[derive(Clone, Debug, Default)]
pub struct UnseenBounce {
    started: Option<Instant>,
}

impl UnseenBounce {
    pub fn restart(&mut self) {
        self.started = Some(Instant::now());
    }

    pub fn stop(&mut self) {
        self.started = None;
    }

    pub fn running(&self) -> bool {
        self.started.is_some_and(|start| !finished(start.elapsed()))
    }

    pub fn offset_y(&self, scale: f32) -> f32 {
        self.started
            .map(|start| bounce_offset(scale, start.elapsed()))
            .unwrap_or(0.)
    }

    /// When the chip should repaint next, if a bounce is running.
    pub fn next_frame_in(&self) -> Option<Duration> {
        let start = self.started?;
        let elapsed = start.elapsed();
        if finished(elapsed) {
            return None;
        }
        Some(Duration::from_millis(16))
    }
}

fn finished(elapsed: Duration) -> bool {
    elapsed.as_millis() >= (BOUNCE_HALF_MS * 2 * BOUNCE_LOOPS as u64) as u128
}

fn ease_in_out_sine(t: f32) -> f32 {
    (1. - (std::f32::consts::PI * t).cos()) / 2.
}

/// Vertical offset in logical pixels for the arrow (`bounceOffset`).
pub fn bounce_offset(scale: f32, elapsed: Duration) -> f32 {
    if finished(elapsed) {
        return 0.;
    }
    let loop_ms = BOUNCE_HALF_MS * 2;
    let ms = (elapsed.as_millis() % loop_ms as u128) as u64;
    let amp = scale * BOUNCE_AMPLITUDE;
    if ms < BOUNCE_HALF_MS {
        amp * ease_in_out_sine(ms as f32 / BOUNCE_HALF_MS as f32)
    } else {
        let t = (ms - BOUNCE_HALF_MS) as f32 / BOUNCE_HALF_MS as f32;
        amp * (1. - ease_in_out_sine(t))
    }
}

/// The chip. `on_press` runs the jump. The press stops at the chip, so it does not
/// start a transcript selection underneath it.
pub fn render_unseen_jump(
    style: &Style,
    bounce_y: f32,
    on_press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let raised = style.raised_color();
    let hover = mix_colors(
        raised,
        style.accent_color(),
        if style.dark_mode() { 0.35 } else { 0.22 },
    );
    div()
        .id("unseen-jump")
        .absolute()
        .right(scaled_px(style, CHIP_INSET))
        .bottom(scaled_px(style, CHIP_INSET))
        .size(scaled_px(style, CHIP_SIZE))
        .rounded_full()
        .border_1()
        .border_color(to_gpui(style.divider_color()))
        .bg(to_gpui(raised))
        .hover(move |chip| chip.bg(to_gpui(hover)))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |ev, window, cx| {
            cx.stop_propagation();
            on_press(ev, window, cx);
        })
        .child(
            div()
                .text_size(scaled_px(style, 16.))
                .text_color(to_gpui(style.ink_color()))
                .font_family(FONT_FAMILY)
                .mt(px(bounce_y))
                .child("\u{2193}"),
        )
        // The accent dot marks that the chip is armed (`unseenJumpDot`).
        .child(
            div()
                .absolute()
                .top(scaled_px(style, 2.))
                .right(scaled_px(style, 2.))
                .size(scaled_px(style, 8.))
                .rounded_full()
                .bg(to_gpui(style.accent_color())),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounce_runs_then_returns_to_zero() {
        let scale = 1.;
        let half = Duration::from_millis(BOUNCE_HALF_MS);
        assert!(bounce_offset(scale, Duration::ZERO) < 0.01);
        assert!(bounce_offset(scale, half) > scale);
        let done = Duration::from_millis(BOUNCE_HALF_MS * 2 * BOUNCE_LOOPS as u64 + 1);
        assert_eq!(bounce_offset(scale, done), 0.);
    }

    #[test]
    fn restart_arms_the_burst() {
        let mut bounce = UnseenBounce::default();
        assert!(!bounce.running());
        bounce.restart();
        assert!(bounce.running());
        bounce.stop();
        assert!(!bounce.running());
    }
}
