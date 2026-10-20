//! Link sheet rows: the topic's links first, then the transcript's links from
//! newest row to oldest, the last link in each row first. Port of
//! `refreshLinkMatches` for the http(s) part. URL policy is already applied by
//! `format_irc`, which only records allowed http(s) ranges in `body.links`.
//! Channel-name entries wait for the Channel open sheet.

use super::format::format_irc;
use super::transcript::{Row, RowKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkTarget {
    pub url: String,
    /// Index into `rows` of the message the link came from. `None` for a
    /// topic link, which Qt reveals nowhere.
    pub row: Option<usize>,
}

/// Links whose URL contains the trimmed, case-folded query. An empty query
/// keeps every link.
pub fn link_targets(topic: Option<&str>, rows: &[Row], query: &str) -> Vec<LinkTarget> {
    let needle = query.trim().to_lowercase();
    let mut out = Vec::new();

    if let Some(topic) = topic {
        let formatted = format_irc(topic);
        push_links(&mut out, &formatted.plain, &formatted.links, None, &needle);
    }

    for (index, row) in rows.iter().enumerate().rev() {
        if row.kind != RowKind::Message {
            continue;
        }
        push_links(
            &mut out,
            &row.body.plain,
            &row.body.links,
            Some(index),
            &needle,
        );
    }

    out
}

/// Shortens `text` to at most `max_chars` characters by replacing its middle
/// with `…`, keeping both ends (Qt's `ElideMiddle`, counted in characters).
pub fn elide_middle(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    match max_chars {
        0 => String::new(),
        1 => "…".to_string(),
        _ => {
            let keep = max_chars - 1;
            let head = keep - keep / 2;
            let tail = keep / 2;
            let mut out: String = chars[..head].iter().collect();
            out.push('…');
            out.extend(chars[chars.len() - tail..].iter());
            out
        }
    }
}

fn push_links(
    out: &mut Vec<LinkTarget>,
    plain: &str,
    links: &[std::ops::Range<usize>],
    row: Option<usize>,
    needle: &str,
) {
    for range in links.iter().rev() {
        let url = &plain[range.clone()];
        if !needle.is_empty() && !url.to_lowercase().contains(needle) {
            continue;
        }
        out.push(LinkTarget {
            url: url.to_string(),
            row,
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

    #[test]
    fn elide_middle_keeps_both_ends() {
        assert_eq!(elide_middle("https://a.example/x", 40), "https://a.example/x");
        assert_eq!(elide_middle("abcdefghij", 5), "ab…ij");
        assert_eq!(elide_middle("abcdefghij", 4), "ab…j");
        assert_eq!(elide_middle("abcdefghij", 1), "…");
        assert_eq!(elide_middle("abcdefghij", 0), "");
        assert_eq!(elide_middle("abcdefghij", 5).chars().count(), 5);
    }

    #[test]
    fn no_links_is_an_empty_list() {
        assert!(link_targets(None, &[], "").is_empty());
        assert!(link_targets(Some("plain topic"), &[], "").is_empty());
    }
}
