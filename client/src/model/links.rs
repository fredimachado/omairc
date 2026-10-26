//! Link sheet rows. On a conversation: the topic's links first, then the
//! transcript's links from newest row to oldest, the last link in each row
//! first. Each line lists its URLs, then its channel names (`appendLinkUrlsFromText`
//! and `appendChannelNamesFromText`). On Status: the same walk over the Status
//! rows, plus an invite row after a `… invited you to #channel` line
//! (`refreshLinkMatches`). URL policy is already applied by `format_irc`, which
//! only records allowed http(s) ranges in `body.links`. Status has no channel rows.

use std::ops::Range;

use super::format::{channel_hits, format_irc, ChannelNameSpan, FormattedText};
use super::fuzzy::rank;
use super::transcript::{Row, RowKind};
use crate::irc::serverfeatures::IrcServerFeatures;

/// What a link row does when activated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// Opens the URL in the browser.
    Url,
    /// Joins the channel named in an INVITE line.
    Invite,
    /// Switches to a channel's row, or asks before joining it.
    Channel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkTarget {
    pub kind: LinkKind,
    /// The URL, or the channel name for an invite.
    pub text: String,
    /// Index into `rows` of the message the link came from. `None` for a
    /// topic link, which Qt reveals nowhere.
    pub row: Option<usize>,
    /// Byte offsets into `text` of the characters the query matched.
    pub indices: Vec<usize>,
}

/// Qt's marker in the INVITE status text, `"<nick> invited you to <channel>"`.
const INVITE_MARKER: &str = " invited you to ";

/// Links that fuzzy-match the query, best match first. Ties keep the order
/// the links were found topic links, then transcript links newest row
/// first. An empty query keeps every link in that order. A channel name is
/// listed only when the network's CHANTYPES (`features`) makes it one.
pub fn link_targets(
    topic: Option<&str>,
    rows: &[Row],
    query: &str,
    features: &IrcServerFeatures,
) -> Vec<LinkTarget> {
    let mut found = Vec::new();

    if let Some(topic) = topic {
        let formatted = format_irc(topic);
        push_line_links(&mut found, &formatted, None, features);
    }

    for (index, row) in rows.iter().enumerate().rev() {
        if !is_chat_line(row) {
            continue;
        }
        push_line_links(&mut found, &row.body, Some(index), features);
    }

    rank_links(query, found)
}

/// The channel names a transcript row offers to a click: a message or whois
/// line's names, outside URLs. Status and event rows offer none.
pub fn conversation_channel_hits(row: &Row, features: &IrcServerFeatures) -> Vec<ChannelNameSpan> {
    if !is_chat_line(row) {
        return Vec::new();
    }
    channel_hits(&row.body, features)
}

/// The rows whose text the Link sheet and the transcript's clicks read as chat:
/// messages, actions and whois lines (Qt skips only event and unread rows).
fn is_chat_line(row: &Row) -> bool {
    matches!(row.kind, RowKind::Message | RowKind::Whois)
}

/// The Status sheet's links: for each Status row newest first, its URLs (last
/// first), then an invite row when the line is an INVITE with a channel after
/// the marker. Ties keep that order.
pub fn status_link_targets(rows: &[Row], query: &str) -> Vec<LinkTarget> {
    let mut found = Vec::new();

    for (index, row) in rows.iter().enumerate().rev() {
        if row.kind != RowKind::Status {
            continue;
        }
        push_url_links(&mut found, &row.body.plain, &row.body.links, Some(index));
        if let Some((channel, _)) = invite_hit(row) {
            found.push(LinkTarget {
                kind: LinkKind::Invite,
                text: channel.to_string(),
                row: Some(index),
                indices: Vec::new(),
            });
        }
    }

    rank_links(query, found)
}

/// The channel an INVITE Status line offers, with the byte range that name
/// occupies in the line's text. Only a Status row labelled `INVITE` counts. The
/// name ends the line, so the range runs to the end.
pub fn invite_hit(row: &Row) -> Option<(&str, Range<usize>)> {
    if row.kind != RowKind::Status {
        return None;
    }
    let is_invite = row
        .status
        .as_ref()
        .is_some_and(|mark| mark.label == "INVITE");
    if !is_invite {
        return None;
    }
    let text = row.body.plain.as_str();
    let channel = invite_channel(text)?;
    Some((channel, text.len() - channel.len()..text.len()))
}

