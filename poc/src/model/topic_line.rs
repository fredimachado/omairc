//! Topic-line hit testing (`TopicLine.qml`). Clicks and the pointing-hand cursor
//! respect ellipsis per hit: a leading URL stays active when the tail is elided.

use std::ops::Range;

const ELLIPSIS: char = '\u{2026}';

/// Whether `x` sits on the ellipsis glyph while the line is elided right.
pub fn ellipsis_covers_x(truncated: bool, x: f32, width: f32, ellipsis_width: f32) -> bool {
    truncated && x >= width - ellipsis_width
}

/// Character index under `x` for a monospace line (`TopicLine.indexAt`).
pub fn index_at_monospace(text: &str, x: f32, char_width: f32) -> usize {
    if text.is_empty() || x <= 0. || char_width <= 0. {
        return 0;
    }
    let mut lo = 0usize;
    let mut hi = text.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if !text.is_char_boundary(mid) {
            lo = mid + 1;
            continue;
        }
        let edge = (text[..=mid].chars().count() as f32) * char_width;
        if edge <= x {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo >= text.len() {
        return text.len().saturating_sub(1);
    }
    lo
}

/// Byte offset of the first character that would be hidden by a right ellipsis.
pub fn visible_end_index(text: &str, width: f32, char_width: f32, ellipsis_width: f32) -> usize {
    if text.is_empty() || char_width <= 0. {
        return 0;
    }
    let max_chars = ((width - ellipsis_width) / char_width).floor() as usize;
    if max_chars >= text.chars().count() {
        return text.len();
    }
    text.char_indices()
        .nth(max_chars)
        .map(|(at, _)| at)
        .unwrap_or(text.len())
}

/// Whether the line would elide at this width.
pub fn truncated(text: &str, width: f32, char_width: f32) -> bool {
    if char_width <= 0. {
        return false;
    }
    (text.chars().count() as f32) * char_width > width
}

/// Clickable byte ranges for `InteractiveText`: each hit is clipped to the visible
/// prefix when the line truncates. A whole-topic guard would drop every hit.
pub fn clipped_hit_ranges(
    hits: &[Range<usize>],
    text: &str,
    width: f32,
    char_width: f32,
    ellipsis_width: f32,
) -> Vec<Range<usize>> {
    if !truncated(text, width, char_width) {
        return hits.to_vec();
    }
    let visible_end = visible_end_index(text, width, char_width, ellipsis_width);
    hits.iter()
        .filter_map(|range| {
            if range.start >= visible_end {
                return None;
            }
            let end = range.end.min(visible_end);
            (range.start < end).then_some(range.start..end)
        })
        .collect()
}

/// Which hit owns the character at `index`, if any.
pub fn hit_index_at_char(index: usize, hits: &[Range<usize>]) -> Option<usize> {
    hits.iter().position(|range| range.contains(&index))
}

/// Whether the pointer at `x` should show a pointing hand for a hit.
pub fn pointing_hand_at(
    x: f32,
    text: &str,
    hits: &[Range<usize>],
    width: f32,
    char_width: f32,
    ellipsis_width: f32,
) -> bool {
    let trunc = truncated(text, width, char_width);
    if ellipsis_covers_x(trunc, x, width, ellipsis_width) {
        return false;
    }
    let index = index_at_monospace(text, x, char_width);
    if trunc && index >= visible_end_index(text, width, char_width, ellipsis_width) {
        return false;
    }
    hit_index_at_char(index, hits).is_some()
}

pub fn ellipsis_width(char_width: f32) -> f32 {
    char_width * ELLIPSIS.len_utf8() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_hit_stays_clickable_when_the_topic_truncates() {
        let text = "aaaa https://b.example tail #chan";
        let url = 5..22;
        let channel = 28..33;
        let width = 100.;
        let cw = 6.;
        let ew = ellipsis_width(cw);
        let clipped = clipped_hit_ranges(&[url.clone(), channel], text, width, cw, ew);
        assert_eq!(clipped.len(), 1, "only the visible hit range is wired");
        assert!(clipped[0].start == url.start && clipped[0].end <= url.end);
        assert!(
            pointing_hand_at(40., text, &[url], width, cw, ew),
            "the URL under the pointer still counts as a hit"
        );
    }

    #[test]
    fn ellipsis_guards_each_hit_not_the_whole_topic() {
        let text = "aaaa https://b.example tail #chan";
        let url = 5..22;
        let width = 100.;
        let cw = 6.;
        let ew = ellipsis_width(cw);
        assert!(
            !pointing_hand_at(95., text, &[url.clone()], width, cw, ew),
            "the ellipsis zone is not a hit"
        );
        let clipped_whole_topic_guard: Vec<Range<usize>> =
            if truncated(text, width, cw) { vec![] } else { vec![url.clone()] };
        assert!(
            clipped_whole_topic_guard.is_empty()
                && !clipped_hit_ranges(&[url.clone()], text, width, cw, ew).is_empty(),
            "per-hit clipping must not clear every range when truncated"
        );
    }
}
