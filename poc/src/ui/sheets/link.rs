//! The Link sheet (`Ctrl+Shift+O`): on a conversation, the topic's links and
//! channel names, then the transcript's, newest row first; on Status, the Status
//! rows' links and INVITE channels. Enter opens the selected URL in the browser,
//! joins the invited channel, or switches to a channel (asking first when the
//! network does not have it yet).

use gpui::{AnyElement, IntoElement};

use crate::model::links::{elide_middle_marked, link_targets, status_link_targets, LinkTarget};
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

/// What the sheet searches, captured when it opens.
pub enum LinkSource {
    /// A conversation: its topic when the sheet opened, its transcript rows, and
    /// the network's CHANTYPES, which decide what counts as a channel name.
    Conversation {
        topic: Option<String>,
        rows: Vec<Row>,
        chantypes: String,
    },
    /// A network's Status pane: its rows when the sheet opened.
    Status { rows: Vec<Row> },
}

impl FilterSource for LinkSource {
    type Item = LinkTarget;

    fn placeholder(&self) -> &'static str {
        "Open link…"
    }

    fn items(&self, query: &str) -> Vec<LinkTarget> {
        match self {
            Self::Conversation {
                topic,
                rows,
                chantypes,
            } => link_targets(topic.as_deref(), rows, query, chantypes),
            Self::Status { rows } => status_link_targets(rows, query),
        }
    }

    fn width(&self) -> f32 {
        WIDTH
    }

    fn row_height(&self) -> f32 {
        28.
    }

    fn render_row(&self, item: &LinkTarget, selected: bool, style: &Style) -> AnyElement {
        let max_chars = ((WIDTH - HORIZONTAL_INSET) / (CHAR_ADVANCE * LABEL_SIZE)).floor() as usize;
        let (label, highlights) = elide_middle_marked(&item.text, max_chars, &item.indices);
        text_row(style, 28., label, &highlights, selected).into_any_element()
    }
}
