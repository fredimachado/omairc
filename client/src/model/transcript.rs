use super::format::{format_irc, FormattedText};
use super::status::StatusMark;
use super::text_edit::{snap_grapheme, word_bounds};
use crate::desktop::locale::short_date;
use crate::irc::reducer::zero_time;
use crate::irc::world::{Message, MessageKind, Origin};
use chrono::{DateTime, Local, NaiveDate, Utc};
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
    /// The IRCv3 `msgid` for a chat line, empty elsewhere.
    pub msgid: String,
    /// Channel-context delivery: Qt's `onlyVisibleToYou` caption under the body.
    pub only_visible_to_you: bool,
    /// Whether the reducer classified this chat line as a transcript highlight
    /// (nick or `/highlight` word hit from someone else).
    pub mentioned: bool,
    /// Live or replay (`client/src/ui/message_row.rs` `origin` / `replayed`).
    pub origin: Origin,
}

/// Whether a transcript row draws Qt's "only visible to you" caption.
pub fn shows_only_visible_to_you_caption(row: &Row) -> bool {
    row.kind == RowKind::Message && row.only_visible_to_you
}

/// Qt mutes the author label when `origin === "replay"`.
pub fn chat_author_uses_muted_ink(row: &Row) -> bool {
    row.kind == RowKind::Message && row.origin == Origin::Replay
}

/// Qt mutes the body on replay and on `/me` lines.
pub fn chat_body_uses_muted_ink(row: &Row) -> bool {
    row.kind == RowKind::Message && (row.origin == Origin::Replay || row.action)
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
        Self {
            tree: vec![0; n + 1],
        }
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

    /// Replaces the rows with a conversation's lines and its "New messages" mark.
    /// `today` is the local calendar day that shapes Today/Yesterday captions.
    pub fn set_conversation(&mut self, messages: &[Message], mark: Option<i64>, today: NaiveDate) {
        self.set_rows(rows_from_messages_on(messages, mark, today));
    }

    /// Replaces a conversation's rows while keeping scroll and selection when they
    /// still apply. Used when the same buffer grows on the wire.
    /// `today` is the local calendar day that shapes Today/Yesterday captions.
    pub fn replace_conversation(
        &mut self,
        messages: &[Message],
        mark: Option<i64>,
        today: NaiveDate,
    ) {
        let scroll = self.scroll;
        let selection = self.selection;
        let rows = rows_from_messages_on(messages, mark, today);
        self.rows = rows;
        self.measured = vec![false; self.rows.len()];
        self.heights = Fenwick::new(self.rows.len());
        for i in 0..self.rows.len() {
            self.heights.add(i, self.estimate_height as i64);
        }
        self.selection = selection
            .filter(|sel| sel.anchor.row < self.rows.len() && sel.focus.row < self.rows.len())
            .map(|sel| Selection {
                anchor: snap_pos(&self.rows, sel.anchor),
                focus: snap_pos(&self.rows, sel.focus),
            });
        self.scroll = if self.rows.is_empty() {
            ScrollAnchor {
                row: 0,
                offset_px: 0,
                pinned_to_bottom: true,
            }
        } else if scroll.pinned_to_bottom {
            ScrollAnchor {
                row: self.rows.len() - 1,
                offset_px: 0,
                pinned_to_bottom: true,
            }
        } else {
            ScrollAnchor {
                row: scroll.row.min(self.rows.len() - 1),
                offset_px: scroll.offset_px,
                pinned_to_bottom: false,
            }
        };
    }

    /// Appends one row, as the test fixtures build a transcript line by line.
    #[cfg(test)]
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
        RowKind::Event => row.event_text.as_deref().unwrap_or(row.body.plain.as_str()),
        RowKind::Message | RowKind::Status | RowKind::Whois | RowKind::UnreadMark => {
            &row.body.plain
        }
    }
}

/// The local `HH:MM` a line shows, from its display instant.
pub fn minute_of(time: DateTime<Utc>) -> String {
    time.with_timezone(&Local).format("%H:%M").to_string()
}

