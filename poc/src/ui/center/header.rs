use std::ops::Range;

use gpui::{div, prelude::*, InteractiveText, MouseButton, SharedString, StyledText};

use crate::model::format::{FormattedText, TextHit};
use crate::model::Style;
use crate::ui::format_ui::{gpui_text_style, irc_runs_to_gpui};
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The header's second line.
pub enum Subtitle<'a> {
    /// Text with nothing to click: the network's last error, or `Connected`.
    Plain(&'a str),
    /// A conversation's topic. It is formatted, its URLs and channel names are
    /// clickable, and `on_hit` gets the hit that was clicked.
    Topic {
        text: &'a FormattedText,
        hits: &'a [(Range<usize>, TextHit)],
        on_hit: Handler<TextHit>,
    },
}

/// What the header shows. A conversation passes its topic as the subtitle and
/// its people count; Status passes `Connected` and no count.
pub struct HeaderView<'a> {
    pub title: &'a str,
    pub subtitle: Subtitle<'a>,
    /// `Some` shows the `N PEOPLE` pill, which only channels have.
    pub people: Option<usize>,
}

/// The title, the subtitle line, and the `N PEOPLE` pill when there is a count.
pub fn render_header(
    view: HeaderView<'_>,
    style: &Style,
    on_toggle_members: Handler<()>,
) -> impl IntoElement {
    let HeaderView {
        title,
        subtitle,
        people,
    } = view;

    div()
        .px(scaled_px(style, 16.))
        .py(scaled_px(style, 10.))
        .border_b_1()
        .border_color(to_gpui(style.divider_color()))
        .flex()
        .flex_col()
        .gap(scaled_px(style, 4.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(scaled_px(style, 8.))
                .child(
                    div()
                        .flex_grow()
                        .text_size(scaled_px(style, 14.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(to_gpui(style.ink_color()))
                        .font_family(FONT_FAMILY)
                        .child(title.to_string()),
                )
                .when_some(people, |this, people| {
                    this.child(
                        div()
                            .px(scaled_px(style, 8.))
                            .py(scaled_px(style, 3.))
                            .rounded_full()
                            .border_1()
                            .border_color(to_gpui(style.divider_color()))
                            .cursor_pointer()
                            .text_size(scaled_px(style, 10.))
                            .text_color(to_gpui(style.muted_color()))
                            .font_family(FONT_FAMILY)
                            .child(format!("{} PEOPLE", people))
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                on_toggle_members((), window, cx);
                            }),
                    )
                }),
        )
        .child(subtitle_line(style, subtitle))
}

/// The subtitle row. A topic is drawn formatted, and each of its hits is a
/// clickable range of the text.
fn subtitle_line(style: &Style, subtitle: Subtitle<'_>) -> impl IntoElement {
    let line = div()
        .text_size(scaled_px(style, 11.))
        .font_family(FONT_FAMILY);
    match subtitle {
        Subtitle::Plain(text) => line
            .text_color(to_gpui(style.muted_color()))
            .child(text.to_string()),
        Subtitle::Topic { text, hits, on_hit } => {
            let mut base = gpui_text_style(style, 11., false);
            base.color = to_gpui(style.muted_color());
            let runs = irc_runs_to_gpui(&text.plain, &text.runs, style, &base, None);
            let styled = StyledText::new(SharedString::from(text.plain.clone())).with_runs(runs);
            let ranges: Vec<Range<usize>> = hits.iter().map(|(range, _)| range.clone()).collect();
            let targets: Vec<TextHit> = hits.iter().map(|(_, hit)| hit.clone()).collect();
            line.child(InteractiveText::new("header-topic", styled).on_click(
                ranges,
                move |index, window, cx| {
                    if let Some(hit) = targets.get(index) {
                        on_hit(hit.clone(), window, cx);
                    }
                },
            ))
        }
    }
}
