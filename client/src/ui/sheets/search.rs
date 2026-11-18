//! The history search sheet (`Ctrl+Shift+F`): a filter field over a list of hits from every
//! conversation, newest first. The sheet owns no search: the shell runs it and pushes the
//! results in through `FilterSheet::sync_source`, so a result list can grow while the user
//! types. Enter opens the hit; `ui/shell/search.rs` does that.

use gpui::{prelude::*, AnyElement, FontWeight, HighlightStyle, SharedString, StyledText};

use crate::model::search::{footer_text, time_label, SearchHit, SearchResults};
use crate::model::Style;
use crate::ui::sheets::filter_sheet::FilterSource;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

const WIDTH: f32 = 680.;
const ROW_HEIGHT: f32 = 28.;

/// What the search has found so far, and whether it is still looking.
#[derive(Clone, Debug, Default)]
pub struct SearchSource {
    pub results: SearchResults,
    pub query_blank: bool,
    pub scanning: bool,
    /// The local day, so a row from today shows only its minute.
    pub today: Option<chrono::NaiveDate>,
}

impl SearchSource {
    pub fn blank() -> Self {
        Self {
            query_blank: true,
            today: Some(chrono::Local::now().date_naive()),
            ..Self::default()
        }
    }
}

impl FilterSource for SearchSource {
    type Item = SearchHit;

    fn placeholder(&self) -> &'static str {
        "Search every conversation…"
    }

    /// The hits are for the query the shell searched, not the field's text, which may be
    /// ahead of them while the debounce runs.
    fn items(&self, _query: &str) -> Vec<SearchHit> {
        self.results.hits.clone()
    }

    fn walks_to_ends(&self) -> bool {
        true
    }

    fn width(&self) -> f32 {
        WIDTH
    }

    fn row_height(&self) -> f32 {
        ROW_HEIGHT
    }

    fn footer(&self, _query: &str, _visible: usize) -> Option<String> {
        Some(footer_text(self.query_blank, &self.results, self.scanning))
    }

    fn render_row(&self, hit: &SearchHit, selected: bool, style: &Style) -> AnyElement {
        let today = self.today.unwrap_or_else(|| chrono::Local::now().date_naive());
        let (text, highlights) = row_text(hit, today, style);
        gpui::div()
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
            .text_ellipsis()
            .whitespace_nowrap()
            .font_family(FONT_FAMILY)
            .text_size(scaled_px(style, 12.))
            .text_color(to_gpui(style.ink_color()))
            .child(StyledText::new(SharedString::from(text)).with_highlights(highlights))
            .into_any_element()
    }
}

/// One row's text, `network  #conversation  time  author  body`, and its highlights: the
/// leading columns muted, the matched nick and body ranges bold in the accent.
fn row_text(
    hit: &SearchHit,
    today: chrono::NaiveDate,
    style: &Style,
) -> (String, Vec<(std::ops::Range<usize>, HighlightStyle)>) {
    let muted = HighlightStyle {
        color: Some(to_gpui(style.muted_color())),
        ..Default::default()
    };
    let matched = HighlightStyle {
        color: Some(to_gpui(style.accent_color())),
        font_weight: Some(FontWeight::BOLD),
        ..Default::default()
    };
    let lead = format!(
        "{}  {}  {}  ",
        hit.network_name,
        hit.conversation_label,
        time_label(hit.time, today)
    );
    let mut text = lead.clone();
    let mut highlights = vec![(0..lead.len(), muted)];
    let author_start = text.len();
    text.push_str(&hit.author);
    if hit.author_match {
        highlights.push((author_start..text.len(), matched));
    }
    text.push_str("  ");
    let body_start = text.len();
    text.push_str(&hit.text);
    highlights.extend(
        hit.matches
            .iter()
            .map(|range| (body_start + range.start..body_start + range.end, matched)),
    );
    (text, highlights)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::event::ConversationKey;
    use crate::model::search::{SearchConversation, SearchLine, SearchQuery};
    use crate::model::theme::Theme;
    use chrono::TimeZone;

    #[test]
    fn highlights_land_on_the_matched_text_whatever_the_lead_columns_are() {
        let query = SearchQuery::parse("tea").unwrap();
        let room = SearchConversation {
            network_id: "omarchy".into(),
            network_name: "Omarchy".into(),
            conversation: ConversationKey::new("omarchy", "#ricing"),
            label: "#ricing".into(),
        };
        let hit = room
            .hit(
                &query,
                &SearchLine {
                    time: chrono::Utc.timestamp_millis_opt(1_700_000_000_000).unwrap(),
                    author: "Teagan",
                    kind: "message",
                    body: "iced TEA café tea",
                    msgid: "",
                },
            )
            .unwrap();
        let style = Style::new(Theme::defaults_dark(), 1.0);
        let today = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let (text, highlights) = row_text(&hit, today, &style);
        let marked: Vec<&str> = highlights.iter().skip(1).map(|(r, _)| &text[r.clone()]).collect();
        assert_eq!(marked, ["Teagan", "TEA", "tea"]);
        assert!(text.starts_with("Omarchy  #ricing  "));
    }
}
