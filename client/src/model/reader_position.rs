//! Where the reader is in one conversation's transcript: one value, so the readings that
//! cannot hold together are not storable together. A reader who follows the end has no
//! catch-up and no saved place waiting. A catch-up needs an anchor, so it belongs to a reader
//! who is scrolled up. Only following the end reports follow-end to the read marker
//! (`Controller::note_transcript_viewport`), so a restore that has not landed, or has landed on
//! a line, never lets it advance past unread backlog.
//!
//! `readerPosition` in `tui/internal/ui/position.go` names the same four positions.

use super::scroll_place::SavedPlace;

/// A line and how far into it the top of the view sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub row: usize,
    pub offset_px: i32,
}

/// Exactly one at a time.
#[derive(Clone, Debug, PartialEq)]
pub enum ReaderPosition {
    /// The view shows the end and follows lines as they arrive.
    FollowingEnd,
    /// The view is held on a line.
    Anchored { anchor: Anchor },
    /// A saved line the transcript waits for (history still in flight). The view stays at
    /// the end until the line is there, but its reader has not reached it.
    /// `open_at_unread` is the unread preference the wait started with: a give-up lands as a
    /// conversation with no saved place does.
    Restoring {
        place: SavedPlace,
        open_at_unread: bool,
    },
    /// The view is held on a line while lines that arrived since sit below it. `first_unseen`
    /// is the first of them, where the unseen jump lands.
    CatchingUp { anchor: Anchor, first_unseen: usize },
}

impl ReaderPosition {
    /// Whether the reader follows the end, the one thing the read marker may advance on.
    pub fn reports_follow_end(&self) -> bool {
        matches!(self, Self::FollowingEnd)
    }

    /// Whether the list lays out at its tail: a reader who follows the end, and one whose
    /// saved line has not arrived yet.
    pub fn shows_end(&self) -> bool {
        matches!(self, Self::FollowingEnd | Self::Restoring { .. })
    }

    /// The line the view is held on. `None` while it shows the end.
    pub fn anchor(&self) -> Option<Anchor> {
        match self {
            Self::Anchored { anchor } | Self::CatchingUp { anchor, .. } => Some(*anchor),
            Self::FollowingEnd | Self::Restoring { .. } => None,
        }
    }

    /// The first line that arrived while the reader was scrolled up, while a catch-up is on.
    pub fn first_unseen(&self) -> Option<usize> {
        match self {
            Self::CatchingUp { first_unseen, .. } => Some(*first_unseen),
            _ => None,
        }
    }

    /// The saved line a restore waits for.
    pub fn restoring(&self) -> Option<(&SavedPlace, bool)> {
        match self {
            Self::Restoring {
                place,
                open_at_unread,
            } => Some((place, *open_at_unread)),
            _ => None,
        }
    }

    /// The same position held on `anchor`, keeping a catch-up. A reader at the end, or
    /// waiting on a saved line, is held on it.
    pub fn held_at(&self, anchor: Anchor) -> Self {
        match self {
            Self::CatchingUp { first_unseen, .. } => Self::CatchingUp {
                anchor,
                first_unseen: *first_unseen,
            },
            _ => Self::Anchored { anchor },
        }
    }

    /// The position without its catch-up: the reader stays on the same line.
    pub fn without_catch_up(&self) -> Self {
        match self {
            Self::CatchingUp { anchor, .. } => Self::Anchored { anchor: *anchor },
            other => other.clone(),
        }
    }

    /// Rows were added or trimmed above the lines the reader has: every row moved by `delta`.
    /// A catch-up row follows the move, and a catch-up whose row left the `rows` that remain
    /// is over. The reader stays on the same line either way.
    pub fn after_rows_moved(self, delta: isize, rows: usize) -> Self {
        match self {
            Self::CatchingUp {
                anchor,
                first_unseen,
            } => match first_unseen
                .checked_add_signed(delta)
                .filter(|&row| row < rows)
            {
                Some(first_unseen) => Self::CatchingUp {
                    anchor,
                    first_unseen,
                },
                None => Self::Anchored { anchor },
            },
            other => other,
        }
    }

