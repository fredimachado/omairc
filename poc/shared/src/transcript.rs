use crate::format::{format_irc, FormattedText};
use std::ops::Range;
use unicode_segmentation::GraphemeCursor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Message,
    Event,
    DaySeparator,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: u64,
    pub kind: RowKind,
    pub author: Option<String>,
    pub time: Option<String>,
    pub body: FormattedText,
    pub continues: bool,
    pub separator_text: Option<String>,
    pub event_text: Option<String>,
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

    fn add(&mut self, mut idx: usize, delta: i64) {
        idx += 1;
        while idx < self.tree.len() {
            self.tree[idx] += delta;
            idx += idx & idx.wrapping_neg();
        }
    }

    fn prefix_sum(&self, mut idx: usize) -> i64 {
        idx += 1;
        let mut sum = 0i64;
        while idx > 0 {
            sum += self.tree[idx];
            idx -= idx & idx.wrapping_neg();
        }
        sum
    }

    fn total(&self) -> i64 {
        self.prefix_sum(self.tree.len().saturating_sub(2))
    }

    fn find_prefix(&self, mut target: i64) -> usize {
        let mut idx = 0usize;
        let mut bit = (self.tree.len() - 1).next_power_of_two() / 2;
        while bit > 0 {
            let next = idx + bit;
            if next < self.tree.len() && self.tree[next] <= target {
                target -= self.tree[next];
                idx = next;
            }
            bit /= 2;
        }
        idx
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

    pub fn y_of_row(&self, row: usize) -> i32 {
        if row == 0 {
            0
        } else {
            self.heights.prefix_sum(row - 1) as i32
        }
    }

    pub fn row_at_y(&self, y: i32) -> usize {
        if self.rows.is_empty() {
            return 0;
        }
        let target = y.max(0) as i64;
        self.heights.find_prefix(target).min(self.rows.len() - 1)
    }

    pub fn invalidate_all(&mut self) {
        for flag in &mut self.measured {
            *flag = false;
        }
        self.heights = Fenwick::new(self.rows.len());
        for i in 0..self.rows.len() {
            self.heights.add(i, self.estimate_height as i64);
        }
    }

    pub fn visible_range(&self, viewport_h: i32, overscan: i32) -> Range<usize> {
        if self.rows.is_empty() {
            return 0..0;
        }
        let top = self.scroll_top_y();
        let bottom = top + viewport_h;
        let start = self.row_at_y(top - overscan).saturating_sub(0);
        let mut end = self.row_at_y(bottom + overscan) + 1;
        end = end.min(self.rows.len());
        start..end
    }

    fn scroll_top_y(&self) -> i32 {
        if self.scroll.pinned_to_bottom {
            let total = self.heights.total() as i32;
            return total.saturating_sub(1);
        }
        self.y_of_row(self.scroll.row) + self.scroll.offset_px
    }

    pub fn append(&mut self, row: Row) {
        self.rows.push(row);
        self.measured.push(false);
        self.heights.add(self.rows.len() - 1, self.estimate_height as i64);
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

    pub fn select_all(&mut self) {
        if self.rows.is_empty() {
            self.selection = None;
            return;
        }
        let last = self.rows.len() - 1;
        self.selection = Some(Selection {
            anchor: Pos { row: 0, offset: 0 },
            focus: Pos {
                row: last,
                offset: plain_len(&self.rows[last]),
            },
        });
    }

    pub fn clear(&mut self) {
        self.selection = None;
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
                RowKind::Message => format!(
                    "[{}] <{}> {}",
                    self.rows[row].time.as_deref().unwrap_or("--:--"),
                    self.rows[row].author.as_deref().unwrap_or(""),
                    selected
                ),
                RowKind::Event => format!("* {selected}"),
                RowKind::DaySeparator => selected,
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

fn snap_grapheme(text: &str, offset: usize) -> usize {
    if offset == 0 || text.is_empty() {
        return 0;
    }
    if offset >= text.len() {
        return text.len();
    }
    let mut cursor = GraphemeCursor::new(offset, text.len(), true);
    if cursor.is_boundary(text, 0).unwrap_or(false) {
        return offset;
    }
    cursor.prev_boundary(text, 0).ok().flatten().unwrap_or(0)
}

fn word_bounds(text: &str, offset: usize) -> (usize, usize) {
    let offset = snap_grapheme(text, offset.min(text.len()));
    if text.is_empty() {
        return (0, 0);
    }
    let mut start = offset;
    while start > 0 {
        let prev = snap_grapheme(text, start - 1);
        if prev == start {
            break;
        }
        let ch = text[prev..start].chars().next().unwrap();
        if is_word_char(ch) {
            start = prev;
        } else {
            break;
        }
    }
    let mut end = offset;
    while end < text.len() {
        let next = next_grapheme_end(text, end);
        let ch = text[end..next].chars().next().unwrap_or(' ');
        if is_word_char(ch) {
            end = next;
        } else {
            break;
        }
    }
    (start, end)
}

fn next_grapheme_end(text: &str, offset: usize) -> usize {
    let mut cursor = GraphemeCursor::new(offset, text.len(), true);
    cursor
        .next_boundary(text, 0)
        .ok()
        .flatten()
        .unwrap_or(text.len())
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-'
}

fn plain_len(row: &Row) -> usize {
    row_plain(row).len()
}

fn row_plain(row: &Row) -> &str {
    match row.kind {
        RowKind::DaySeparator => row
            .separator_text
            .as_deref()
            .or(Some(row.body.plain.as_str()))
            .unwrap(),
        RowKind::Event => row
            .event_text
            .as_deref()
            .or(Some(row.body.plain.as_str()))
            .unwrap(),
        RowKind::Message => &row.body.plain,
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
        separator_text: None,
        event_text: None,
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

    #[test]
    fn fenwick_prefix_and_update() {
        let mut t = Transcript::new(20);
        t.set_rows(sample_rows());
        t.set_measured(0, 30);
        assert_eq!(t.y_of_row(1), 30);
        t.set_measured(1, 40);
        assert_eq!(t.heights.total(), 70);
    }

    #[test]
    fn row_at_y_boundaries() {
        let mut t = Transcript::new(10);
        t.set_rows(sample_rows());
        t.set_measured(0, 20);
        t.set_measured(1, 30);
        assert_eq!(t.row_at_y(0), 0);
        assert_eq!(t.row_at_y(19), 0);
        assert_eq!(t.row_at_y(20), 1);
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
}
