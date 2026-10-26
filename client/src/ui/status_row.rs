//! One line of a network's Status pane: the time,
//! the `glyph label` column, and the text. Its http(s) URLs and INVITE channels
//! are clickable; the transcript's pointer code finds those hits (see
//! `transcript/pointer.rs`), so this row only draws them underlined.

use std::ops::Range;

use gpui::{div, prelude::*, px, SharedString, Stateful, StyledText};

use crate::irc::statusentry::LogSeverity;
use crate::model::status::{body_is_muted, label_tone, LabelTone};
use crate::model::{mix_colors, Row, Style};
use crate::ui::format_ui::{gpui_text_style, irc_runs_to_gpui, link_underline_run, IrcColors};
use crate::ui::message_row::{BodyTextElement, RowLayoutMap};
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The Status row for `row_ix`. Colours follow the severity (see `label_tone`
/// and `body_is_muted`). `hovered_link` underlines the link under the pointer.
pub fn render_status_row(
    row_ix: usize,
    row: &Row,
    style: &Style,
    selection_range: Option<Range<usize>>,
    hovered_link: Option<Range<usize>>,
    layouts: RowLayoutMap,
    colors: IrcColors,
) -> Stateful<gpui::Div> {
    let severity = row
        .status
        .as_ref()
        .map_or(LogSeverity::Info, |mark| mark.severity);
    let label_color = match label_tone(severity) {
        LabelTone::Accent => style.accent_color(),
        LabelTone::Muted => style.muted_color(),
        LabelTone::Mixed => mix_colors(style.page_color(), style.ink_color(), 0.62),
    };
    let body_color = if body_is_muted(severity) {
        style.muted_color()
    } else {
        style.ink_color()
    };
    let label = row
        .status
        .as_ref()
        .map(|mark| mark.label_text())
        .unwrap_or_default();

    let mut base = gpui_text_style(style, 12., false);
    base.color = to_gpui(body_color);
    let mut runs = irc_runs_to_gpui(
        &row.body.plain,
        &row.body.runs,
        style,
        &base,
        colors,
        selection_range.clone(),
    );
    if let Some(link) = hovered_link {
        runs = link_underline_run(runs, link, true);
    }
    let text = StyledText::new(SharedString::from(row.body.plain.clone())).with_runs(runs);

    // Qt's columns: time at 24 (68 wide), label at 96 (88 wide), body at 192 and
    // running to 24 short of the right edge.
    div()
        .id(("transcript-row", row_ix))
        .w_full()
        .py(scaled_px(style, 4.))
        .pl(scaled_px(style, 24.))
        .flex()
        .items_start()
        .child(
            div()
                .w(scaled_px(style, 68.))
                .flex_shrink_0()
                .text_size(scaled_px(style, 10.))
                .text_color(to_gpui(style.muted_color()))
                .font_family(FONT_FAMILY)
                .child(row.time.clone().unwrap_or_default()),
        )
        .child(
            div()
                .ml(scaled_px(style, 4.))
                .w(scaled_px(style, 88.))
                .flex_shrink_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(scaled_px(style, 10.))
                .text_color(to_gpui(label_color))
                .font_family(FONT_FAMILY)
                .child(label),
        )
        .child(
            div()
                .ml(scaled_px(style, 8.))
                .pr(scaled_px(style, 24.))
                .flex_grow()
                .min_w_0()
                .text_size(scaled_px(style, 12.))
                .font_family(FONT_FAMILY)
                .child(BodyTextElement {
                    row_ix,
                    text,
                    layouts,
                    // A Status row has no header or avatar, so a pointer hit
                    // anywhere in the row reads the text.
                    avatar_top: px(0.),
                    header_height: px(0.),
                    selection_range,
                    selection_color: to_gpui(style.selection_color()),
                }),
        )
}
