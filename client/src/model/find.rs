//! Find mode (Ctrl+F): the transcript search. Ports `findNextMatch`,
//! `advanceFind` and `transcriptRowText` in `client/src/ui/shell`. A match is a
//! case-insensitive substring of the row's plain text, with the author or the
//! Status label in front. Toolkit-free: the shell owns the composer and the
//! transcript, and passes the rows in.

use super::transcript::{Row, RowKind};

/// Find mode's place in the transcript: the row it last landed on and the
/// query it last searched. The composer holds the query and the draft (see
/// `ComposerState::begin_find`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Find {
    query: String,
    current: Option<usize>,
}

impl Find {
    /// A search that has not run yet, so the first `advance` starts from the top.
    pub fn new() -> Self {
        Self::default()
    }

    /// The row drawn with the selection colour, if the search has landed on one.
    pub fn current(&self) -> Option<usize> {
        self.current
    }

    /// The query the last `advance` searched for. Typing changes the composer's
    /// text, and the shell compares it with this to know when to search again.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Runs the search for `query` over `rows`. `from_start` searches from the
    /// top, as the first press of Ctrl+F does. Otherwise it searches after the
    /// current row and wraps, so the current row is only the last thing checked.
    ///
    /// Returns the row to reveal when the search finds one. An empty query
    /// clears the current row. A query with no match leaves the current row where
    /// it was, as Qt does, so the last match stays highlighted.
    pub fn advance(
        &mut self,
        rows: &[Row],
        status_view: bool,
        query: &str,
        from_start: bool,
    ) -> Option<usize> {
        self.query = query.to_string();
        if query.is_empty() {
            self.current = None;
            return None;
        }
        let start = if from_start { None } else { self.current };
        let index = find_next(rows, status_view, query, start)?;
        self.current = Some(index);
        Some(index)
    }
}

/// The text find searches in one row: the author or Status label, a space, then
/// the body. `status_view` picks the label, as Qt's `consoleVisible` does.
pub fn row_search_text(row: &Row, status_view: bool) -> String {
    let lead = if status_view {
        row.status.as_ref().map_or("", |mark| mark.label.as_str())
    } else {
        row.author.as_deref().unwrap_or("")
    };
    format!("{lead} {}", row.body.plain)
}

