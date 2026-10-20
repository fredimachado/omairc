//! Where a conversation's transcript was left, so returning to it opens at the
//! same line (Qt `IrcScrollPlaceStore`, `rememberOpenTranscript` and
//! `restoreRememberedTranscript`).
//!
//! Qt keys the place on the top line's content and persists it. The PoC keeps
//! the anchor row and pixel offset from `Transcript::scroll()` in memory, and
//! checks the line's content on return, so a row inserted or hidden above it
//! (a `/pref joins` change, say) does not move the view to a different line.

use std::collections::HashMap;

use super::ids::ConversationRef;
use super::transcript::{Row, RowKind, ScrollAnchor};

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
}

impl LineKey {
    pub fn of(row: &Row) -> Self {
        Self {
            kind: row.kind,
            author: row.author.clone(),
            time: row.time.clone(),
            text: row.body.plain.clone(),
        }
    }

    fn is_row(&self, row: &Row) -> bool {
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
    let found = if rows.get(*row).is_some_and(|candidate| line.is_row(candidate)) {
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

/// The saved places, one per conversation. Status is never kept: it always
/// opens at the end, so the shell does not store it here.
#[derive(Debug, Default)]
pub struct ScrollPlaces {
    places: HashMap<ConversationRef, SavedPlace>,
}

impl ScrollPlaces {
    pub fn remember(&mut self, target: ConversationRef, place: SavedPlace) {
        self.places.insert(target, place);
    }

    pub fn get(&self, target: &ConversationRef) -> Option<&SavedPlace> {
        self.places.get(target)
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
    fn places_are_kept_per_conversation() {
        let mut places = ScrollPlaces::default();
        let anna = ConversationRef::new("omarchy", "anna");
        let dax = ConversationRef::new("omarchy", "dax");
        places.remember(anna.clone(), SavedPlace::End);
        assert_eq!(places.get(&anna), Some(&SavedPlace::End));
        assert_eq!(places.get(&dax), None);
    }

    #[test]
    fn a_later_save_replaces_the_earlier_place() {
        let mut places = ScrollPlaces::default();
        let anna = ConversationRef::new("omarchy", "anna");
        places.remember(anna.clone(), SavedPlace::End);
        let rows = rows(4);
        places.remember(anna.clone(), SavedPlace::from_anchor(anchor(1, 0), &rows));
        assert!(matches!(places.get(&anna), Some(SavedPlace::Line { row: 1, .. })));
    }
}
