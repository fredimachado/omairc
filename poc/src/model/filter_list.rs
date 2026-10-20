//! Selection math shared by the filter sheets (Jump, Link, Nick, Inbox).
//! Matches the Qt sheets: Up/Down wrap, and a filter that shrinks the list
//! clamps the selection to the last remaining row.

/// Moves the selection by `delta` rows, wrapping at both ends. An empty list
/// always yields 0.
pub fn step(selected: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let len = len as isize;
    (selected as isize + delta).rem_euclid(len) as usize
}

/// Keeps the selection on a row that still exists after the list changed.
pub fn clamp_after_filter(selected: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        selected.min(len - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_moves_within_the_list() {
        assert_eq!(step(0, 3, 1), 1);
        assert_eq!(step(1, 3, 1), 2);
        assert_eq!(step(2, 3, -1), 1);
    }

    #[test]
    fn step_wraps_at_both_ends() {
        assert_eq!(step(2, 3, 1), 0);
        assert_eq!(step(0, 3, -1), 2);
        assert_eq!(step(0, 3, -4), 2);
        assert_eq!(step(0, 3, 7), 1);
    }

    #[test]
    fn step_on_an_empty_list_stays_at_zero() {
        assert_eq!(step(0, 0, 1), 0);
        assert_eq!(step(5, 0, -1), 0);
    }

    #[test]
    fn clamp_pulls_a_stale_selection_back_into_range() {
        assert_eq!(clamp_after_filter(4, 2), 1);
        assert_eq!(clamp_after_filter(1, 2), 1);
        assert_eq!(clamp_after_filter(3, 0), 0);
    }
}