/// The local date a line falls on. Two lines on different dates split a day separator.
fn day_of(time: DateTime<Utc>) -> NaiveDate {
    time.with_timezone(&Local).date_naive()
}

/// The caption of a day separator: `Today`, `Yesterday`, or the
/// locale's short date for any other day.
pub fn day_caption(day: NaiveDate, today: NaiveDate) -> String {
    if day == today {
        return "Today".to_owned();
    }
    if today.pred_opt() == Some(day) {
        return "Yesterday".to_owned();
    }
    short_date(day)
}

/// The day a line belongs to. A line with a display time takes that
/// time's local date. A live line without one takes today. Any other untimed line, such as
/// a replayed one, takes the day of the line above it.
fn assigned_day(
    message: &Message,
    previous: Option<NaiveDate>,
    today: NaiveDate,
) -> Option<NaiveDate> {
    if message.time != zero_time() {
        return Some(day_of(message.time));
    }
    if message.origin == Origin::Live {
        return Some(today);
    }
    previous
}

/// The rows for one conversation's lines, oldest first.
/// `mark` is the sequence of the "New messages" boundary (`ConversationState::unread_mark`).
/// The boundary row sits in front of that line, but only when a row is above it, as
/// Qt's `!view.empty` rule has it. A day separator sits in front of a line whose day
/// differs from the line above, and in front of the first dated line when that day is not
/// today, so a backlog still says which day it is.
pub fn rows_from_messages(messages: &[Message], mark: Option<i64>) -> Vec<Row> {
    rows_from_messages_on(messages, mark, Local::now().date_naive())
}

/// `rows_from_messages` with `today` given, so a test can fix the day as Qt's
/// `setTranscriptTodayForTest` does.
/// The row index of a chat line with `msgid`, if the transcript holds one.
pub fn row_for_msgid(rows: &[Row], msgid: &str) -> Option<usize> {
    if msgid.is_empty() {
        return None;
    }
    rows.iter().position(|row| row.msgid == msgid)
}

pub fn rows_from_messages_on(
    messages: &[Message],
    mark: Option<i64>,
    today: NaiveDate,
) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::with_capacity(messages.len());
    let mut previous: Option<NaiveDate> = None;
    for message in messages {
        let date = assigned_day(message, previous, today);
        let day_change = matches!((date, previous), (Some(day), Some(prior)) if day != prior);
        let leading_backlog = previous.is_none() && date.is_some_and(|day| day != today);
        if let Some(day) = date {
            if day_change || leading_backlog {
                let id = rows.len() as u64 + 1;
                rows.push(day_separator_row(id, day, today));
            }
            previous = Some(day);
        }
        if mark == Some(message.sequence) && !rows.is_empty() {
            let id = rows.len() as u64 + 1;
            rows.push(unread_mark_row_for(id));
        }
        let id = rows.len() as u64 + 1;
        let row = row_for_message(id, message, rows.last());
        rows.push(row);
    }
    rows
}

/// The row for one line, numbered `id`. `previous` is the row above it, which decides
/// whether a chat line folds under it (`continues_group`). Used for the stored lines and
/// for a line appended while the conversation is open.
pub fn row_for_message(id: u64, message: &Message, previous: Option<&Row>) -> Row {
    match message.kind {
        MessageKind::Message | MessageKind::Notice | MessageKind::Action => {
            let minute = minute_of(message.time);
            let continues = continues_group(previous, &message.author, &minute, message.origin);
            row_from_chat_line(id, message, &minute, continues)
        }
        MessageKind::Whois => row_from_whois(id, &message.body),
        MessageKind::Event | MessageKind::Error => row_from_event(id, &message.body),
    }
}

fn day_separator_row(id: u64, day: NaiveDate, today: NaiveDate) -> Row {
    let text = day_caption(day, today);
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
        msgid: String::new(),
        only_visible_to_you: false,
        mentioned: false,
        origin: Origin::Live,
    }
}

fn unread_mark_row_for(id: u64) -> Row {
    Row {
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
        msgid: String::new(),
        only_visible_to_you: false,
        mentioned: false,
        origin: Origin::Live,
    }
}

