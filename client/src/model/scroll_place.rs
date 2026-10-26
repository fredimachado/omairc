//! Where a conversation's transcript was left, so returning to it opens at the
//! same line (Qt `IrcScrollPlaceStore`, `rememberOpenTranscript` and
//! `restoreRememberedTranscript`).
//! Qt keys the place on the first visible message's msgid when the server gave
//! one, else on its kind, author, body and time. The PoC keeps the anchor row
//! and pixel offset from `Transcript::scroll` in memory while a conversation
//! is open, and persists through the same JSON shape Qt uses.

use super::transcript::{row_for_msgid, Row, RowKind, ScrollAnchor};
use crate::storage::scrollplace::ScrollPlace as StoredPlace;

/// What a transcript looked like when it was left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SavedPlace {
    /// The reader was at the end. Returning follows the end again.
    End,
    /// The line at the top of the view, and how far its top was scrolled past.
    Line {
        row: usize,
        offset_px: i32,
        line: LineKey,
    },
}

/// The content of a displayed line. It tells a saved line apart from the row now
/// at the same index, so a place can be found again after rows move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineKey {
    kind: RowKind,
    author: Option<String>,
    time: Option<String>,
    text: String,
    msgid: String,
}

impl LineKey {
    pub fn of(row: &Row) -> Self {
        Self {
            kind: row.kind,
            author: row.author.clone(),
            time: row.time.clone(),
            text: row.body.plain.clone(),
            msgid: row.msgid.clone(),
        }
    }

    fn is_row(&self, row: &Row) -> bool {
        if !self.msgid.is_empty() {
            return row.msgid == self.msgid;
        }
        self.kind == row.kind
            && self.author == row.author
            && self.time == row.time
            && self.text == row.body.plain
    }
}

impl SavedPlace {
    /// The place for a transcript whose scroll is `anchor` and whose rows are
    /// `rows`. A pinned anchor, or an empty transcript, is the end.
    pub fn from_anchor(anchor: ScrollAnchor, rows: &[Row]) -> Self {
        if anchor.pinned_to_bottom || rows.is_empty() {
            return Self::End;
        }
        let row = anchor.row.min(rows.len() - 1);
        Self::Line {
            row,
            offset_px: anchor.offset_px,
            line: LineKey::of(&rows[row]),
        }
    }
}

/// Where a restore lands in the rows on screen now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resume {
    End,
    Line { row: usize, offset_px: i32 },
}

/// Resolves a saved place against the rows now shown. A saved line is found at
/// its old index when it is still there, else at the first row with the same
/// content, else at its old index clamped to the rows that remain. No place
/// resumes at the end: the caller decides what a conversation with no place
/// opens at.
pub fn resume(place: Option<&SavedPlace>, rows: &[Row]) -> Resume {
    let Some(SavedPlace::Line {
        row,
        offset_px,
        line,
    }) = place
    else {
        return Resume::End;
    };
    if rows.is_empty() {
        return Resume::End;
    }
    let found = if !line.msgid.is_empty() {
        row_for_msgid(rows, &line.msgid)
            .unwrap_or_else(|| (*row).min(rows.len() - 1))
    } else if rows.get(*row).is_some_and(|candidate| line.is_row(candidate)) {
        *row
    } else {
        rows.iter()
            .position(|candidate| line.is_row(candidate))
            .unwrap_or_else(|| (*row).min(rows.len() - 1))
    };
    Resume::Line {
        row: found,
        offset_px: *offset_px,
    }
}

/// The Qt name of a row kind, as the saved place's `kind` field holds it.
fn kind_name(kind: RowKind) -> &'static str {
    match kind {
        RowKind::Message => "message",
        RowKind::Event => "event",
        RowKind::DaySeparator => "day",
        RowKind::Status => "status",
        RowKind::Whois => "whois",
        RowKind::UnreadMark => "unread",
    }
}

fn kind_from_name(name: &str) -> Option<RowKind> {
    Some(match name {
        "message" => RowKind::Message,
        "event" => RowKind::Event,
        "status" => RowKind::Status,
        "whois" => RowKind::Whois,
        _ => return None,
    })
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
                kind: kind_name(line.kind).to_owned(),
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
                    kind: RowKind::Message,
                    author: None,
                    time: None,
                    text: String::new(),
                    msgid: place.msg_id.clone(),
                },
            });
        }
        let kind = kind_from_name(&place.kind)?;
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                line: LineKey::of(&rows[2]),
            }
        );
    }

    #[test]
    fn an_empty_transcript_has_no_line_to_keep() {
        assert_eq!(SavedPlace::from_anchor(anchor(0, 0), &[]), SavedPlace::End);
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
    fn a_place_clamps_when_its_line_is_gone() {
        let rows = rows(5);
        let place = SavedPlace::from_anchor(anchor(3, 9), &rows);
        let fewer = rows[..2].to_vec();
        assert_eq!(
            resume(Some(&place), &fewer),
            Resume::Line {
                row: 1,
                offset_px: 9
            }
        );
    }

    #[test]
    fn a_place_in_an_emptied_transcript_is_the_end() {
        let rows = rows(5);
        let place = SavedPlace::from_anchor(anchor(3, 9), &rows);
        assert_eq!(resume(Some(&place), &[]), Resume::End);
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
    }
}
