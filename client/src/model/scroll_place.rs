//! Where a conversation's transcript was left, so returning to it opens at the
//! same line (Qt `IrcScrollPlaceStore`, `rememberOpenTranscript` and
//! `restoreRememberedTranscript`).
//! Qt keys the place on the first visible message's msgid when the server gave
//! one, else on its kind, author, body and time. The client keeps the anchor row
//! and pixel offset from `Transcript::scroll()` in memory while a conversation
//! is open, and persists through the same JSON shape Qt uses.
//!
//! Unread marks and day separators are not places: capture skips them for the first
//! message/event/status/whois row at or below the top of the view. A divider that was on
//! top stays there only while it still sits above that line, so a mark that moves does not
//! take the reader with it. A live refresh and persistence share this one place.

use super::transcript::{row_for_msgid, Row, RowKind, ScrollAnchor};
use crate::storage::scrollplace::ScrollPlace as StoredPlace;

/// What a transcript looked like when it was left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SavedPlace {
    /// The reader was at the end. Returning follows the end again.
    End,
    /// The first line at or below the top of the view, the dividers on top above it, and how
    /// far the top row was scrolled past. Dividers and pixels are not kept on disk.
    Line {
        row: usize,
        offset_px: i32,
        line: LineKey,
        lead: Vec<DividerKey>,
    },
}

/// A divider on top of the view above the reader's line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DividerKey {
    kind: RowKind,
    text: String,
}

impl DividerKey {
    fn of(row: &Row) -> Self {
        Self {
            kind: row.kind,
            text: row.body.plain.clone(),
        }
    }
}

/// Row kinds that can be stored and restored. Dividers (`UnreadMark`,
/// `DaySeparator`) are excluded so `kind:unread` / `kind:day` never land on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestorableKind {
    Message,
    Event,
    Status,
    Whois,
}

impl RestorableKind {
    fn from_row(kind: RowKind) -> Option<Self> {
        Some(match kind {
            RowKind::Message => Self::Message,
            RowKind::Event => Self::Event,
            RowKind::Status => Self::Status,
            RowKind::Whois => Self::Whois,
            RowKind::DaySeparator | RowKind::UnreadMark => return None,
        })
    }

    fn matches(self, kind: RowKind) -> bool {
        Self::from_row(kind) == Some(self)
    }

    fn name(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Event => "event",
            Self::Status => "status",
            Self::Whois => "whois",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "message" => Self::Message,
            "event" => Self::Event,
            "status" => Self::Status,
            "whois" => Self::Whois,
            _ => return None,
        })
    }
}

/// The content of a displayed line. It tells a saved line apart from the row now
/// at the same index, so a place can be found again after rows move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineKey {
    kind: RestorableKind,
    author: Option<String>,
    time: Option<String>,
    text: String,
    msgid: String,
}

impl LineKey {
    fn of(row: &Row) -> Option<Self> {
        Some(Self {
            kind: RestorableKind::from_row(row.kind)?,
            author: row.author.clone(),
            time: row.time.clone(),
            text: row.body.plain.clone(),
            msgid: row.msgid.clone(),
        })
    }

    fn is_row(&self, row: &Row) -> bool {
        if !self.msgid.is_empty() {
            return row.msgid == self.msgid;
        }
        self.kind.matches(row.kind)
            && self.author == row.author
            && self.time == row.time
            && self.text == row.body.plain
    }
}

impl SavedPlace {
    /// The place for a transcript whose scroll is `anchor` and whose rows are
    /// `rows`. A pinned anchor, or an empty transcript, is the end. Capture skips
    /// unread marks and day separators, anchoring on the first restorable row at
    /// or below the top of the view (Qt's first visible message).
    pub fn from_anchor(anchor: ScrollAnchor, rows: &[Row]) -> Self {
        if anchor.pinned_to_bottom || rows.is_empty() {
            return Self::End;
        }
        let start = anchor.row.min(rows.len() - 1);
        let Some((rel, row_ix, line)) = rows[start..].iter().enumerate().find_map(|(rel, row)| {
            LineKey::of(row).map(|line| (rel, start + rel, line))
        }) else {
            return Self::End;
        };
        Self::Line {
            row: row_ix,
            offset_px: anchor.offset_px,
            line,
            lead: rows[start..start + rel]
                .iter()
                .map(DividerKey::of)
                .collect(),
        }
    }
}