/// Qt folds a chat line under the one above when both are chat lines from the same
/// author in the same minute and origin (`continuesMessageGroup`).
pub fn continues_group(previous: Option<&Row>, author: &str, time: &str, origin: Origin) -> bool {
    if author.is_empty() || time.is_empty() {
        return false;
    }
    previous.is_some_and(|prev| {
        prev.kind == RowKind::Message
            && prev.origin == origin
            && prev.author.as_deref() == Some(author)
            && prev.time.as_deref() == Some(time)
    })
}

/// A chat, notice or `/me` line. `minute` is its local `HH:MM`.
fn row_from_chat_line(id: u64, message: &Message, minute: &str, continues: bool) -> Row {
    Row {
        id,
        kind: RowKind::Message,
        author: Some(message.author.clone()),
        time: Some(minute.to_owned()),
        body: format_irc(&message.body),
        continues,
        action: message.kind == MessageKind::Action,
        separator_text: None,
        event_text: None,
        status: None,
        msgid: message.msgid.value.clone(),
        only_visible_to_you: message.only_visible_to_you,
        mentioned: message.mentioned,
        origin: message.origin,
    }
}

/// A join, part, quit, mode or error line. Its caption is the stored body.
fn row_from_event(id: u64, caption: &str) -> Row {
    Row {
        id,
        kind: RowKind::Event,
        author: None,
        time: None,
        body: format_irc(caption),
        continues: false,
        action: false,
        separator_text: None,
        event_text: Some(caption.to_owned()),
        status: None,
        msgid: String::new(),
        only_visible_to_you: false,
        mentioned: false,
        origin: Origin::Live,
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
        msgid: String::new(),
        only_visible_to_you: false,
        mentioned: false,
        origin: Origin::Live,
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
        msgid: String::new(),
        only_visible_to_you: false,
        mentioned: false,
        origin: Origin::Live,
    }
}

#[cfg(test)]
mod tests {
    use crate::irc::event::MsgId;
    use crate::irc::world::{Message, MessageKind, Origin};
    use chrono::{Local, TimeZone};

    /// A local wall time on 19 October 2026, as a UTC instant. The transcript shows its
    /// `HH:MM` back in the local zone, so the minute reads the same in any zone.
    fn local(hour: u32, minute: u32) -> chrono::DateTime<chrono::Utc> {
        local_on(19, hour, minute)
    }

