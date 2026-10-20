//! The channel list sheet (`/list [mask]`): the answer for the mask, most users
//! first, filtered by a fuzzy query. Enter or a click joins the channel, or
//! focuses it when the network already has it (the shell decides). Escape
//! closes. Port of `src/qml/ChannelListSheet.qml`.

use std::ops::Range;

use gpui::{
    combine_highlights, div, prelude::*, px, AnyElement, FontStyle, FontWeight, HighlightStyle,
    Hsla, SharedString, StyledText, UnderlineStyle,
};

use crate::model::channel_list::{channel_list_rows, listing_status, ListRow, ListedChannel};
use crate::model::{Style, StyleRun};
use crate::ui::sheets::filter_sheet::{match_highlights, match_text, FilterSource};
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// Sheet width in logical pixels (`ChannelListSheet.qml`).
const WIDTH: f32 = 480.;
/// The list stops growing at this height in logical pixels, then scrolls.
const LIST_CAP: f32 = 280.;
/// One row's height in logical pixels.
const ROW_HEIGHT: f32 = 28.;

/// What the sheet lists, captured when it opens: the `LIST` answer for its mask.
pub struct ChannelListSource {
    pub listing: Vec<ListedChannel>,
}

impl FilterSource for ChannelListSource {
    type Item = ListRow;

    fn placeholder(&self) -> &'static str {
        "Filter channels…"
    }

    fn items(&self, query: &str) -> Vec<ListRow> {
        channel_list_rows(&self.listing, query)
    }

    fn width(&self) -> f32 {
        WIDTH
    }

    fn row_height(&self) -> f32 {
        ROW_HEIGHT
    }

    fn list_cap(&self) -> f32 {
        LIST_CAP
    }

    fn footer(&self, query: &str, visible: usize) -> Option<String> {
        Some(listing_status(self.listing.len(), visible, query))
    }

    fn render_row(&self, item: &ListRow, selected: bool, style: &Style) -> AnyElement {
        channel_row(style, item, selected).into_any_element()
    }
}

/// One 28px row. The name sits at 8px, and the user count and the topic follow
/// 10px apart. The name is ink when the row is selected and muted otherwise, and
/// the count and topic are always muted. The row takes `hover` when selected.
fn channel_row(style: &Style, row: &ListRow, selected: bool) -> impl IntoElement {
    let ink = to_gpui(style.ink_color());
    let muted = to_gpui(style.muted_color());
    let name_color = if selected { ink } else { muted };
    div()
        .h(scaled_px(style, ROW_HEIGHT))
        .px(scaled_px(style, 8.))
        .rounded(scaled_px(style, 7.))
        .bg(if selected {
            to_gpui(style.hover_color())
        } else {
            gpui::transparent_black()
        })
        .flex()
        .items_center()
        .overflow_hidden()
        .font_family(FONT_FAMILY)
        .text_size(scaled_px(style, 12.))
        .child(div().flex_none().text_color(name_color).child(match_text(
            row.name.clone(),
            &row.name_indices,
            ink,
        )))
        .child(
            div()
                .flex_none()
                .ml(scaled_px(style, 10.))
                .text_color(muted)
                .child(row.users.to_string()),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .ml(scaled_px(style, 10.))
                .overflow_hidden()
                .text_ellipsis()
                .text_color(muted)
                .child(topic_text(row, ink)),
        )
}

/// The topic as styled text: its bold, italic and underline runs, with the
/// matched characters drawn bold in ink. Colours were removed in the model.
fn topic_text(row: &ListRow, ink: Hsla) -> StyledText {
    let emphasis = row.topic.runs.iter().filter_map(emphasis_highlight);
    let matches = match_highlights(&row.topic.plain, &row.topic_indices, ink);
    let highlights: Vec<(Range<usize>, HighlightStyle)> =
        combine_highlights(emphasis, matches).collect();
    StyledText::new(SharedString::from(row.topic.plain.clone())).with_highlights(highlights)
}

/// The highlight for a run that is bold, italic or underlined. A plain run needs none.
/// The colour is left to the row, so the topic keeps its muted ink.
fn emphasis_highlight(run: &StyleRun) -> Option<(Range<usize>, HighlightStyle)> {
    let style = &run.style;
    if !(style.bold || style.italic || style.underline) {
        return None;
    }
    Some((
        run.range.clone(),
        HighlightStyle {
            font_weight: style.bold.then_some(FontWeight::BOLD),
            font_style: style.italic.then_some(FontStyle::Italic),
            underline: style.underline.then(|| UnderlineStyle {
                thickness: px(1.),
                color: None,
                wavy: false,
            }),
            ..Default::default()
        },
    ))
}