/// Where a restore lands in the rows on screen now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resume {
    End,
    Line { row: usize, offset_px: i32 },
}

/// The row a saved line sits on in `rows`: its msgid row, else its old row while that
/// still holds it, else the first row with the same content. `None` when it is gone.
fn find_line(line: &LineKey, row: usize, rows: &[Row]) -> Option<usize> {
    if !line.msgid.is_empty() {
        return row_for_msgid(rows, &line.msgid);
    }
    if rows.get(row).is_some_and(|candidate| line.is_row(candidate)) {
        return Some(row);
    }
    rows.iter().position(|candidate| line.is_row(candidate))
}

/// Whether the saved line is among `rows` now. A restore that waits for history asks
/// this before it lands. [`resume`] clamps a line that is gone, so it cannot tell.
pub fn has_line(place: &SavedPlace, rows: &[Row]) -> bool {
    match place {
        SavedPlace::Line { row, line, .. } => find_line(line, *row, rows).is_some(),
        SavedPlace::End => false,
    }
}

/// Resolves a saved place against the rows now shown. A saved line is found at
/// its old index when it is still there, else at the first row with the same
/// content, else at its old index clamped to the rows that remain. Its dividers go
/// back on top only when exactly those rows still sit above it. Otherwise the line
/// is on top. No place resumes at the end: the caller decides what a conversation
/// with no place opens at.
pub fn resume(place: Option<&SavedPlace>, rows: &[Row]) -> Resume {
    let Some(SavedPlace::Line {
        row,
        offset_px,
        line,
        lead,
    }) = place
    else {
        return Resume::End;
    };
    if rows.is_empty() {
        return Resume::End;
    }
    let Some(found) = find_line(line, *row, rows) else {
        return Resume::Line {
            row: (*row).min(rows.len() - 1),
            offset_px: *offset_px,
        };
    };
    let top = found.checked_sub(lead.len()).filter(|&top| {
        rows[top..found]
            .iter()
            .map(DividerKey::of)
            .eq(lead.iter().cloned())
    });
    match top {
        Some(top) => Resume::Line {
            row: top,
            offset_px: *offset_px,
        },
        None => Resume::Line {
            row: found,
            offset_px: 0,
        },
    }
}

impl SavedPlace {
    /// The place as the settings file keeps it. A chat line with a msgid is stored by
    /// that id alone, as Qt does. Other lines fall back to content. Pixel offset is not
    /// kept on disk.
    pub fn to_stored(&self) -> StoredPlace {
        match self {
            SavedPlace::End => StoredPlace {
                follow_end: true,
                ..StoredPlace::default()
            },
            SavedPlace::Line { line, .. } if !line.msgid.is_empty() => StoredPlace {
                msg_id: line.msgid.clone(),
                ..StoredPlace::default()
            },
            SavedPlace::Line { line, .. } => StoredPlace {
                kind: line.kind.name().to_owned(),
                author: line.author.clone().unwrap_or_default(),
                body: line.text.clone(),
                shown_time: line.time.clone().unwrap_or_default(),
                ..StoredPlace::default()
            },
        }
    }

