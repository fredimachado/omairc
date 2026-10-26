//! Fuzzy filtering for the GPUI sheets (Jump, Link, Nick and the `/list`
//! sheet). Qt and the TUI keep substring matching; only this client uses this.
//!
//! Matching is `nucleo-matcher`, so `omy` finds `#omarchy`. Hits report the
//! matched characters as byte offsets into the haystack, so the UI can draw
//! them bold without re-deriving anything.

use std::ops::Range;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// One successful match: its score, and where the query's characters landed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzyHit {
    pub score: u32,
    /// Byte offsets of the matched characters, ascending and unique.
    pub indices: Vec<usize>,
}

/// A query parsed once and matched against many haystacks. The matcher's
/// allocation is reused across calls, so build one per filter pass.
pub struct FuzzyQuery {
    /// `None` for a blank query, which matches everything.
    pattern: Option<Pattern>,
    matcher: Matcher,
    /// Codepoints of the current non-ASCII haystack, reused between calls.
    chars: Vec<char>,
}

impl FuzzyQuery {
    pub fn new(query: &str) -> Self {
        let query = query.trim();
        let pattern = (!query.is_empty())
            .then(|| Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart));
        Self {
            pattern,
            matcher: Matcher::new(Config::DEFAULT),
            chars: Vec::new(),
        }
    }

    /// Matches `haystack`. A blank query hits everything with score 0 and no
    /// indices.
    pub fn matches(&mut self, haystack: &str) -> Option<FuzzyHit> {
        let Some(pattern) = &self.pattern else {
            return Some(FuzzyHit {
                score: 0,
                indices: Vec::new(),
            });
        };

        // Codepoints, not `Utf32Str::new`: nucleo segments non-ASCII text into
        // graphemes there, and its indices would not line up with byte offsets.
        let ascii = haystack.is_ascii();
        let mut raw = Vec::new();
        let haystack_chars = if ascii {
            Utf32Str::Ascii(haystack.as_bytes())
        } else {
            self.chars.clear();
            self.chars.extend(haystack.chars());
            Utf32Str::Unicode(&self.chars)
        };
        let score = pattern.indices(haystack_chars, &mut self.matcher, &mut raw)?;

        raw.sort_unstable();
        raw.dedup();
        let indices = if ascii {
            raw.into_iter().map(|at| at as usize).collect()
        } else {
            let offsets: Vec<usize> = haystack.char_indices().map(|(at, _)| at).collect();
            raw.into_iter()
                .filter_map(|at| offsets.get(at as usize).copied())
                .collect()
        };
        Some(FuzzyHit { score, indices })
    }
}

/// One item that survived [`rank`].
#[derive(Clone, Debug, PartialEq)]
pub struct Ranked<T, const N: usize> {
    pub item: T,
    pub score: u32,
    /// Each field's own match, in the order `rank` was given them. `None`
    /// where that field did not match.
    pub fields: [Option<FuzzyHit>; N],
}

/// Filters and ranks `items` by `query`. `fields` names each searchable text
/// with its weight; an item's score is its best weighted field score. Items
/// that match no field are dropped. The sort is stable by score, descending
/// so ties keep the order of `items`.
pub fn rank<T, const N: usize>(
    query: &str,
    items: impl IntoIterator<Item = T>,
    fields: impl Fn(&T) -> [(&str, u32); N],
) -> Vec<Ranked<T, N>> {
    let mut matcher = FuzzyQuery::new(query);
    let mut ranked: Vec<Ranked<T, N>> = items
        .into_iter()
        .filter_map(|item| {
            let mut hits: [Option<FuzzyHit>; N] = std::array::from_fn(|_| None);
            let mut best: Option<u32> = None;
            for (slot, (text, weight)) in fields(&item).into_iter().enumerate() {
                let Some(hit) = matcher.matches(text) else {
                    continue;
                };
                let weighted = hit.score.saturating_mul(weight);
                best = Some(best.map_or(weighted, |current| current.max(weighted)));
                hits[slot] = Some(hit);
            }
            let score = best?;
            Some(Ranked {
                item,
                score,
                fields: hits,
            })
        })
        .collect();
    ranked.sort_by_key(|ranked| std::cmp::Reverse(ranked.score));
    ranked
}

