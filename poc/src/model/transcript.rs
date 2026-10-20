use super::format::{format_irc, FormattedText};
use super::membership::membership_text;
use super::mock::MockMessage;
use super::status::StatusMark;
use super::text_edit::{snap_grapheme, word_bounds};
use super::unread::{shows_row, UNREAD_LINE};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Message,
    Event,
    DaySeparator,
    /// A line in a network's Status pane. Its label, glyph and colour live in `Row::status`.
    Status,
    /// A whois-style line in a conversation: the `/help` list, `/pref` replies and
    /// the `/version` reply. Left-aligned and muted, with its text in `body`.
    Whois,
    /// The "New messages" boundary in front of the first line that arrived unseen
    /// (`model::unread`). Drawn between two rules, and never copied.
    UnreadMark,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: u64,
    pub kind: RowKind,
    pub author: Option<String>,
    pub time: Option<String>,
    pub body: FormattedText,
    pub continues: bool,
    /// A `/me` line: the body renders italic and muted, as Qt's action rows do.
    pub action: bool,
    pub separator_text: Option<String>,
    pub event_text: Option<String>,
    /// Set on `RowKind::Status` rows only.
    pub status: Option<StatusMark>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub row: usize,
    pub offset: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Pos,
    pub focus: Pos,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerZone {
    HeaderOrAvatar,
    GapAbove,
}

/// Maps a pointer hit to a text position. `GapAbove` selects the end of the
/// previous row's plain body; row 0 maps to offset 0.
pub fn map_pointer(rows: &[Row], row: usize, zone: PointerZone) -> Pos {
    match zone {
        PointerZone::HeaderOrAvatar => Pos { row, offset: 0 },
        PointerZone::GapAbove => {
            if row == 0 {
                Pos { row: 0, offset: 0 }
            } else {
                let prev = row - 1;
                Pos {
                    row: prev,
                    offset: plain_len(&rows[prev]),
                }
            }
        }
    }
}

struct Fenwick {
    tree: Vec<i64>,
}

impl Fenwick {
    fn new(n: usize) -> Self {
        Self { tree: vec![0; n + 1] }
    }

    fn ensure_rows(&mut self, rows: usize) {
        let need = rows + 1;
        if self.tree.len() < need {
            self.tree.resize(need, 0);
        }
    }

    fn add(&mut self, mut idx: usize, delta: i64) {
        self.ensure_rows(idx + 1);
        idx += 1;
        while idx < self.tree.len() {
            self.tree[idx] += delta;
            idx += idx.isolate_lowest_one();
        }
    }