    /// The place a stored one describes. A line whose kind this build does not draw is not
    /// restored, and the conversation opens where it would with no place.
    pub fn from_stored(place: &StoredPlace) -> Option<Self> {
        if place.follow_end {
            return Some(SavedPlace::End);
        }
        if !place.msg_id.is_empty() {
            return Some(SavedPlace::Line {
                row: 0,
                offset_px: 0,
                line: LineKey {
                    kind: RestorableKind::Message,
                    author: None,
                    time: None,
                    text: String::new(),
                    msgid: place.msg_id.clone(),
                },
                lead: Vec::new(),
            });
        }
        let kind = RestorableKind::from_name(&place.kind)?;
        Some(SavedPlace::Line {
            row: 0,
            offset_px: 0,
            line: LineKey {
                kind,
                author: (!place.author.is_empty()).then(|| place.author.clone()),
                time: (!place.shown_time.is_empty()).then(|| place.shown_time.clone()),
                text: place.body.clone(),
                msgid: String::new(),
            },
            lead: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::world::Origin;
    use crate::model::format::format_irc;
    use crate::model::transcript::row_from_message;

    fn rows(count: u64) -> Vec<Row> {
        (1..=count)
            .map(|n| row_from_message(n, "anna", "09:00", &format!("line {n}"), false))
            .collect()
    }

    fn row_with_msgid(id: u64, msgid: &str, author: &str, time: &str, body: &str, continues: bool) -> Row {
        let mut row = row_from_message(id, author, time, body, continues);
        row.msgid = msgid.to_owned();
        row
    }

    fn divider(id: u64, kind: RowKind, text: &str) -> Row {
        Row {
            id,
            kind,
            author: None,
            time: None,
            body: format_irc(text),
            continues: false,
            action: false,
            separator_text: (kind == RowKind::DaySeparator).then(|| text.to_owned()),
            event_text: None,
            status: None,
            msgid: String::new(),
            only_visible_to_you: false,
            mentioned: false,
            origin: Origin::Live,
        }
    }

    fn anchor(row: usize, offset_px: i32) -> ScrollAnchor {
        ScrollAnchor {
            row,
            offset_px,
            pinned_to_bottom: false,
        }
    }

    #[test]
    fn a_pinned_anchor_is_the_end() {
        let rows = rows(5);
        let pinned = ScrollAnchor {
            row: 4,
            offset_px: 0,
            pinned_to_bottom: true,
        };
        assert_eq!(SavedPlace::from_anchor(pinned, &rows), SavedPlace::End);
    }

    #[test]
    fn an_unpinned_anchor_keeps_its_row_offset_and_line() {
        let rows = rows(5);
        let place = SavedPlace::from_anchor(anchor(2, 17), &rows);
        assert_eq!(
            place,
            SavedPlace::Line {
                row: 2,
                offset_px: 17,
                line: LineKey::of(&rows[2]).expect("message is restorable"),
                lead: Vec::new(),
            }
        );
    }

    #[test]
    fn an_empty_transcript_has_no_line_to_keep() {
        assert_eq!(SavedPlace::from_anchor(anchor(0, 0), &[]), SavedPlace::End);
    }

    #[test]
    fn an_unread_mark_at_the_top_saves_the_message_below() {
        let rows = vec![
            divider(1, RowKind::UnreadMark, "New messages"),
            row_with_msgid(2, "below-unread", "anna", "09:00", "first after mark", false),
            row_from_message(3, "anna", "09:01", "later", false),
        ];
        let place = SavedPlace::from_anchor(anchor(0, 3), &rows);
        assert_eq!(
            place,
            SavedPlace::Line {
                row: 1,
                offset_px: 3,
                line: LineKey::of(&rows[1]).expect("message"),
                lead: vec![DividerKey::of(&rows[0])],
            }
        );
        assert_eq!(place.to_stored().msg_id, "below-unread");
        assert_ne!(place.to_stored().kind, "unread");
    }

    #[test]
    fn a_day_separator_at_the_top_saves_the_message_below() {
        let rows = vec![
            divider(1, RowKind::DaySeparator, "Yesterday"),
            row_with_msgid(2, "below-day", "mira", "08:00", "first of day", false),
        ];
        let place = SavedPlace::from_anchor(anchor(0, 8), &rows);
        assert_eq!(
            place,
            SavedPlace::Line {
                row: 1,
                offset_px: 8,
                line: LineKey::of(&rows[1]).expect("message"),
                lead: vec![DividerKey::of(&rows[0])],
            }
        );
        assert_eq!(place.to_stored().msg_id, "below-day");
        assert_ne!(place.to_stored().kind, "day");
    }

    #[test]
    fn only_dividers_visible_saves_the_end() {
        let rows = vec![
            divider(1, RowKind::DaySeparator, "Yesterday"),
            divider(2, RowKind::UnreadMark, "New messages"),
        ];
        assert_eq!(SavedPlace::from_anchor(anchor(0, 0), &rows), SavedPlace::End);
        assert!(SavedPlace::from_anchor(anchor(0, 0), &rows).to_stored().follow_end);
    }

    #[test]
    fn a_divider_top_stays_only_while_it_sits_above_its_line() {
        let mark = || divider(9, RowKind::UnreadMark, "New messages");
        let day = || divider(8, RowKind::DaySeparator, "Yesterday");
        let lines = rows(3);
        let before = vec![mark(), lines[0].clone(), lines[1].clone()];
        let place = SavedPlace::from_anchor(anchor(0, 4), &before);
        assert_eq!(
            resume(Some(&place), &before),
            Resume::Line { row: 0, offset_px: 4 },
            "the mark still above its line stays on top"
        );
        let moved = vec![lines[0].clone(), mark(), lines[1].clone()];
        assert_eq!(
            resume(Some(&place), &moved),
            Resume::Line { row: 0, offset_px: 0 },
            "a moved mark leaves the reader on the line that was under it"
        );
        assert_eq!(
            resume(Some(&place), &lines),
            Resume::Line { row: 0, offset_px: 0 },
            "a removed mark leaves the reader on the same line"
        );
        let under_a_day = vec![day(), lines[0].clone(), mark(), lines[1].clone()];
        assert_eq!(
            resume(Some(&place), &under_a_day),
            Resume::Line { row: 1, offset_px: 0 },
            "another divider above the line does not stand in for the mark"
        );
        let prepended = vec![lines[2].clone(), mark(), lines[0].clone(), lines[1].clone()];
        assert_eq!(
            resume(Some(&place), &prepended),
            Resume::Line { row: 1, offset_px: 4 },
            "rows above move the divider and its line together"
        );
        let on_a_day = vec![day(), lines[0].clone(), lines[1].clone()];
        let place = SavedPlace::from_anchor(anchor(0, 2), &on_a_day);
        let older_day = vec![lines[2].clone(), day(), lines[0].clone(), lines[1].clone()];
        assert_eq!(
            resume(Some(&place), &older_day),
            Resume::Line { row: 1, offset_px: 2 },
            "a day separator still above its line stays on top"
        );
        let same_day = vec![day(), lines[2].clone(), lines[0].clone(), lines[1].clone()];
        assert_eq!(
            resume(Some(&place), &same_day),
            Resume::Line { row: 2, offset_px: 0 },
            "lines spliced under a day separator leave the reader on the same line"
        );
    }

    #[test]
    fn a_missing_place_resumes_at_the_end() {
        assert_eq!(resume(None, &rows(3)), Resume::End);
        assert_eq!(resume(Some(&SavedPlace::End), &rows(3)), Resume::End);
    }

    #[test]
    fn a_place_resumes_at_its_row_when_nothing_moved() {
        let rows = rows(5);
        let place = SavedPlace::from_anchor(anchor(3, 9), &rows);
        assert_eq!(
            resume(Some(&place), &rows),
            Resume::Line {
                row: 3,
                offset_px: 9
            }
        );
    }

    #[test]
    fn a_place_follows_its_line_when_rows_shift_above_it() {
        let rows = rows(5);
        let place = SavedPlace::from_anchor(anchor(3, 9), &rows);
        // A new line lands above, so the saved line is now row 4.
        let mut shifted = vec![row_from_message(99, "mira", "08:00", "new", false)];
        shifted.extend(rows.iter().cloned());
        assert_eq!(
            resume(Some(&place), &shifted),
            Resume::Line {
                row: 4,
                offset_px: 9
            }
        );
    }

    #[test]
    fn a_gone_line_is_not_there_and_resumes_at_its_old_row() {
        let rows = rows(5);
        let place = SavedPlace::from_anchor(anchor(3, 9), &rows);
        let fewer = rows[..2].to_vec();
        assert!(!has_line(&place, &fewer));
        assert_eq!(
            resume(Some(&place), &fewer),
            Resume::Line {
                row: 1,
                offset_px: 9
            }
        );
    }

    #[test]
    fn an_emptied_transcript_has_no_line_and_resumes_at_the_end() {
        let rows = rows(5);
        let place = SavedPlace::from_anchor(anchor(3, 9), &rows);
        assert!(!has_line(&place, &[]));
        assert_eq!(resume(Some(&place), &[]), Resume::End);
    }

    #[test]
    fn a_msgid_line_is_there_only_once_its_row_arrives() {
        let place = SavedPlace::from_stored(&StoredPlace {
            msg_id: "history-line".to_owned(),
            ..StoredPlace::default()
        })
        .expect("msgid place");
        assert!(!has_line(&place, &rows(3)));
        let arrived = vec![row_with_msgid(1, "history-line", "anna", "09:00", "hello", false)];
        assert!(has_line(&place, &arrived));
        assert_eq!(
            resume(Some(&place), &arrived),
            Resume::Line {
                row: 0,
                offset_px: 0
            }
        );
    }

    #[test]
    fn a_line_place_round_trips_by_its_content() {
        let rows = rows(4);
        let place = SavedPlace::from_anchor(anchor(2, 5), &rows);
        let stored = place.to_stored();
        assert_eq!(stored.kind, "message");
        let restored = SavedPlace::from_stored(&stored).expect("a message restores");
        // The restored line has no row of its own, so it is found by its content.
        assert_eq!(resume(Some(&restored), &rows), Resume::Line { row: 2, offset_px: 0 });
    }

    #[test]
    fn a_msgid_anchor_restores_the_same_message_when_two_lines_share_a_minute() {
        let rows = vec![
            row_with_msgid(1, "m1", "anna", "09:00", "same text", false),
            row_with_msgid(2, "m2", "anna", "09:00", "same text", true),
        ];
        let place = SavedPlace::from_anchor(anchor(1, 4), &rows);
        let restored = SavedPlace::from_stored(&place.to_stored()).expect("msgid restores");
        assert_eq!(
            resume(Some(&restored), &rows),
            Resume::Line {
                row: 1,
                offset_px: 0
            },
            "content alone would land on row 0"
        );
    }

    #[test]
    fn the_gui_stored_place_carries_msgid_not_text_only() {
        let rows = vec![row_with_msgid(1, "mid-9", "anna", "09:00", "hello", false)];
        let stored = SavedPlace::from_anchor(anchor(0, 0), &rows).to_stored();
        assert_eq!(stored.msg_id, "mid-9");
        assert!(stored.body.is_empty() && stored.author.is_empty());
    }

    #[test]
    fn the_end_round_trips_as_follow_end() {
        let stored = SavedPlace::End.to_stored();
        assert!(stored.follow_end);
        assert_eq!(SavedPlace::from_stored(&stored), Some(SavedPlace::End));
    }

    #[test]
    fn an_unknown_kind_is_not_restored() {
        let stored = StoredPlace {
            kind: "day".to_owned(),
            ..StoredPlace::default()
        };
        assert_eq!(SavedPlace::from_stored(&stored), None);
        let unread = StoredPlace {
            kind: "unread".to_owned(),
            ..StoredPlace::default()
        };
        assert_eq!(SavedPlace::from_stored(&unread), None);
    }
}
