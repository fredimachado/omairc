//! Link sheet rows: the topic's links first, then the transcript's links from
//! newest row to oldest, the last link in each row first. Port of
//! `refreshLinkMatches` for the http(s) part. URL policy is already applied by
//! `format_irc`, which only records allowed http(s) ranges in `body.links`.
//! Channel-name entries wait for the Channel open sheet.

use super::format::format_irc;
use super::fuzzy::rank;
use super::transcript::{Row, RowKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkTarget {
    pub url: String,
    /// Index into `rows` of the message the link came from. `None` for a
    /// topic link, which Qt reveals nowhere.
    pub row: Option<usize>,
    /// Byte offsets into `url` of the characters the query matched.
    pub indices: Vec<usize>,
}

/// Links that fuzzy-match the query, best match first. Ties keep the order
/// the links were found in: topic links, then transcript links newest row
/// first. An empty query keeps every link in that order.
pub fn link_targets(topic: Option<&str>, rows: &[Row], query: &str) -> Vec<LinkTarget> {
    let mut found = Vec::new();

    if let Some(topic) = topic {
        let formatted = format_irc(topic);
        push_links(&mut found, &formatted.plain, &formatted.links, None);
    }

    for (index, row) in rows.iter().enumerate().rev() {
        if row.kind != RowKind::Message {
            continue;
        }
        push_links(&mut found, &row.body.plain, &row.body.links, Some(index));
    }

    rank(query, found, |link: &LinkTarget| [(link.url.as_str(), 1)])
        .into_iter()
        .map(|ranked| {
            let [hit] = ranked.fields;
            LinkTarget {
                indices: hit.map(|hit| hit.indices).unwrap_or_default(),
                ..ranked.item
            }
        })
        .collect()
}

/// Shortens `text` to at most `max_chars` characters by replacing its middle
/// with `…`, keeping both ends (Qt's `ElideMiddle`, counted in characters).
/// Also returns where each marked character sits in the elided text. `marks`
/// and the result are byte offsets. A mark inside the elided middle is
/// dropped, so it is not drawn.
pub fn elide_middle_marked(text: &str, max_chars: usize, marks: &[usize]) -> (String, Vec<usize>) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    if chars.len() <= max_chars {
        return (text.to_string(), marks.to_vec());
    }
    let (head, tail) = match max_chars {
        0 => return (String::new(), Vec::new()),
        1 => return ("…".to_string(), Vec::new()),
        _ => {
            let keep = max_chars - 1;
            (keep - keep / 2, keep / 2)
        }
    };

    // (byte offset in `text`, byte offset in the output) for each kept character.
    let mut kept = Vec::with_capacity(head + tail);
    let mut out = String::new();
    for &(source, ch) in &chars[..head] {
        kept.push((source, out.len()));
        out.push(ch);
    }
    out.push('…');
    for &(source, ch) in &chars[chars.len() - tail..] {
        kept.push((source, out.len()));
        out.push(ch);
    }

    let marks = marks
        .iter()
        .filter_map(|mark| {
            kept.binary_search_by_key(mark, |&(source, _)| source)
                .ok()
                .map(|found| kept[found].1)
        })
        .collect();
    (out, marks)
}

