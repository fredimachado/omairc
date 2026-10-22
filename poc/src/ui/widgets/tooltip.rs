//! The hover tooltip (Qt's `ToolTip`): a raised card with a hairline edge and one
//! line of 10px ink text per entry. The card is capped at `TOOLTIP_MAX_WIDTH`, so a
//! long entry wraps inside it. A row or a help mark attaches it with
//! `.tooltip(text_tooltip(lines))`.

use gpui::{div, prelude::*, AnyView, App, Context, Render, Window};

use crate::util::{scaled_px, style, to_gpui, FONT_FAMILY};

/// The card's widest size at text scale 1. Its lines wrap at this width rather than
/// widen the card, so a help note stays beside the field it explains.
const TOOLTIP_MAX_WIDTH: f32 = 320.;

/// The tooltip's view: its lines, top to bottom.
pub struct TextTooltip {
    pub lines: Vec<String>,
}

impl Render for TextTooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_style = style(cx);
        div()
            .max_w(scaled_px(&app_style, TOOLTIP_MAX_WIDTH))
            .px(scaled_px(&app_style, 8.))
            .py(scaled_px(&app_style, 6.))
            .rounded(scaled_px(&app_style, 6.))
            .border_1()
            .border_color(to_gpui(app_style.divider_color()))
            .bg(to_gpui(app_style.raised_color()))
            .flex()
            .flex_col()
            .text_size(scaled_px(&app_style, 10.))
            .text_color(to_gpui(app_style.ink_color()))
            .font_family(FONT_FAMILY)
            .children(self.lines.iter().map(|line| div().child(line.clone())))
    }
}

/// The builder a `.tooltip` call takes: each hover makes a fresh `TextTooltip` with
/// `lines`, one per entry in order.
pub fn text_tooltip(lines: Vec<String>) -> impl Fn(&mut Window, &mut App) -> AnyView {
    move |_window, cx| {
        cx.new(|_| TextTooltip {
            lines: lines.clone(),
        })
        .into()
    }
}
