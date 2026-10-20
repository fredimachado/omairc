//! Keyboard cursor math shared by the transcript and the member list.
//! Paging is `round(extent × 0.8)` for a page and `× 0.35` for a half page,
//! as in the GPUI plan's shortcut scope. Pure functions only.

/// How far one page (or half page) moves across `extent`, rounded to whole
/// units. The transcript passes its viewport height in pixels; the member
/// list passes its visible row count.
pub fn page_amount(extent: f32, half: bool) -> f32 {
    let factor = if half { 0.35 } else { 0.8 };
    (extent * factor).round().max(0.0)
}

/// Moves a list cursor by `delta` without wrapping. Clamps to `0..len`.
/// An empty list stays at 0.
pub fn move_index(len: usize, current: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let last = len as isize - 1;
    (current as isize + delta).clamp(0, last) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_is_eighty_percent_and_half_is_thirty_five() {
        assert_eq!(page_amount(700., false), 560.);
        assert_eq!(page_amount(700., true), 245.);
        assert_eq!(page_amount(10., false), 8.);
        assert_eq!(page_amount(1., true), 0.);
        assert_eq!(page_amount(-5., false), 0.);
    }

    #[test]
    fn cursor_clamps_at_both_ends_without_wrapping() {
        assert_eq!(move_index(5, 0, -1), 0);
        assert_eq!(move_index(5, 4, 1), 4);
        assert_eq!(move_index(5, 1, 1), 2);
        assert_eq!(move_index(5, 2, -10), 0);
        assert_eq!(move_index(5, 0, 10), 4);
        assert_eq!(move_index(0, 0, 3), 0);
    }
}
