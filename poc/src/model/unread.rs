//! The "New messages" mark row and the unseen jump, as the transcript shows them. Ports
//! `unreadMarkRow` in `src/irc/messagelistmodel.cpp` and `jumpToUnseen` / `pinToUnread`
//! in `src/qml/TranscriptList.qml`.
//!
//! The mark itself is the controller's: `ConversationState::unread_mark` holds the
//! sequence of the line the boundary sits in front of, and the reducer moves it on
//! arrivals and reads. `transcript::rows_from_messages` turns it into a
//! `RowKind::UnreadMark` row. This module only answers where the open view lands on
//! it, and where the unseen jump goes.

use super::transcript::{Row, RowKind};

/// The row of the open "New messages" mark, if the rows hold one.
pub fn unread_mark_row(rows: &[Row]) -> Option<usize> {
    rows.iter().position(|row| row.kind == RowKind::UnreadMark)
}

/// The row a conversation opens at when `open_at_unread` is on: its mark (Qt
/// `pinTranscriptToUnreadOr`). `None` opens at the end.
pub fn opening_row(rows: &[Row], open_at_unread: bool) -> Option<usize> {
    open_at_unread.then(|| unread_mark_row(rows)).flatten()
}

/// Where the unseen jump lands (`↓` chip and Alt+U).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnseenLanding {
    /// The first line that arrived while the reader was scrolled up.
    Row(usize),
    /// The end, while a mark is open and no arrival is waiting.
    End,
}

/// What the unseen jump does from this view (Qt `jumpArmed` and `jumpToUnseen`).
/// Nothing while the reader follows the end. A waiting arrival wins over the mark.
pub fn unseen_landing(
    detached: bool,
    first_unseen: Option<usize>,
    rows: &[Row],
) -> Option<UnseenLanding> {
    if !detached {
        return None;
    }
    if let Some(row) = first_unseen.filter(|&row| row < rows.len()) {
        return Some(UnseenLanding::Row(row));
    }
    unread_mark_row(rows).map(|_| UnseenLanding::End)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::format::format_irc;

    fn row(kind: RowKind) -> Row {
        Row {
            id: 1,
            kind,
            author: None,
            time: None,
            body: format_irc("x"),
            continues: false,
            action: false,
            separator_text: None,
            event_text: None,
            status: None,
        }
    }

    #[test]
    fn the_mark_row_is_found_by_kind() {
        let rows = vec![row(RowKind::Message), row(RowKind::UnreadMark), row(RowKind::Message)];
        assert_eq!(unread_mark_row(&rows), Some(1));
        assert_eq!(unread_mark_row(&[row(RowKind::Message)]), None);
    }

    #[test]
    fn opening_lands_on_the_mark_only_when_asked() {
        let rows = vec![row(RowKind::Message), row(RowKind::UnreadMark)];
        assert_eq!(opening_row(&rows, true), Some(1));
        assert_eq!(opening_row(&rows, false), None);
        assert_eq!(opening_row(&[row(RowKind::Message)], true), None);
    }

    #[test]
    fn the_unseen_jump_follows_the_reader_or_waits_for_it() {
        let rows = vec![row(RowKind::Message), row(RowKind::UnreadMark), row(RowKind::Message)];
        assert_eq!(unseen_landing(false, Some(2), &rows), None, "at the end: nothing to jump");
        assert_eq!(unseen_landing(true, Some(2), &rows), Some(UnseenLanding::Row(2)));
        assert_eq!(unseen_landing(true, None, &rows), Some(UnseenLanding::End));
        assert_eq!(unseen_landing(true, None, &[row(RowKind::Message)]), None);
    }
}