/// The channel in an invite line: the text after the last `" invited you to "`
/// when it is not empty (`refreshLinkMatches` uses `lastIndexOf`).
fn invite_channel(text: &str) -> Option<&str> {
    let at = text.rfind(INVITE_MARKER)?;
    let channel = &text[at + INVITE_MARKER.len()..];
    (!channel.is_empty()).then_some(channel)
}

/// Fuzzy-ranks `found` by its text. Keeps `found`'s order on ties.
fn rank_links(query: &str, found: Vec<LinkTarget>) -> Vec<LinkTarget> {
    rank(query, found, |link: &LinkTarget| [(link.text.as_str(), 1)])
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

/// One line's entries: its URLs last first, then its channel names last first.
fn push_line_links(
    out: &mut Vec<LinkTarget>,
    text: &FormattedText,
    row: Option<usize>,
    features: &IrcServerFeatures,
) {
    push_url_links(out, &text.plain, &text.links, row);
    for span in channel_hits(text, features).into_iter().rev() {
        out.push(LinkTarget {
            kind: LinkKind::Channel,
            text: span.name,
            row,
            indices: Vec::new(),
        });
    }
}

fn push_url_links(
    out: &mut Vec<LinkTarget>,
    plain: &str,
    links: &[Range<usize>],
    row: Option<usize>,
) {
    for range in links.iter().rev() {
        out.push(LinkTarget {
            kind: LinkKind::Url,
            text: plain[range.clone()].to_string(),
            row,
            indices: Vec::new(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::controller::Controller;
    use crate::irc::statusentry::{outcome, LogSource, StatusEntry};
    use crate::model::status::row_from_status_entry;
    use crate::model::transcript::row_from_message;

    fn urls(targets: &[LinkTarget]) -> Vec<&str> {
        targets.iter().map(|t| t.text.as_str()).collect()
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
        let targets = link_targets(None, &rows, "", &IrcServerFeatures::with_chantypes("#&"));
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
        let targets = link_targets(Some("docs at https://docs.example/"), &rows, "", &IrcServerFeatures::with_chantypes("#&"));
        assert_eq!(targets[0].text, "https://docs.example/");
        assert_eq!(targets[0].row, None);
        assert_eq!(targets.len(), 4);
    }

    #[test]
    fn query_filters_by_url_case_insensitively() {
        let rows = transcript();
        let targets = link_targets(None, &rows, "  B.EXAMPLE ", &IrcServerFeatures::with_chantypes("#&"));
        assert_eq!(urls(&targets), ["https://b.example/two"]);
    }

    #[test]
    fn event_and_separator_rows_contribute_nothing() {
        let mut event = row_from_message(9, "", "", "https://ignored.example/", false);
        event.kind = RowKind::Event;
        let rows = vec![event];
        assert!(link_targets(None, &rows, "", &IrcServerFeatures::with_chantypes("#&")).is_empty());
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
        let targets = link_targets(None, &rows, "hbt", &IrcServerFeatures::with_chantypes("#&"));
        assert_eq!(urls(&targets), ["https://b.example/two"]);
        let picked: Vec<char> = targets[0]
            .indices
            .iter()
            .map(|&at| targets[0].text[at..].chars().next().unwrap())
            .collect();
        assert_eq!(picked, ['h', 'b', 't']);
    }

    #[test]
    fn a_tie_keeps_the_topic_link_first() {
        let url = "https://docs.example/";
        let rows = vec![row_from_message(1, "anna", "09:41", url, false)];
        let targets = link_targets(Some(url), &rows, "docs", &IrcServerFeatures::with_chantypes("#&"));
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
        assert!(link_targets(None, &[], "", &IrcServerFeatures::with_chantypes("#&")).is_empty());
        assert!(link_targets(Some("plain topic"), &[], "", &IrcServerFeatures::with_chantypes("#&")).is_empty());
    }

    fn status_row(id: u64, label: &str, text: &str) -> Row {
        let entry = StatusEntry {
            label: label.to_string(),
            source: LogSource::Server,
            ..outcome("omarchy", text, chrono::Utc::now())
        };
        row_from_status_entry(id, &entry)
    }

    fn kinds_and_texts(targets: &[LinkTarget]) -> Vec<(LinkKind, &str)> {
        targets.iter().map(|t| (t.kind, t.text.as_str())).collect()
    }

    #[test]
    fn status_lists_urls_newest_first_with_invites_in_their_row() {
        let rows = vec![
            status_row(1, "NOTICE", "see https://a.example/one"),
            status_row(2, "INVITE", "rio invited you to #debian"),
            status_row(3, "001", "nothing here"),
            status_row(
                4,
                "002",
                "two: https://b.example/two and https://c.example/three",
            ),
        ];
        let targets = status_link_targets(&rows, "");
        assert_eq!(
            kinds_and_texts(&targets),
            [
                (LinkKind::Url, "https://c.example/three"),
                (LinkKind::Url, "https://b.example/two"),
                (LinkKind::Invite, "#debian"),
                (LinkKind::Url, "https://a.example/one"),
            ]
        );
        assert_eq!(targets[2].row, Some(1));
        assert_eq!(targets[3].row, Some(0));
    }

    #[test]
    fn only_status_rows_feed_the_status_list() {
        let conversation = row_from_message(
            1,
            "rio",
            "09:00",
            "rio invited you to #x and https://x.example/",
            false,
        );
        assert!(status_link_targets(&[conversation], "").is_empty());
    }

    #[test]
    fn invite_needs_the_invite_label_not_just_the_marker() {
        let rows = vec![status_row(1, "NOTICE", "rio invited you to #debian")];
        assert!(status_link_targets(&rows, "").is_empty());
    }

    #[test]
    fn invite_channel_is_the_text_after_the_last_marker() {
        assert_eq!(
            invite_channel("rio invited you to #debian"),
            Some("#debian")
        );
        assert_eq!(invite_channel("rio invited you to "), None);
        assert_eq!(invite_channel("no marker here"), None);
        assert_eq!(
            invite_channel("a invited you to #one, b invited you to #two"),
            Some("#two")
        );
        assert_eq!(invite_channel("x invited you to #ümlaut"), Some("#ümlaut"));
    }

    #[test]
    fn invite_hit_covers_the_channel_at_the_end_of_the_line() {
        let row = status_row(1, "INVITE", "rio invited you to #debian");
        let (channel, range) = invite_hit(&row).expect("invite hit");
        assert_eq!(channel, "#debian");
        assert_eq!(&row.body.plain[range], "#debian");
        assert!(invite_hit(&status_row(2, "NOTICE", "rio invited you to #debian")).is_none());
        assert!(invite_hit(&status_row(3, "INVITE", "rio invited you to ")).is_none());
        let conversation = row_from_message(4, "rio", "09:00", "rio invited you to #x", false);
        assert!(invite_hit(&conversation).is_none());
    }

    #[test]
    fn invite_query_is_fuzzy_over_the_channel_name() {
        let rows = vec![status_row(1, "INVITE", "rio invited you to #debian")];
        let targets = status_link_targets(&rows, "dbn");
        assert_eq!(kinds_and_texts(&targets), [(LinkKind::Invite, "#debian")]);
        let picked: Vec<char> = targets[0]
            .indices
            .iter()
            .map(|&at| targets[0].text[at..].chars().next().unwrap())
            .collect();
        assert_eq!(picked, ['d', 'b', 'n']);
        assert!(status_link_targets(&rows, "zzz").is_empty());
    }

    #[test]
    fn empty_status_has_no_links() {
        assert!(status_link_targets(&[], "").is_empty());
    }

    #[test]
    fn a_line_lists_its_urls_then_its_channels_newest_row_first() {
        let rows = vec![
            row_from_message(1, "anna", "09:41", "join #desktop", false),
            row_from_message(
                2,
                "dax",
                "09:43",
                "see https://b.example/x in #lab and #ricing",
                false,
            ),
        ];
        let targets = link_targets(None, &rows, "", &IrcServerFeatures::with_chantypes("#&"));
        assert_eq!(
            kinds_and_texts(&targets),
            [
                (LinkKind::Url, "https://b.example/x"),
                (LinkKind::Channel, "#ricing"),
                (LinkKind::Channel, "#lab"),
                (LinkKind::Channel, "#desktop"),
            ]
        );
        assert_eq!(targets[1].row, Some(1));
        assert_eq!(targets[3].row, Some(0));
    }

    #[test]
    fn the_topic_lists_its_channels_after_its_urls_with_no_row() {
        let rows = Vec::new();
        let targets = link_targets(Some("about #lab, https://docs.example/"), &rows, "", &IrcServerFeatures::with_chantypes("#&"));
        assert_eq!(
            kinds_and_texts(&targets),
            [
                (LinkKind::Url, "https://docs.example/"),
                (LinkKind::Channel, "#lab"),
            ]
        );
        assert!(targets.iter().all(|t| t.row.is_none()));
    }

    #[test]
    fn a_channel_inside_a_url_is_not_listed() {
        let rows = vec![row_from_message(
            1,
            "anna",
            "09:41",
            "https://x.example/(#chan)",
            false,
        )];
        let targets = link_targets(None, &rows, "", &IrcServerFeatures::with_chantypes("#&"));
        assert_eq!(
            kinds_and_texts(&targets),
            [(LinkKind::Url, "https://x.example/(#chan)")]
        );
    }

    #[test]
    fn a_whois_line_lists_its_links_and_an_event_line_does_not() {
        let mut whois = row_from_message(
            1,
            "",
            "",
            "VERSION reply from anna: https://omairc.app",
            false,
        );
        whois.kind = RowKind::Whois;
        let mut event = row_from_message(2, "", "", "#lab joined https://ignored.example/", false);
        event.kind = RowKind::Event;
        let targets = link_targets(None, &[whois, event], "", &IrcServerFeatures::with_chantypes("#&"));
        assert_eq!(
            kinds_and_texts(&targets),
            [(LinkKind::Url, "https://omairc.app")]
        );
    }

    #[test]
    fn a_channel_row_fuzzy_matches_its_name() {
        let rows = vec![row_from_message(
            1,
            "anna",
            "09:41",
            "meet in #desktop",
            false,
        )];
        let targets = link_targets(None, &rows, "dsk", &IrcServerFeatures::with_chantypes("#&"));
        assert_eq!(kinds_and_texts(&targets), [(LinkKind::Channel, "#desktop")]);
        let picked: Vec<char> = targets[0]
            .indices
            .iter()
            .map(|&at| targets[0].text[at..].chars().next().unwrap())
            .collect();
        assert_eq!(picked, ['d', 's', 'k']);
    }

    #[test]
    fn status_has_no_channel_rows() {
        let rows = vec![status_row(1, "NOTICE", "see #lab and https://a.example/")];
        assert_eq!(
            kinds_and_texts(&status_link_targets(&rows, "")),
            [(LinkKind::Url, "https://a.example/")]
        );
    }

    #[test]
    fn the_seeded_channel_names_resolve_on_their_network() {
        use crate::model::transcript::rows_from_messages;

        let mut controller = Controller::seeded_for_tests();
        let features = controller.features("oftc").clone();
        let has_row = |controller: &Controller, target: &str| {
            controller
                .conversations()
                .iter()
                .any(|row| row.key.network_id == "oftc" && row.target == target)
        };

        // `This #omarchy is the OFTC one.` sits in OFTC's #omarchy, and names a row there.
        controller.select_conversation("oftc", "#omarchy");
        let names: Vec<String> = rows_from_messages(controller.messages(), None)
            .iter()
            .flat_map(|row| conversation_channel_hits(row, &features))
            .map(|span| span.name)
            .collect();
        assert!(names.iter().any(|name| name == "#omarchy"));
        assert!(has_row(&controller, "#omarchy"));

        // `Leave the log in #build` is in OFTC's #lab; #build is a row too.
        controller.select_conversation("oftc", "#lab");
        let hits: Vec<String> = rows_from_messages(controller.messages(), None)
            .iter()
            .flat_map(|row| conversation_channel_hits(row, &features))
            .map(|span| span.name)
            .collect();
        assert_eq!(hits, ["#build"]);
        assert!(has_row(&controller, "#build"));
    }

    #[test]
    fn conversation_channel_hits_are_for_chat_lines_only() {
        let chat = row_from_message(1, "anna", "09:41", "in #lab", false);
        let hits = conversation_channel_hits(&chat, &IrcServerFeatures::with_chantypes("#&"));
        assert_eq!(hits.len(), 1);
        assert_eq!(&chat.body.plain[hits[0].range.clone()], "#lab");
        let status = status_row(2, "NOTICE", "in #lab");
        assert!(conversation_channel_hits(&status, &IrcServerFeatures::with_chantypes("#&")).is_empty());
    }
}