fn push_links(
    out: &mut Vec<LinkTarget>,
    plain: &str,
    links: &[std::ops::Range<usize>],
    row: Option<usize>,
) {
    for range in links.iter().rev() {
        out.push(LinkTarget {
            url: plain[range.clone()].to_string(),
            row,
            indices: Vec::new(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::transcript::row_from_message;

    fn urls(targets: &[LinkTarget]) -> Vec<&str> {
        targets.iter().map(|t| t.url.as_str()).collect()
    }

    fn transcript() -> Vec<Row> {
        vec![
            row_from_message(1, "anna", "09:41", "see https://a.example/one", false),
            row_from_message(
                2,
                "dax",
                "09:43",
                "two: https://b.example/two and https://c.example/three",
                false,
            ),
            row_from_message(3, "mira", "09:44", "nothing here", false),
        ]
    }

    #[test]
    fn newest_row_first_and_last_link_in_a_row_first() {
        let rows = transcript();
        let targets = link_targets(None, &rows, "");
        assert_eq!(
            urls(&targets),
            [
                "https://c.example/three",
                "https://b.example/two",
                "https://a.example/one",
            ]
        );
        assert_eq!(targets[0].row, Some(1));
        assert_eq!(targets[2].row, Some(0));
    }

    #[test]
    fn topic_links_come_before_transcript_links() {
        let rows = transcript();
        let targets = link_targets(Some("docs at https://docs.example/"), &rows, "");
        assert_eq!(targets[0].url, "https://docs.example/");
        assert_eq!(targets[0].row, None);
        assert_eq!(targets.len(), 4);
    }

    #[test]
    fn query_filters_by_url_case_insensitively() {
        let rows = transcript();
        let targets = link_targets(None, &rows, "  B.EXAMPLE ");
        assert_eq!(urls(&targets), ["https://b.example/two"]);
    }

    #[test]
    fn event_and_separator_rows_contribute_nothing() {
        let mut event = row_from_message(9, "", "", "https://ignored.example/", false);
        event.kind = RowKind::Event;
        let rows = vec![event];
        assert!(link_targets(None, &rows, "").is_empty());
    }

    fn elide(text: &str, max_chars: usize) -> String {
        elide_middle_marked(text, max_chars, &[]).0
    }

    #[test]
    fn elide_middle_keeps_both_ends() {
        assert_eq!(elide("https://a.example/x", 40), "https://a.example/x");
        assert_eq!(elide("abcdefghij", 5), "ab…ij");
        assert_eq!(elide("abcdefghij", 4), "ab…j");
        assert_eq!(elide("abcdefghij", 1), "…");
        assert_eq!(elide("abcdefghij", 0), "");
        assert_eq!(elide("abcdefghij", 5).chars().count(), 5);
    }

    #[test]
    fn fuzzy_query_matches_a_subsequence_of_the_url() {
        let rows = transcript();
        let targets = link_targets(None, &rows, "hbt");
        assert_eq!(urls(&targets), ["https://b.example/two"]);
        let picked: Vec<char> = targets[0]
            .indices
            .iter()
            .map(|&at| targets[0].url[at..].chars().next().unwrap())
            .collect();
        assert_eq!(picked, ['h', 'b', 't']);
    }

    #[test]
    fn a_tie_keeps_the_topic_link_first() {
        let url = "https://docs.example/";
        let rows = vec![row_from_message(1, "anna", "09:41", url, false)];
        let targets = link_targets(Some(url), &rows, "docs");
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].row, None);
        assert_eq!(targets[1].row, Some(0));
    }

    #[test]
    fn elision_keeps_marks_on_the_kept_ends_only() {
        // "ab…ij": a and b keep their place; i is shifted by the ellipsis; the
        // middle (c..h) is elided, so its mark disappears.
        let (text, marks) = elide_middle_marked("abcdefghij", 5, &[0, 1, 4, 9]);
        assert_eq!(text, "ab…ij");
        assert_eq!(marks, [0, 1, 6]);
    }

    #[test]
    fn elision_remaps_marks_after_multibyte_characters() {
        // "üa…gh": the ü takes two bytes, so the final h moves from byte 10 to byte 7.
        let (text, marks) = elide_middle_marked("üabcdefgh", 5, &[0, 9]);
        assert_eq!(text, "üa…gh");
        assert_eq!(marks, [0, 7]);
        assert!(text.is_char_boundary(marks[1]));
    }

    #[test]
    fn unelided_text_keeps_every_mark() {
        let (text, marks) = elide_middle_marked("short", 10, &[1, 3]);
        assert_eq!(text, "short");
        assert_eq!(marks, [1, 3]);
    }

    #[test]
    fn no_links_is_an_empty_list() {
        assert!(link_targets(None, &[], "").is_empty());
        assert!(link_targets(Some("plain topic"), &[], "").is_empty());
    }
}