/// The byte ranges of the characters that start at `starts`, for drawing.
/// Offsets that are not character boundaries of `text` are skipped.
pub fn char_ranges(text: &str, starts: &[usize]) -> Vec<Range<usize>> {
    starts
        .iter()
        .filter_map(|&start| {
            let ch = text.get(start..)?.chars().next()?;
            Some(start..start + ch.len_utf8())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fuzzy_match(query: &str, haystack: &str) -> Option<FuzzyHit> {
        FuzzyQuery::new(query).matches(haystack)
    }

    fn ranked_texts<'a>(query: &str, texts: &[&'a str]) -> Vec<&'a str> {
        rank(query, texts.iter().copied(), |text| [(*text, 1)])
            .into_iter()
            .map(|r| r.item)
            .collect()
    }

    #[test]
    fn subsequence_matches_with_gaps() {
        let hit = fuzzy_match("omy", "#omarchy").expect("omy is a subsequence");
        assert_eq!(hit.indices, [1, 2, 7]);
    }

    #[test]
    fn a_query_longer_than_the_haystack_misses_without_panicking() {
        assert!(fuzzy_match("abcdef", "ab").is_none());
        assert!(fuzzy_match("abcdef", "").is_none());
        assert!(fuzzy_match("ab", "ab").is_some());
        assert!(fuzzy_match("ééé", "é").is_none());
    }

    #[test]
    fn non_subsequence_misses() {
        assert!(fuzzy_match("zq", "#omarchy").is_none());
        assert!(fuzzy_match("ymo", "#omarchy").is_none());
    }

    #[test]
    fn exact_substring_outranks_a_scattered_match() {
        let contiguous = fuzzy_match("arc", "#archive").expect("contiguous hit");
        let scattered = fuzzy_match("arc", "#a-long-r-and-c").expect("scattered hit");
        assert!(contiguous.score > scattered.score);
        assert_eq!(contiguous.indices, [1, 2, 3]);
    }

    #[test]
    fn ranking_puts_the_contiguous_hit_first() {
        let order = ranked_texts("arc", &["#a-long-r-and-c", "#archive"]);
        assert_eq!(order, ["#archive", "#a-long-r-and-c"]);
    }

    #[test]
    fn matching_ignores_case_both_ways() {
        assert!(fuzzy_match("OMY", "#omarchy").is_some());
        assert!(fuzzy_match("omy", "#OMARCHY").is_some());
        assert!(fuzzy_match("Omy", "#OmArChY").is_some());
    }

    #[test]
    fn empty_and_blank_queries_match_everything_with_score_zero() {
        for query in ["", "   ", "\t"] {
            let hit = fuzzy_match(query, "anything").expect("blank matches");
            assert_eq!(hit.score, 0);
            assert!(hit.indices.is_empty());
        }
    }

    #[test]
    fn empty_query_keeps_order() {
        let texts = ["zeta", "alpha", "#mid"];
        assert_eq!(ranked_texts("", &texts), texts);
        assert_eq!(ranked_texts("  ", &texts), texts);
    }

    #[test]
    fn ties_keep_the_original_order() {
        let texts = ["b-one", "a-two", "c-three"];
        // Each is a single contiguous `-` hit, so the scores tie.
        let order = ranked_texts("-", &texts);
        assert_eq!(order, texts);
    }

    #[test]
    fn indices_land_on_character_boundaries_for_multibyte_text() {
        let text = "zürich #chat";
        let hit = fuzzy_match("rich", text).expect("rich in zürich");
        for &at in &hit.indices {
            assert!(text.is_char_boundary(at));
        }
        // The scorer may prefer the word-start "ch" of #chat over the one in
        // zürich, so only the spelling of the matched characters is fixed.
        let picked: Vec<char> = hit
            .indices
            .iter()
            .map(|&at| text[at..].chars().next().unwrap())
            .collect();
        assert_eq!(picked, ['r', 'i', 'c', 'h']);
    }

    #[test]
    fn indices_skip_wide_characters_correctly() {
        let text = "日本語 #chat";
        let hit = fuzzy_match("ch", text).expect("ch in #chat");
        let picked: Vec<char> = hit
            .indices
            .iter()
            .map(|&at| text[at..].chars().next().unwrap())
            .collect();
        assert_eq!(picked, ['c', 'h']);
    }

    #[test]
    fn char_ranges_cover_each_multibyte_character() {
        let text = "é#x";
        let ranges = char_ranges(text, &[0, 2]);
        assert_eq!(ranges, [0..2, 2..3]);
        assert_eq!(&text[ranges[0].clone()], "é");
    }

    #[test]
    fn char_ranges_skip_offsets_inside_a_character() {
        assert_eq!(char_ranges("é", &[1]), Vec::<Range<usize>>::new());
        assert_eq!(char_ranges("ab", &[9]), Vec::<Range<usize>>::new());
    }

    #[test]
    fn rank_reports_each_fields_own_hit() {
        let ranked = rank("omy", ["#omarchy"], |name| [(*name, 2), ("omy topic", 1)]);
        let row = &ranked[0];
        assert_eq!(row.fields[0].as_ref().map(|h| h.indices.len()), Some(3));
        assert!(row.fields[1].is_some());
    }

    #[test]
    fn rank_scores_by_the_best_weighted_field() {
        let ranked = rank("ab", ["ab"], |text| [(*text, 1), (*text, 3)]);
        let row = &ranked[0];
        let base = row.fields[0].as_ref().expect("weight 1 field hits").score;
        assert_eq!(row.score, base * 3);
    }

    #[test]
    fn rank_keeps_a_detail_only_hit_with_no_name_hit() {
        let ranked = rank("build", ["lab"], |name| [(*name, 2), ("build lab", 1)]);
        let row = &ranked[0];
        assert!(row.fields[0].is_none());
        assert!(row.fields[1].is_some());
    }

    #[test]
    fn rank_drops_items_that_match_no_field() {
        let ranked = rank("zq", ["#omarchy"], |name| [(*name, 2), ("topic", 1)]);
        assert!(ranked.is_empty());
    }

    #[test]
    fn rank_sorts_by_score_before_input_order() {
        // `#dsk` is contiguous in its own spelling, `#desk` is scattered.
        let items = vec![("#desk", "tie"), ("#dsk", "tie")];
        let ranked = rank("dsk", items, |(name, _)| [(*name, 2)]);
        let names: Vec<&str> = ranked.iter().map(|r| r.item.0).collect();
        assert_eq!(names, ["#dsk", "#desk"]);
    }
}
