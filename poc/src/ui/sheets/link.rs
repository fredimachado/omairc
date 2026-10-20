//! The Link sheet (`Ctrl+Shift+O`): the topic's links, then the transcript's
//! links newest row first. Enter opens the selected link in the browser.

use gpui::{AnyElement, IntoElement};

use crate::model::links::{elide_middle_marked, link_targets, LinkTarget};
use crate::model::{Row, Style};
use crate::ui::sheets::filter_sheet::{text_row, FilterSource};

/// Sheet width in logical pixels (Qt `LinkSheet.qml`).
const WIDTH: f32 = 420.;
/// Padding on the card (16 each side) and the row's 8px inset, both sides.
const HORIZONTAL_INSET: f32 = 48.;
/// The mono font's advance per point of size, used to count how many
/// characters fit a row. Approximate: the sheet elides by characters, not
/// measured pixels.
const CHAR_ADVANCE: f32 = 0.6;
const LABEL_SIZE: f32 = 12.;

pub struct LinkSource {
    /// The conversation's topic when the sheet opened.
    pub topic: Option<String>,
    /// The transcript rows when the sheet opened.
    pub rows: Vec<Row>,
}

impl FilterSource for LinkSource {
    type Item = LinkTarget;

    fn placeholder(&self) -> &'static str {
        "Open link…"
    }

    fn items(&self, query: &str) -> Vec<LinkTarget> {
        link_targets(self.topic.as_deref(), &self.rows, query)
    }

    fn width(&self) -> f32 {
        WIDTH
    }

    fn row_height(&self) -> f32 {
        28.
    }

    fn render_row(&self, item: &LinkTarget, selected: bool, style: &Style) -> AnyElement {
        let max_chars = ((WIDTH - HORIZONTAL_INSET) / (CHAR_ADVANCE * LABEL_SIZE)).floor() as usize;
        let (label, highlights) = elide_middle_marked(&item.url, max_chars, &item.indices);
        text_row(style, 28., label, &highlights, selected).into_any_element()
    }
}