    /// Lines arrived: the row count went from `previous` to `new`. A reader held on a line
    /// starts a catch-up at the first new row. A reader at the end has seen them, and a
    /// reader who waits on a saved line has not reached them. A shrink drops a catch-up whose
    /// row is gone.
    pub fn after_row_growth(self, previous: usize, new: usize) -> Self {
        if new < previous {
            return match self {
                Self::CatchingUp {
                    anchor,
                    first_unseen,
                } if first_unseen >= new => Self::Anchored { anchor },
                other => other,
            };
        }
        if new == previous {
            return self;
        }
        match self {
            Self::Anchored { anchor } => Self::CatchingUp {
                anchor,
                first_unseen: previous,
            },
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(row: usize) -> Anchor {
        Anchor { row, offset_px: 0 }
    }

    fn waiting() -> ReaderPosition {
        ReaderPosition::Restoring {
            place: SavedPlace::End,
            open_at_unread: true,
        }
    }

    // A catch-up needs an anchor, and a saved line that has not landed has no catch-up: the
    // variants cannot hold a follow-end beside either, so there is no runtime test for it.

    #[test]
    fn reports_follow_end_only_for_following_end() {
        assert!(ReaderPosition::FollowingEnd.reports_follow_end());
        assert!(!ReaderPosition::Anchored { anchor: anchor(3) }.reports_follow_end());
        assert!(!waiting().reports_follow_end());
        assert!(!ReaderPosition::CatchingUp {
            anchor: anchor(3),
            first_unseen: 9
        }
        .reports_follow_end());
    }

    #[test]
    fn the_list_lays_out_at_its_tail_for_the_end_and_for_a_waiting_restore() {
        assert!(ReaderPosition::FollowingEnd.shows_end());
        assert!(waiting().shows_end());
        assert!(!ReaderPosition::Anchored { anchor: anchor(1) }.shows_end());
        assert!(!ReaderPosition::CatchingUp {
            anchor: anchor(1),
            first_unseen: 2
        }
        .shows_end());
    }

    #[test]
    fn growth_while_held_on_a_line_starts_a_catch_up_at_the_first_new_row() {
        let held = ReaderPosition::Anchored { anchor: anchor(2) };
        assert_eq!(
            held.after_row_growth(4, 6),
            ReaderPosition::CatchingUp {
                anchor: anchor(2),
                first_unseen: 4
            }
        );
    }

    #[test]
    fn later_growth_keeps_the_first_unseen_row() {
        let catching = ReaderPosition::CatchingUp {
            anchor: anchor(2),
            first_unseen: 4,
        };
        assert_eq!(catching.clone().after_row_growth(6, 8), catching);
    }

    #[test]
    fn growth_never_starts_a_catch_up_at_the_end_or_while_waiting() {
        assert_eq!(
            ReaderPosition::FollowingEnd.after_row_growth(4, 6),
            ReaderPosition::FollowingEnd
        );
        assert_eq!(waiting().after_row_growth(4, 6), waiting());
    }

    #[test]
    fn a_shrink_drops_a_catch_up_whose_row_is_gone() {
        let catching = ReaderPosition::CatchingUp {
            anchor: anchor(1),
            first_unseen: 7,
        };
        assert_eq!(
            catching.clone().after_row_growth(9, 5),
            ReaderPosition::Anchored { anchor: anchor(1) }
        );
        assert_eq!(catching.clone().after_row_growth(9, 8), catching);
    }

    #[test]
    fn a_catch_up_row_moves_with_the_rows_above_it() {
        let catching = ReaderPosition::CatchingUp {
            anchor: anchor(1),
            first_unseen: 7,
        };
        assert_eq!(
            catching.clone().after_rows_moved(9, 30),
            ReaderPosition::CatchingUp {
                anchor: anchor(1),
                first_unseen: 16
            }
        );
        assert_eq!(
            catching.clone().after_rows_moved(-3, 30),
            ReaderPosition::CatchingUp {
                anchor: anchor(1),
                first_unseen: 4
            }
        );
        assert_eq!(
            catching.clone().after_rows_moved(-8, 30),
            ReaderPosition::Anchored { anchor: anchor(1) },
            "a trim that took the row ends the catch-up"
        );
        assert_eq!(
            catching.after_rows_moved(30, 30),
            ReaderPosition::Anchored { anchor: anchor(1) },
            "a row past the rows that remain ends it too"
        );
        assert_eq!(
            ReaderPosition::FollowingEnd.after_rows_moved(5, 9),
            ReaderPosition::FollowingEnd
        );
    }

    #[test]
    fn holding_a_new_line_keeps_a_catch_up_and_a_row_jump_drops_it() {
        let catching = ReaderPosition::CatchingUp {
            anchor: anchor(1),
            first_unseen: 7,
        };
        assert_eq!(
            catching.held_at(anchor(3)),
            ReaderPosition::CatchingUp {
                anchor: anchor(3),
                first_unseen: 7
            }
        );
        assert_eq!(
            catching.without_catch_up(),
            ReaderPosition::Anchored { anchor: anchor(1) }
        );
        assert_eq!(
            ReaderPosition::FollowingEnd.held_at(anchor(3)),
            ReaderPosition::Anchored { anchor: anchor(3) }
        );
    }
}