    /// A local wall time on the given day of October 2026, as a UTC instant.
    fn local_on(day: u32, hour: u32, minute: u32) -> chrono::DateTime<chrono::Utc> {
        Local
            .with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    /// The fixed "today" the day-separator tests pass, as Qt's test hook does.
    fn today() -> NaiveDate {
        october(22)
    }

    fn october(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    /// A line replayed from history rather than received live.
    fn replayed(mut message: Message) -> Message {
        message.origin = Origin::Replay;
        message
    }

    /// One transcript line of the given kind.
    fn line(
        kind: MessageKind,
        author: &str,
        body: &str,
        time: chrono::DateTime<chrono::Utc>,
        sequence: i64,
    ) -> Message {
        Message {
            kind,
            author: author.to_string(),
            body: body.to_string(),
            time,
            server_time: None,
            msgid: MsgId::default(),
            origin: Origin::Live,
            mentioned: false,
            collapsible: false,
            sequence,
            only_visible_to_you: false,
        }
    }

    fn chat(author: &str, body: &str, hour: u32, minute: u32, sequence: i64) -> Message {
        line(
            MessageKind::Message,
            author,
            body,
            local(hour, minute),
            sequence,
        )
    }

    /// The row kinds of a transcript, without its day separators.
    fn kinds(rows: &[Row]) -> Vec<RowKind> {
        rows.iter()
            .map(|row| row.kind)
            .filter(|kind| *kind != RowKind::DaySeparator)
            .collect()
    }
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
        let messages = [
            chat("anna", "hello", 9, 41, 1),
            line(
                MessageKind::Whois,
                "",
                "VERSION reply from anna: https://omairc.app",
                local(0, 0),
                2,
            ),
            chat("anna", "again", 9, 41, 3),
        ];
        let rows = rows_from_messages(&messages, None);
        assert_eq!(
            kinds(&rows),
            [RowKind::Message, RowKind::Whois, RowKind::Message]
        );
        assert_eq!(rows.last().map(|row| row.continues), Some(false));
    }

    #[test]
    fn an_appended_line_folds_under_the_row_above_like_a_stored_one() {
        let messages = [chat("anna", "first", 9, 41, 1)];
        let rows = rows_from_messages(&messages, None);
        let previous = rows.last().cloned();
        let folded = row_for_message(9, &chat("anna", "second", 9, 41, 2), previous.as_ref());
        assert!(folded.continues);
        let apart = row_for_message(10, &chat("anna", "third", 9, 42, 3), previous.as_ref());
        assert!(!apart.continues);
    }

    #[test]
    fn a_selection_over_the_unread_mark_copies_the_lines_only() {
        let messages = [chat("anna", "one", 9, 41, 1), chat("anna", "two", 9, 43, 2)];
        let rows = rows_from_messages(&messages, Some(2));
        assert!(rows.iter().any(|row| row.kind == RowKind::UnreadMark));
        let mut t = Transcript::new(72);
        t.set_rows(rows);
        select_everything(&mut t);
        let copied = t.copy_text();
        assert!(copied.contains("one") && copied.contains("two"));
        assert!(!copied.contains("New messages"));
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
        let messages = [
            chat("fred", "a", 9, 46, 1),
            chat("fred", "b", 9, 46, 2),
            chat("fred", "c", 9, 47, 3),
        ];
        let rows = rows_from_messages(&messages, None);
        let chats: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Message)
            .collect();
        assert_eq!(chats.len(), 3);
        assert!(!chats[0].continues);
        assert!(chats[1].continues, "same author, same minute");
        assert!(!chats[2].continues, "a new minute starts a group");
    }

    #[test]
    fn continues_group_needs_same_author_and_minute() {
        let prev = row_from_message(1, "anna", "09:00", "hi", false);
        assert!(continues_group(Some(&prev), "anna", "09:00", Origin::Live));
        assert!(!continues_group(Some(&prev), "dax", "09:00", Origin::Live));
        assert!(!continues_group(Some(&prev), "anna", "09:01", Origin::Live));
        assert!(!continues_group(None, "anna", "09:00", Origin::Live));
        assert!(!continues_group(Some(&prev), "", "", Origin::Live));
    }

    #[test]
    fn a_replayed_chat_row_is_dimmed() {
        let messages = [replayed(chat("anna", "backlog", 9, 41, 1))];
        let rows = rows_from_messages(&messages, None);
        let row = rows
            .iter()
            .find(|row| row.kind == RowKind::Message)
            .expect("a chat row");
        assert_eq!(row.origin, Origin::Replay);
        assert!(chat_author_uses_muted_ink(row));
        assert!(chat_body_uses_muted_ink(row));
    }

    #[test]
    fn a_live_chat_row_is_not_dimmed_for_replay() {
        let messages = [chat("anna", "live", 9, 41, 1)];
        let rows = rows_from_messages(&messages, None);
        let row = rows
            .iter()
            .find(|row| row.kind == RowKind::Message)
            .expect("a chat row");
        assert_eq!(row.origin, Origin::Live);
        assert!(!chat_author_uses_muted_ink(row));
        assert!(!chat_body_uses_muted_ink(row));
    }