    fn prefix_sum(&self, mut idx: usize) -> i64 {
        idx += 1;
        let mut sum = 0i64;
        while idx > 0 && idx < self.tree.len() {
            sum += self.tree[idx];
            idx -= idx.isolate_lowest_one();
        }
        sum
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollAnchor {
    pub row: usize,
    pub offset_px: i32,
    pub pinned_to_bottom: bool,
}

pub struct Transcript {
    rows: Vec<Row>,
    heights: Fenwick,
    measured: Vec<bool>,
    estimate_height: i32,
    scroll: ScrollAnchor,
    selection: Option<Selection>,
}

impl Transcript {
    pub fn new(estimate_height: i32) -> Self {
        Self {
            rows: Vec::new(),
            heights: Fenwick::new(0),
            measured: Vec::new(),
            estimate_height: estimate_height.max(1),
            scroll: ScrollAnchor {
                row: 0,
                offset_px: 0,
                pinned_to_bottom: true,
            },
            selection: None,
        }
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    pub fn scroll(&self) -> ScrollAnchor {
        self.scroll
    }

    pub fn set_rows(&mut self, rows: Vec<Row>) {
        self.rows = rows;
        self.measured = vec![false; self.rows.len()];
        self.heights = Fenwick::new(self.rows.len());
        for i in 0..self.rows.len() {
            self.heights.add(i, self.estimate_height as i64);
        }
        self.selection = None;
        self.scroll = ScrollAnchor {
            row: self.rows.len().saturating_sub(1),
            offset_px: 0,
            pinned_to_bottom: true,
        };
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub fn set_measured(&mut self, row: usize, height: i32) {
        if row >= self.rows.len() {
            return;
        }
        let h = height.max(1) as i64;
        let old = if self.measured[row] {
            self.row_height(row) as i64
        } else {
            self.estimate_height as i64
        };
        self.heights.add(row, h - old);
        self.measured[row] = true;
    }

    fn row_height(&self, row: usize) -> i32 {
        if row >= self.rows.len() {
            return self.estimate_height;
        }
        if row == 0 {
            return self.heights.prefix_sum(0) as i32;
        }
        (self.heights.prefix_sum(row) - self.heights.prefix_sum(row - 1)) as i32
    }

    pub fn invalidate_all(&mut self) {
        self.measured.fill(false);
        self.heights = Fenwick::new(self.rows.len());
        for i in 0..self.rows.len() {
            self.heights.add(i, self.estimate_height as i64);
        }
    }

    pub fn set_conversation(&mut self, messages: &[MockMessage]) {
        self.set_rows(rows_from_messages(messages));
    }

    pub fn append(&mut self, row: Row) {
        self.rows.push(row);
        self.measured.push(false);
        self.heights.ensure_rows(self.rows.len());
        self.heights
            .add(self.rows.len() - 1, self.estimate_height as i64);
        if self.scroll.pinned_to_bottom {
            self.scroll.row = self.rows.len() - 1;
            self.scroll.offset_px = 0;
        }
    }

    pub fn set_pinned_to_bottom(&mut self, pinned: bool) {
        self.scroll.pinned_to_bottom = pinned;
        if pinned && !self.rows.is_empty() {
            self.scroll.row = self.rows.len() - 1;
            self.scroll.offset_px = 0;
        }
    }

    pub fn set_scroll_anchor(&mut self, row: usize, offset_px: i32) {
        self.scroll.pinned_to_bottom = false;
        self.scroll.row = row.min(self.rows.len().saturating_sub(1));
        self.scroll.offset_px = offset_px;
    }

    pub fn begin(&mut self, pos: Pos) {
        let pos = snap_pos(self.rows.as_slice(), pos);
        self.selection = Some(Selection {
            anchor: pos,
            focus: pos,
        });
    }

    pub fn extend(&mut self, pos: Pos) {
        let pos = snap_pos(self.rows.as_slice(), pos);
        if let Some(sel) = &mut self.selection {
            sel.focus = pos;
        } else {
            self.begin(pos);
        }
    }

    pub fn select_word(&mut self, pos: Pos) {
        let pos = snap_pos(self.rows.as_slice(), pos);
        if pos.row >= self.rows.len() {
            return;
        }
        let text = row_plain(&self.rows[pos.row]);
        let (start, end) = word_bounds(text, pos.offset);
        self.selection = Some(Selection {
            anchor: Pos {
                row: pos.row,
                offset: start,
            },
            focus: Pos {
                row: pos.row,
                offset: end,
            },
        });
    }

    pub fn select_row(&mut self, row: usize) {
        if row >= self.rows.len() {
            return;
        }
        let end = plain_len(&self.rows[row]);
        self.selection = Some(Selection {
            anchor: Pos { row, offset: 0 },
            focus: Pos { row, offset: end },
        });
    }

    pub fn range_for_row(&self, row: usize) -> Option<Range<usize>> {
        let sel = self.selection?;
        let (start, end) = ordered_positions(sel);
        if row < start.row || row > end.row {
            return None;
        }
        let row_start = if row == start.row { start.offset } else { 0 };
        let row_end = if row == end.row {
            end.offset
        } else {
            plain_len(&self.rows[row])
        };
        if row_start >= row_end {
            return None;
        }
        Some(row_start..row_end)
    }

    pub fn copy_text(&self) -> String {
        let Some(sel) = self.selection else {
            return String::new();
        };
        let (start, end) = ordered_positions(sel);
        if start.row == end.row && start.offset == end.offset {
            return String::new();
        }
        if start.row == end.row {
            return slice_plain(row_plain(&self.rows[start.row]), start.offset..end.offset);
        }
        let mut lines = Vec::new();
        for row in start.row..=end.row {
            let body = row_plain(&self.rows[row]).to_string();
            let selected = if let Some(range) = self.range_for_row(row) {
                slice_plain(&body, range)
            } else {
                continue;
            };
            let line = match self.rows[row].kind {
                RowKind::Message if self.rows[row].action => format!(
                    "[{}] * {} {}",
                    self.rows[row].time.as_deref().unwrap_or("--:--"),
                    self.rows[row].author.as_deref().unwrap_or(""),
                    selected
                ),
                RowKind::Message => format!(
                    "[{}] <{}> {}",
                    self.rows[row].time.as_deref().unwrap_or("--:--"),
                    self.rows[row].author.as_deref().unwrap_or(""),
                    selected
                ),
                RowKind::Event => format!("* {selected}"),
                RowKind::DaySeparator => selected,
                RowKind::Status => format!(
                    "[{}] {} {}",
                    self.rows[row].time.as_deref().unwrap_or("--:--"),
                    self.rows[row]
                        .status
                        .as_ref()
                        .map_or("", |mark| mark.label.as_str()),
                    selected
                ),
                RowKind::Whois => selected,
                RowKind::UnreadMark => continue,
            };
            lines.push(line);
        }
        lines.join("\n")
    }
}

fn slice_plain(text: &str, range: Range<usize>) -> String {
    if range.is_empty() {
        return String::new();
    }
    text.get(range.clone()).unwrap_or("").to_string()
}

fn ordered_positions(sel: Selection) -> (Pos, Pos) {
    if sel.anchor.row < sel.focus.row
        || (sel.anchor.row == sel.focus.row && sel.anchor.offset <= sel.focus.offset)
    {
        (sel.anchor, sel.focus)
    } else {
        (sel.focus, sel.anchor)
    }
}

fn snap_pos(rows: &[Row], pos: Pos) -> Pos {
    if pos.row >= rows.len() {
        return Pos {
            row: rows.len().saturating_sub(1),
            offset: 0,
        };
    }
    let len = plain_len(&rows[pos.row]);
    Pos {
        row: pos.row,
        offset: snap_grapheme(row_plain(&rows[pos.row]), pos.offset.min(len)),
    }
}

fn plain_len(row: &Row) -> usize {
    row_plain(row).len()
}

fn row_plain(row: &Row) -> &str {
    match row.kind {
        RowKind::DaySeparator => row
            .separator_text
            .as_deref()
            .unwrap_or(row.body.plain.as_str()),
        RowKind::Event => row
            .event_text
            .as_deref()
            .unwrap_or(row.body.plain.as_str()),
        RowKind::Message | RowKind::Status | RowKind::Whois | RowKind::UnreadMark => {
            &row.body.plain
        }
    }
}

pub fn rows_from_messages(messages: &[MockMessage]) -> Vec<Row> {
    let mut rows = Vec::with_capacity(messages.len());
    for (index, message) in messages.iter().enumerate() {
        if !shows_row(message, rows.last()) {
            continue;
        }
        let row = row_for_message(index as u64 + 1, message, rows.last());
        rows.push(row);
    }
    rows
}

/// The row for one stored line, numbered `id`. `previous` is the row above it,
/// which decides whether a chat line folds under it (`continues_group`). Used
/// for stored lines and for lines appended while a conversation is open.
pub fn row_for_message(id: u64, message: &MockMessage, previous: Option<&Row>) -> Row {
    match message.kind.as_str() {
        "message" | "action" => {
            let continues = continues_group(previous, &message.author, &message.time);
            row_from_mock_message(id, message, continues)
        }
        "day" => {
            let text = message.body.clone();
            Row {
                id,
                kind: RowKind::DaySeparator,
                author: None,
                time: None,
                body: format_irc(&text),
                continues: false,
                action: false,
                separator_text: Some(text),
                event_text: None,
                status: None,
            }
        }
        "whois" => row_from_whois(id, &message.body),
        UNREAD_LINE => Row {
            id,
            kind: RowKind::UnreadMark,
            author: None,
            time: None,
            body: format_irc("New messages"),
            continues: false,
            action: false,
            separator_text: None,
            event_text: None,
            status: None,
        },
        _ => {
            let event_text = membership_text(message);
            Row {
                id,
                kind: RowKind::Event,
                author: if message.author.is_empty() {
                    None
                } else {
                    Some(message.author.clone())
                },
                time: if message.time.is_empty() {
                    None
                } else {
                    Some(message.time.clone())
                },
                body: format_irc(&event_text),
                continues: false,
                action: false,
                separator_text: None,
                event_text: Some(event_text),
                status: None,
            }
        }
    }
}

/// Qt folds a chat line under the one above when both are chat lines from the
/// same author in the same minute (`continuesMessageGroup`).
pub fn continues_group(previous: Option<&Row>, author: &str, time: &str) -> bool {
    if author.is_empty() || time.is_empty() {
        return false;
    }
    previous.is_some_and(|prev| {
        prev.kind == RowKind::Message
            && prev.author.as_deref() == Some(author)
            && prev.time.as_deref() == Some(time)
    })
}

pub fn row_from_mock_message(id: u64, message: &MockMessage, continues: bool) -> Row {
    Row {
        id,
        kind: RowKind::Message,
        author: Some(message.author.clone()),
        time: Some(message.time.clone()),
        body: format_irc(&message.body),
        continues,
        action: message.kind == "action",
        separator_text: None,
        event_text: None,
        status: None,
    }
}

/// A whois-style line. It has no author, time or avatar: only the text.
pub fn row_from_whois(id: u64, text: &str) -> Row {
    Row {
        id,
        kind: RowKind::Whois,
        author: None,
        time: None,
        body: format_irc(text),
        continues: false,
        action: false,
        separator_text: None,
        event_text: None,
        status: None,
    }
}

pub fn row_from_message(id: u64, author: &str, time: &str, body: &str, continues: bool) -> Row {
    Row {
        id,
        kind: RowKind::Message,
        author: Some(author.to_string()),
        time: Some(time.to_string()),
        body: format_irc(body),
        continues,
        action: false,
        separator_text: None,
        event_text: None,
        status: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rows() -> Vec<Row> {
        vec![
            row_from_message(1, "anna", "09:41", "hello", false),
            row_from_message(2, "dax", "09:43", "world", false),
        ]
    }

    /// Selects the whole transcript, from the first row's start to the last row's end.
    fn select_everything(t: &mut Transcript) {
        let last = t.rows().len() - 1;
        let end = plain_len(&t.rows()[last]);
        t.begin(Pos { row: 0, offset: 0 });
        t.extend(Pos {
            row: last,
            offset: end,
        });
    }

    #[test]
    fn fenwick_prefix_and_update() {
        let mut t = Transcript::new(20);
        t.set_rows(sample_rows());
        t.set_measured(0, 30);
        assert_eq!(t.row_height(0), 30);
        assert_eq!(t.row_height(1), 20);
        t.set_measured(1, 40);
        assert_eq!(t.row_height(0), 30);
        assert_eq!(t.row_height(1), 40);
    }

    #[test]
    fn anchor_survives_remeasure_above() {
        let mut t = Transcript::new(10);
        t.set_rows(sample_rows());
        t.set_pinned_to_bottom(false);
        t.set_scroll_anchor(1, 0);
        t.set_measured(0, 100);
        assert_eq!(t.scroll.row, 1);
    }

    #[test]
    fn pinned_stays_on_append() {
        let mut t = Transcript::new(10);
        t.set_rows(sample_rows());
        t.set_pinned_to_bottom(true);
        t.append(row_from_message(3, "mira", "09:46", "more", false));
        assert_eq!(t.scroll.row, t.rows.len() - 1);
        assert!(t.scroll.pinned_to_bottom);
    }

    #[test]
    fn a_stored_whois_line_becomes_a_whois_row_and_breaks_a_group() {
        let messages = vec![
            MockMessage {
                kind: "message".to_string(),
                author: "anna".to_string(),
                time: "09:41".to_string(),
                day: "2026-10-19".to_string(),
                body: "hello".to_string(),
            },
            crate::model::mock::whois_message("VERSION reply from anna: https://omairc.app"),
            MockMessage {
                kind: "message".to_string(),
                author: "anna".to_string(),
                time: "09:41".to_string(),
                day: "2026-10-19".to_string(),
                body: "again".to_string(),
            },
        ];
        let rows = rows_from_messages(&messages);
        assert_eq!(rows[1].kind, RowKind::Whois);
        assert_eq!(rows[1].body.plain, "VERSION reply from anna: https://omairc.app");
        // The whois line sits between the two, so the second line starts a new header.
        assert!(!rows[2].continues);
    }

    #[test]
    fn an_appended_line_folds_under_the_row_above_like_a_stored_one() {
        let first = row_from_message(1, "fred", "21:00", "one", false);
        let folded = row_for_message(
            2,
            &MockMessage {
                kind: "message".to_string(),
                author: "fred".to_string(),
                time: "21:00".to_string(),
                day: "2026-10-19".to_string(),
                body: "two".to_string(),
            },
            Some(&first),
        );
        assert!(folded.continues);
    }

    #[test]
    fn a_selection_over_the_unread_mark_copies_the_lines_only() {
        use crate::model::unread::mark_line;
        let messages = [
            crate::model::mock::MockMessage {
                kind: "message".to_string(),
                author: "anna".to_string(),
                time: "09:00".to_string(),
                day: String::new(),
                body: "one".to_string(),
            },
            mark_line(),
            crate::model::mock::MockMessage {
                kind: "message".to_string(),
                author: "dax".to_string(),
                time: "09:01".to_string(),
                day: String::new(),
                body: "two".to_string(),
            },
        ];
        let mut t = Transcript::new(10);
        t.set_rows(rows_from_messages(&messages));
        select_everything(&mut t);
        assert_eq!(t.copy_text(), "[09:00] <anna> one\n[09:01] <dax> two");
    }

    #[test]
    fn a_whois_row_copies_its_text_alone() {
        let mut t = Transcript::new(10);
        t.set_rows(vec![row_from_whois(1, "Commands: /me, /help.")]);
        select_everything(&mut t);
        assert_eq!(t.copy_text(), "Commands: /me, /help.");
    }

    #[test]
    fn copy_single_row_no_timestamp() {
        let mut t = Transcript::new(10);
        t.set_rows(sample_rows());
        t.begin(Pos { row: 0, offset: 1 });
        t.extend(Pos { row: 0, offset: 3 });
        assert_eq!(t.copy_text(), "el");
    }

    #[test]
    fn copy_two_rows_with_prefix() {
        let mut t = Transcript::new(10);
        t.set_rows(sample_rows());
        t.begin(Pos { row: 0, offset: 0 });
        t.extend(Pos { row: 1, offset: 5 });
        let text = t.copy_text();
        assert!(text.contains("[09:41] <anna> hello"));
        assert!(text.contains("[09:43] <dax> world"));
    }

    #[test]
    fn reversed_drag() {
        let mut t = Transcript::new(10);
        t.set_rows(sample_rows());
        t.begin(Pos { row: 1, offset: 3 });
        t.extend(Pos { row: 0, offset: 1 });
        assert_eq!(t.range_for_row(0), Some(1..5));
    }

    #[test]
    fn map_pointer_gap() {
        let rows = sample_rows();
        let pos = map_pointer(&rows, 1, PointerZone::GapAbove);
        assert_eq!(pos.row, 0);
        assert_eq!(pos.offset, rows[0].body.plain.len());
    }

    #[test]
    fn rows_from_messages_continues_same_author() {
        use crate::model::mock::MockMessage;
        let messages = [
            MockMessage {
                kind: "message".to_string(),
                author: "anna".to_string(),
                time: "09:00".to_string(),
                day: String::new(),
                body: "one".to_string(),
            },
            MockMessage {
                kind: "message".to_string(),
                author: "anna".to_string(),
                time: "09:00".to_string(),
                day: String::new(),
                body: "two".to_string(),
            },
            MockMessage {
                kind: "message".to_string(),
                author: "anna".to_string(),
                time: "09:01".to_string(),
                day: String::new(),
                body: "three".to_string(),
            },
        ];
        let rows = rows_from_messages(&messages);
        assert!(!rows[0].continues);
        assert!(rows[1].continues);
        assert!(!rows[2].continues, "a new minute starts a new header");
        assert_eq!(rows[0].body.plain, "one");
        assert_eq!(rows[1].body.plain, "two");
    }

    #[test]
    fn continues_group_needs_same_author_and_minute() {
        let prev = row_from_message(1, "anna", "09:00", "hi", false);
        assert!(continues_group(Some(&prev), "anna", "09:00"));
        assert!(!continues_group(Some(&prev), "dax", "09:00"));
        assert!(!continues_group(Some(&prev), "anna", "09:01"));
        assert!(!continues_group(None, "anna", "09:00"));
        assert!(!continues_group(Some(&prev), "", ""));
    }

    #[test]
    fn stored_action_renders_as_action_row() {
        use crate::model::mock::MockMessage;
        let messages = [MockMessage {
            kind: "action".to_string(),
            author: "fred".to_string(),
            time: "14:07".to_string(),
            day: String::new(),
            body: "waves".to_string(),
        }];
        let rows = rows_from_messages(&messages);
        assert_eq!(rows[0].kind, RowKind::Message);
        assert!(rows[0].action);
        assert_eq!(rows[0].body.plain, "waves");
        assert_eq!(rows[0].author.as_deref(), Some("fred"));
    }

    #[test]
    fn rows_from_messages_breaks_on_event_and_day() {
        use crate::model::mock::MockMessage;
        let messages = [
            MockMessage {
                kind: "message".to_string(),
                author: "anna".to_string(),
                time: "09:00".to_string(),
                day: String::new(),
                body: "before join".to_string(),
            },
            MockMessage {
                kind: "join".to_string(),
                author: "sol".to_string(),
                time: String::new(),
                day: String::new(),
                body: String::new(),
            },
            MockMessage {
                kind: "message".to_string(),
                author: "anna".to_string(),
                time: "09:02".to_string(),
                day: String::new(),
                body: "after join".to_string(),
            },
            MockMessage {
                kind: "day".to_string(),
                author: String::new(),
                time: String::new(),
                day: String::new(),
                body: "-- Monday --".to_string(),
            },
            MockMessage {
                kind: "message".to_string(),
                author: "anna".to_string(),
                time: "10:00".to_string(),
                day: String::new(),
                body: "next day".to_string(),
            },
        ];
        let rows = rows_from_messages(&messages);
        assert_eq!(rows.len(), 5);
        assert!(!rows[2].continues);
        assert!(!rows[4].continues);
        assert_eq!(rows[1].kind, RowKind::Event);
        assert_eq!(rows[3].kind, RowKind::DaySeparator);
    }

    #[test]
    fn set_conversation_replaces_rows_and_clears_selection() {
        use crate::model::mock::MockMessage;
        let mut t = Transcript::new(10);
        t.set_rows(sample_rows());
        t.begin(Pos { row: 0, offset: 1 });
        t.set_conversation(&[MockMessage {
            kind: "message".to_string(),
            author: "solo".to_string(),
            time: "12:00".to_string(),
            day: String::new(),
            body: "fresh".to_string(),
        }]);
        assert_eq!(t.rows().len(), 1);
        assert_eq!(t.rows()[0].author.as_deref(), Some("solo"));
        assert!(t.selection().is_none());
    }
}