/// The first row after `start` whose search text holds `query`, ignoring case.
/// `None` starts from the top. The search wraps, and the start row is checked
/// last. An empty query matches nothing.
pub fn find_next(
    rows: &[Row],
    status_view: bool,
    query: &str,
    start: Option<usize>,
) -> Option<usize> {
    let count = rows.len();
    if count == 0 || query.is_empty() {
        return None;
    }
    let needle = query.to_lowercase();
    (1..=count).find_map(|step| {
        let index = match start {
            None => step - 1,
            Some(current) => (current + step) % count,
        };
        // The "New messages" boundary is chrome, not a line to land on.
        if rows[index].kind == RowKind::UnreadMark {
            return None;
        }
        let hay = row_search_text(&rows[index], status_view).to_lowercase();
        hay.contains(&needle).then_some(index)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::statusentry::{LogSource, StatusEntry};
    use crate::model::status::row_from_status_entry;
    use crate::model::transcript::row_from_message;

    fn chat(id: u64, author: &str, body: &str) -> Row {
        row_from_message(id, author, "09:00", body, false)
    }

    fn status(id: u64, label: &str, text: &str) -> Row {
        let entry = StatusEntry {
            label: label.to_string(),
            source: LogSource::Server,
            ..crate::irc::statusentry::outcome("omarchy", text, chrono::Utc::now())
        };
        row_from_status_entry(id, &entry)
    }

    fn sample() -> Vec<Row> {
        vec![
            chat(1, "anna", "morning all"),
            chat(2, "dax", "the build is green"),
            chat(3, "anna", "Build went out"),
            chat(4, "mira", "tea?"),
        ]
    }

    #[test]
    fn a_conversation_row_is_searched_as_author_space_body() {
        let row = chat(1, "anna", "hello");
        assert_eq!(row_search_text(&row, false), "anna hello");
        // The author is searchable, and so is a phrase that crosses into the body.
        assert_eq!(find_next(std::slice::from_ref(&row), false, "anna h", None), Some(0));
        assert_eq!(find_next(&[row], false, "nna he", None), Some(0));
    }

    #[test]
    fn a_status_row_is_searched_as_label_space_text() {
        let row = status(1, "-AUTH-", "Looking up your hostname");
        assert_eq!(row_search_text(&row, true), "-AUTH- Looking up your hostname");
        assert_eq!(find_next(std::slice::from_ref(&row), true, "auth", None), Some(0));
        assert_eq!(find_next(&[row], true, "-auth- look", None), Some(0));
    }

    #[test]
    fn the_status_label_is_not_searched_in_a_conversation() {
        let row = status(1, "-AUTH-", "Looking up your hostname");
        assert_eq!(find_next(&[row], false, "auth", None), None);
    }

    #[test]
    fn the_author_is_not_searched_on_status() {
        let row = chat(1, "anna", "hello");
        assert_eq!(find_next(&[row], true, "anna", None), None);
    }

    #[test]
    fn matching_ignores_case_in_both_directions() {
        let rows = sample();
        assert_eq!(find_next(&rows, false, "BUILD", None), Some(1));
        assert_eq!(find_next(&rows, false, "went", None), Some(2));
    }

    #[test]
    fn matching_is_case_folded_for_non_ascii_text() {
        let rows = vec![chat(1, "anna", "Café au lait")];
        assert_eq!(find_next(&rows, false, "CAFÉ", None), Some(0));
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        let rows = sample();
        assert_eq!(find_next(&rows, false, "", None), None);
        assert_eq!(find_next(&rows, false, "", Some(2)), None);
    }

    #[test]
    fn an_empty_transcript_matches_nothing() {
        assert_eq!(find_next(&[], false, "anything", None), None);
        assert_eq!(find_next(&[], false, "anything", Some(0)), None);
    }

    #[test]
    fn a_search_from_the_top_finds_the_first_match() {
        let rows = sample();
        assert_eq!(find_next(&rows, false, "anna", None), Some(0));
        assert_eq!(find_next(&rows, false, "build", None), Some(1));
    }

    #[test]
    fn advancing_moves_to_the_next_match_and_wraps_to_the_first() {
        let rows = sample();
        let mut find = Find::new();

        assert_eq!(find.advance(&rows, false, "build", true), Some(1));
        assert_eq!(find.current(), Some(1));
        assert_eq!(find.advance(&rows, false, "build", false), Some(2));
        assert_eq!(find.advance(&rows, false, "build", false), Some(1), "wraps to the first match");
    }

    #[test]
    fn a_single_match_stays_put_when_advanced() {
        let rows = sample();
        let mut find = Find::new();
        assert_eq!(find.advance(&rows, false, "tea", true), Some(3));
        assert_eq!(find.advance(&rows, false, "tea", false), Some(3));
        assert_eq!(find.current(), Some(3));
    }

    #[test]
    fn a_search_from_the_top_restarts_after_the_current_row() {
        let rows = sample();
        let mut find = Find::new();
        assert_eq!(find.advance(&rows, false, "build", true), Some(1));
        assert_eq!(find.advance(&rows, false, "build", false), Some(2));
        // Typing starts the search again from the top, whatever row was current.
        assert_eq!(find.advance(&rows, false, "build", true), Some(1));
    }

    #[test]
    fn a_query_with_no_match_keeps_the_current_row() {
        let rows = sample();
        let mut find = Find::new();
        assert_eq!(find.advance(&rows, false, "tea", true), Some(3));
        assert_eq!(find.advance(&rows, false, "zebra", true), None);
        assert_eq!(find.current(), Some(3), "the last match stays highlighted");
        assert_eq!(find.query(), "zebra");
    }

    #[test]
    fn an_empty_query_clears_the_current_row() {
        let rows = sample();
        let mut find = Find::new();
        assert_eq!(find.advance(&rows, false, "tea", true), Some(3));
        assert_eq!(find.advance(&rows, false, "", true), None);
        assert_eq!(find.current(), None);
    }

    #[test]
    fn a_new_search_starts_with_nothing_highlighted() {
        let find = Find::new();
        assert_eq!(find.current(), None);
        assert_eq!(find.query(), "");
    }

    #[test]
    fn advancing_skips_rows_without_a_match_and_wraps() {
        let rows = sample();
        let mut find = Find::new();
        assert_eq!(find.advance(&rows, false, "anna", true), Some(0));
        assert_eq!(find.advance(&rows, false, "anna", false), Some(2));
        assert_eq!(find.advance(&rows, false, "anna", false), Some(0));
    }

    #[test]
    fn find_never_lands_on_the_new_messages_mark() {
        use crate::irc::event::MsgId;
        use crate::irc::world::{Message, MessageKind, Origin};
        use crate::model::transcript::rows_from_messages;
        use chrono::{Local, TimeZone, Utc};
        let at = |minute: u32| {
            Local
                .with_ymd_and_hms(2026, 10, 19, 9, minute, 0)
                .unwrap()
                .with_timezone(&Utc)
        };
        let line = |author: &str, body: &str, minute: u32| Message {
            kind: MessageKind::Message,
            author: author.to_string(),
            body: body.to_string(),
            time: at(minute),
            server_time: None,
            msgid: MsgId::default(),
            origin: Origin::Live,
            mentioned: false,
            collapsible: false,
            sequence: minute as i64,
            only_visible_to_you: false,
        };
        let messages = [line("anna", "one", 0), line("dax", "two", 1)];
        // The reducer plants the mark in front of the second line (its sequence).
        let rows = rows_from_messages(&messages, Some(1));
        let mark = rows.iter().position(|row| row.kind == RowKind::UnreadMark);
        assert!(mark.is_some(), "the mark row is in the transcript");
        assert_eq!(find_next(&rows, false, "new messages", None), None);
        let two = rows.iter().position(|row| row.body.plain == "two");
        assert_eq!(find_next(&rows, false, "two", None), two);
    }
}