    #[test]
    fn a_live_line_after_a_replay_does_not_continue_the_group() {
        let messages = [
            replayed(chat("anna", "old", 9, 41, 1)),
            chat("anna", "new", 9, 41, 2),
        ];
        let rows = rows_from_messages(&messages, None);
        let chats: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Message)
            .collect();
        assert_eq!(chats.len(), 2);
        assert!(!chats[1].continues);
    }

    #[test]
    fn stored_action_renders_as_action_row() {
        let messages = [line(MessageKind::Action, "fred", "waves", local(21, 57), 1)];
        let rows = rows_from_messages(&messages, None);
        let row = rows.last().expect("an action row");
        assert!(row.action);
        assert_eq!(row.author.as_deref(), Some("fred"));
        assert_eq!(row.time.as_deref(), Some("21:57"));
        assert_eq!(row.body.plain, "waves");
    }

    #[test]
    fn an_untimed_replayed_line_with_no_day_above_starts_no_day() {
        let untimed = replayed(line(
            MessageKind::Event,
            "",
            "nora joined",
            DateTime::<Utc>::MIN_UTC,
            1,
        ));
        let rows = rows_from_messages_on(&[untimed], None, today());
        assert_eq!(kinds(&rows), [RowKind::Event]);
        assert!(rows.iter().all(|row| row.kind != RowKind::DaySeparator));
    }

    #[test]
    fn a_live_untimed_line_with_no_day_above_is_today_and_starts_no_rule() {
        let untimed = line(
            MessageKind::Event,
            "",
            "nora joined",
            DateTime::<Utc>::MIN_UTC,
            1,
        );
        let rows = rows_from_messages_on(&[untimed], None, today());
        assert_eq!(kinds(&rows), [RowKind::Event]);
        assert!(rows.iter().all(|row| row.kind != RowKind::DaySeparator));
    }

    #[test]
    fn rows_from_messages_breaks_on_event_and_day() {
        let next_day = local_on(20, 9, 0);
        let messages = [
            chat("anna", "one", 9, 0, 1),
            line(
                MessageKind::Event,
                "",
                "dax joined #omarchy",
                local(9, 0),
                2,
            ),
            chat("anna", "two", 9, 0, 3),
            line(MessageKind::Message, "anna", "three", next_day, 4),
        ];
        let rows = rows_from_messages_on(&messages, None, today());
        let plain: Vec<RowKind> = rows.iter().map(|row| row.kind).collect();
        assert_eq!(
            plain,
            [
                RowKind::DaySeparator,
                RowKind::Message,
                RowKind::Event,
                RowKind::Message,
                RowKind::DaySeparator,
                RowKind::Message,
            ]
        );
        assert!(!rows[3].continues, "an event row ends the group");
        let short = short_date(october(20));
        assert_eq!(rows[4].separator_text.as_deref(), Some(short.as_str()));
    }

    #[test]
    fn a_leading_backlog_day_is_labelled_when_it_is_not_today() {
        let messages = [
            replayed(line(
                MessageKind::Message,
                "anna",
                "one",
                local_on(19, 9, 41),
                1,
            )),
            replayed(line(
                MessageKind::Message,
                "anna",
                "two",
                local_on(19, 9, 43),
                2,
            )),
        ];
        let rows = rows_from_messages_on(&messages, None, today());
        assert_eq!(rows.len(), 3, "one rule, then both lines of that day");
        assert_eq!(rows[0].kind, RowKind::DaySeparator);
        let backlog = short_date(october(19));
        assert_eq!(rows[0].separator_text.as_deref(), Some(backlog.as_str()));
        assert!(rows[1..].iter().all(|row| row.kind == RowKind::Message));
    }

    #[test]
    fn a_leading_today_gets_no_separator() {
        let messages = [replayed(line(
            MessageKind::Message,
            "anna",
            "hi",
            local_on(22, 9, 41),
            1,
        ))];
        let rows = rows_from_messages_on(&messages, None, today());
        assert_eq!(rows.len(), 1);
        assert_eq!(kinds(&rows), [RowKind::Message]);
    }

    #[test]
    fn a_live_untimed_line_after_a_backlog_day_is_labelled_today() {
        let messages = [
            replayed(line(
                MessageKind::Message,
                "anna",
                "old",
                local_on(19, 9, 41),
                1,
            )),
            line(
                MessageKind::Event,
                "",
                "nora joined",
                DateTime::<Utc>::MIN_UTC,
                2,
            ),
        ];
        let rows = rows_from_messages_on(&messages, None, today());
        let backlog = short_date(october(19));
        let captions: Vec<&str> = rows
            .iter()
            .filter(|row| row.kind == RowKind::DaySeparator)
            .filter_map(|row| row.separator_text.as_deref())
            .collect();
        assert_eq!(captions, [backlog.as_str(), "Today"]);
        assert_eq!(rows.last().map(|row| row.kind), Some(RowKind::Event));
    }

    #[test]
    fn the_day_before_today_is_labelled_yesterday() {
        let messages = [replayed(line(
            MessageKind::Message,
            "anna",
            "late",
            local_on(21, 23, 50),
            1,
        ))];
        let rows = rows_from_messages_on(&messages, None, today());
        assert_eq!(rows[0].separator_text.as_deref(), Some("Yesterday"));
    }

    #[test]
    fn day_caption_names_today_yesterday_and_other_days() {
        assert_eq!(day_caption(october(22), today()), "Today");
        assert_eq!(day_caption(october(21), today()), "Yesterday");
        assert_eq!(day_caption(october(20), today()), short_date(october(20)));
    }

    #[test]
    fn day_separator_captions_update_when_today_advances() {
        let messages = vec![line(
            MessageKind::Message,
            "nick",
            "older",
            local_on(21, 12, 0),
            1,
        )];
        let rows_before = rows_from_messages_on(&messages, None, october(22));
        assert_eq!(
            rows_before[0].separator_text.as_deref(),
            Some("Yesterday")
        );
        let rows_after = rows_from_messages_on(&messages, None, october(23));
        assert_eq!(
            rows_after[0].separator_text.as_deref(),
            Some(short_date(october(21)).as_str())
        );
    }

    #[test]
    fn only_visible_to_you_carried_onto_row() {
        let mut private = chat("nick", "psst", 9, 0, 1);
        private.only_visible_to_you = true;
        let rows = rows_from_messages(&[private, chat("nick", "public", 9, 1, 2)], None);
        let chat_rows: Vec<_> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Message)
            .collect();
        assert!(shows_only_visible_to_you_caption(&chat_rows[0]));
        assert!(!shows_only_visible_to_you_caption(&chat_rows[1]));
        assert!(chat_rows[0].only_visible_to_you);
        assert!(!chat_rows[1].only_visible_to_you);
    }

    #[test]
    fn a_whois_line_takes_its_day_like_any_other_line() {
        let messages = [replayed(line(
            MessageKind::Whois,
            "",
            "alice is a user",
            local_on(19, 9, 0),
            1,
        ))];
        let rows = rows_from_messages_on(&messages, None, today());
        assert_eq!(rows.len(), 2, "a day rule, then the whois line");
        assert_eq!(rows[0].kind, RowKind::DaySeparator);
        assert_eq!(kinds(&rows), [RowKind::Whois]);
    }

    #[test]
    fn set_conversation_replaces_rows_and_clears_selection() {
        let mut t = Transcript::new(72);
        t.set_rows(sample_rows());
        select_everything(&mut t);
        t.set_conversation(&[chat("anna", "fresh", 9, 50, 1)], None, today());
        assert!(t.selection().is_none());
        assert!(t.rows().iter().any(|row| row.body.plain == "fresh"));
        assert!(!t.rows().iter().any(|row| row.body.plain == "hello"));
    }

    #[test]
    fn replace_conversation_keeps_selection_and_scroll() {
        let mut t = Transcript::new(72);
        t.set_conversation(
            &[chat("anna", "one", 9, 50, 1), chat("bob", "two", 9, 51, 2)],
            None,
            today(),
        );
        t.begin(Pos { row: 0, offset: 0 });
        t.set_scroll_anchor(0, 4);
        t.replace_conversation(
            &[
                chat("anna", "one", 9, 50, 1),
                chat("bob", "two", 9, 51, 2),
                chat("carl", "three", 9, 52, 3),
            ],
            None,
            today(),
        );
        assert!(t.selection().is_some());
        assert_eq!(t.scroll().row, 0);
        assert_eq!(t.scroll().offset_px, 4);
    }
}
