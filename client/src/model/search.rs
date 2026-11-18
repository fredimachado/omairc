//! History search (Ctrl+Shift+F): what a query matches, how hits from the loaded
//! transcripts and the on-disk logs merge into one newest-first list, and where Enter
//! lands. Toolkit-free and free of IO: the shell feeds it lines and rows.
//!
//! A line matches as Ctrl+F matches a row: the case-folded text `"{author} {plain body}"`
//! holds the case-folded query (`find::fold`). Only the plain text is searched, never the
//! wire body, so a mIRC control code is never matched and never shown.
//!
//! `tui/internal/irc/search.go` holds the same shapes and rules.

use std::cmp::Reverse;
use std::collections::HashSet;
use std::ops::Range;

use chrono::{DateTime, TimeZone, Utc};

use crate::irc::event::ConversationKey;
use crate::irc::plaintext::plain_irc_text;
use crate::model::find::fold;
use crate::model::scroll_place::{line_row, SavedPlace};
use crate::model::transcript::{minute_of, Row};
use crate::storage::scrollplace::ScrollPlace as StoredPlace;

/// The most hits one search lists. A search that matched more says so (`SearchResults::more`).
pub const SEARCH_CAP: usize = 200;

/// What the search sheet's field asks for: the trimmed text, case-folded once. An empty
/// field is no search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchQuery {
    text: String,
    folded: String,
}

impl SearchQuery {
    /// The query for what was typed, or `None` while the field is blank.
    pub fn parse(raw: &str) -> Option<Self> {
        let text = raw.trim();
        (!text.is_empty()).then(|| Self {
            text: text.to_owned(),
            folded: fold(text),
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether the line matches, and where in its body. See [`matches_line`].
    pub fn matches_line(&self, author: &str, plain_body: &str) -> Option<LineMatch> {
        let author_folded = fold(author);
        let body_folded = fold(plain_body);
        // Ctrl+F's text, so a query that crosses from the nick into the body matches too.
        let search_text = format!("{author_folded} {body_folded}");
        if !search_text.contains(&self.folded) {
            return None;
        }
        Some(LineMatch {
            matches: body_ranges(plain_body, &self.folded),
            author_match: author_folded.contains(&self.folded),
        })
    }
}

/// How a line matched. `matches` are byte ranges into the plain body, on char boundaries.
/// A query that matched only the author, or only across the nick and the body, has none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineMatch {
    pub matches: Vec<Range<usize>>,
    pub author_match: bool,
}

/// Whether `query` matches a line from `author` with the plain body `plain_body`. An empty
/// query matches nothing.
pub fn matches_line(query: &str, author: &str, plain_body: &str) -> Option<LineMatch> {
    SearchQuery::parse(query)?.matches_line(author, plain_body)
}

/// The byte ranges of `needle` (already folded) in `body`, ignoring case. Folding a char can
/// change its length (`İ` folds to two chars), so the match runs over the folded chars and
/// each range maps back to the whole original chars it covers.
fn body_ranges(body: &str, needle: &str) -> Vec<Range<usize>> {
    let needle: Vec<char> = needle.chars().collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut folded: Vec<(char, usize, usize)> = Vec::with_capacity(body.len());
    for (start, ch) in body.char_indices() {
        let end = start + ch.len_utf8();
        folded.extend(ch.to_lowercase().map(|lower| (lower, start, end)));
    }
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut at = 0;
    while at + needle.len() <= folded.len() {
        let window = &folded[at..at + needle.len()];
        if window.iter().map(|folded| folded.0).eq(needle.iter().copied()) {
            let range = window[0].1..window[window.len() - 1].2;
            match ranges.last_mut() {
                // Two matches that touch the same original char are one highlight.
                Some(last) if range.start < last.end => last.end = last.end.max(range.end),
                _ => ranges.push(range),
            }
            at += needle.len();
        } else {
            at += 1;
        }
    }
    ranges
}

/// Where Enter lands for a hit. A msgid names the line when the server gave one. Otherwise
/// the line is found by its content, the way a saved place without a msgid is.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum JumpTarget {
    /// Never empty.
    Msgid(String),
    /// The saved place's fields: author, kind token, plain body and the time in ms since the
    /// epoch (the log stores it to the millisecond; the transcript shows its local minute).
    Anchor {
        author: String,
        kind: String,
        body: String,
        ms: i64,
    },
}

impl JumpTarget {
    fn for_line(msgid: &str, author: &str, kind: &str, plain_body: &str, time: DateTime<Utc>) -> Self {
        if msgid.is_empty() {
            Self::Anchor {
                author: author.to_owned(),
                kind: kind.to_owned(),
                body: plain_body.to_owned(),
                ms: time.timestamp_millis(),
            }
        } else {
            Self::Msgid(msgid.to_owned())
        }
    }

    /// The saved place the transcript restores to land on this line. `SavedPlace` is the one
    /// way a restore names a line, so a jump is a restore of this place.
    pub fn saved_place(&self) -> SavedPlace {
        let stored = match self {
            Self::Msgid(msgid) => StoredPlace {
                msg_id: msgid.clone(),
                ..StoredPlace::default()
            },
            Self::Anchor {
                author, body, ms, ..
            } => StoredPlace {
                // Every chat kind is a `message` row in the transcript.
                kind: "message".to_owned(),
                author: author.clone(),
                body: body.clone(),
                shown_time: Utc
                    .timestamp_millis_opt(*ms)
                    .single()
                    .map(minute_of)
                    .unwrap_or_default(),
                ..StoredPlace::default()
            },
        };
        SavedPlace::from_stored(&stored).unwrap_or(SavedPlace::End)
    }
}

/// One matching line, ready to draw and to jump to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
    pub network_id: String,
    pub network_name: String,
    /// The conversation's identity. Never read a conversation back from its label.
    pub conversation: ConversationKey,
    /// What the row shows for the conversation: the channel name or the peer's nick.
    pub conversation_label: String,
    pub time: DateTime<Utc>,
    pub author: String,
    /// The plain text of the body.
    pub text: String,
    /// Byte ranges into `text` to highlight.
    pub matches: Vec<Range<usize>>,
    /// The query matched the nick. Nothing in `text` is highlighted for it.
    pub author_match: bool,
    pub target: JumpTarget,
}

/// The conversation a line was found in. Both sources (loaded transcripts and logs) build
/// their hits through it, so a hit is made one way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchConversation {
    pub network_id: String,
    pub network_name: String,
    pub conversation: ConversationKey,
    pub label: String,
}

/// One stored or loaded line, as the sources hand it over.
pub struct SearchLine<'a> {
    pub time: DateTime<Utc>,
    pub author: &'a str,
    /// The stored kind token: `message`, `notice`, `action` or `event`.
    pub kind: &'a str,
    /// The body as it came off the wire or the log, mIRC codes and all.
    pub body: &'a str,
    pub msgid: &'a str,
}

impl SearchConversation {
    /// The hit for `line`, when it is a chat line the query matches. Joins, parts and the
    /// like are not searched: they are noise a person looking for something said never wants.
    pub fn hit(&self, query: &SearchQuery, line: &SearchLine<'_>) -> Option<SearchHit> {
        if !matches!(line.kind, "message" | "notice" | "action") {
            return None;
        }
        let text = plain_irc_text(line.body);
        let found = query.matches_line(line.author, &text)?;
        // The log keeps the time to the millisecond. A loaded line may carry more, and the
        // same line from both sources must compare equal.
        let time = Utc
            .timestamp_millis_opt(line.time.timestamp_millis())
            .single()
            .unwrap_or(line.time);
        Some(SearchHit {
            network_id: self.network_id.clone(),
            network_name: self.network_name.clone(),
            conversation: self.conversation.clone(),
            conversation_label: self.label.clone(),
            time,
            author: line.author.to_owned(),
            target: JumpTarget::for_line(line.msgid, line.author, line.kind, &text, time),
            text,
            matches: found.matches,
            author_match: found.author_match,
        })
    }
}

/// The list the sheet shows: the newest [`SEARCH_CAP`] hits, and whether more matched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub more: bool,
}

impl SearchResults {
    /// Folds a batch of hits into the list. Used as hits stream in: the list stays the
    /// newest `cap` of everything seen, and `more` stays set once it is.
    pub fn absorb(&mut self, batch: Vec<SearchHit>, cap: usize) {
        let held = std::mem::take(&mut self.hits);
        let merged = merge_results(vec![held, batch], cap);
        self.more |= merged.more;
        self.hits = merged.hits;
    }
}

/// Merges the hits of every source, newest first, and keeps the first `cap`. Ties on the
/// time go to the lower network id, then conversation, then the source's own order, so the
/// order never depends on which source answered first. A line two sources both found (the
/// same target in the same conversation at the same time) counts once, the earlier source's.
pub fn merge_results(sources: Vec<Vec<SearchHit>>, cap: usize) -> SearchResults {
    let mut seen: HashSet<(ConversationKey, JumpTarget, DateTime<Utc>)> = HashSet::new();
    let mut all: Vec<(usize, usize, SearchHit)> = Vec::new();
    for (source, hits) in sources.into_iter().enumerate() {
        for (index, hit) in hits.into_iter().enumerate() {
            let line = (hit.conversation.clone(), hit.target.clone(), hit.time);
            if seen.insert(line) {
                all.push((source, index, hit));
            }
        }
    }
    all.sort_by(|a, b| {
        (Reverse(a.2.time), &a.2.network_id, &a.2.conversation, a.0, a.1).cmp(&(
            Reverse(b.2.time),
            &b.2.network_id,
            &b.2.conversation,
            b.0,
            b.1,
        ))
    });
    let more = all.len() > cap;
    all.truncate(cap);
    SearchResults {
        hits: all.into_iter().map(|(_, _, hit)| hit).collect(),
        more,
    }
}

/// The note a jump shows when its line is older than the transcript the conversation has
/// loaded (`JumpOutcome::OlderThanLoaded`).
pub const OLDER_THAN_LOADED_NOTE: &str = "That message is older than the loaded history.";

/// The footer under the result list, from where the search stands.
pub fn footer_text(query_blank: bool, results: &SearchResults, scanning: bool) -> String {
    if query_blank {
        return "Type to search every conversation".to_owned();
    }
    let count = results.hits.len();
    if results.more {
        return format!("{SEARCH_CAP}+ results, refine your search");
    }
    match (count, scanning) {
        (0, true) => "Searching…".to_owned(),
        (0, false) => "No results".to_owned(),
        (1, true) => "1 result · Searching…".to_owned(),
        (1, false) => "1 result".to_owned(),
        (n, true) => format!("{n} results · Searching…"),
        (n, false) => format!("{n} results"),
    }
}

/// The time a row shows: the local `HH:MM` for a line from `today`, else the short date in
/// front of it.
pub fn time_label(time: DateTime<Utc>, today: chrono::NaiveDate) -> String {
    use chrono::Local;
    let local = time.with_timezone(&Local);
    if local.date_naive() == today {
        local.format("%H:%M").to_string()
    } else {
        local.format("%b %-d %H:%M").to_string()
    }
}

/// The row of `rows` the target names, by msgid or by content.
pub fn resolve_jump(target: &JumpTarget, rows: &[Row]) -> Option<usize> {
    line_row(&target.saved_place(), rows)
}

/// What opening a hit did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JumpOutcome {
    /// The row the line sits on.
    Landed(usize),
    /// The line is on disk, older than the transcript the conversation has loaded. The
    /// conversation opens as it would without a search, and says so.
    OlderThanLoaded,
}

pub fn jump_outcome(target: &JumpTarget, rows: &[Row]) -> JumpOutcome {
    match resolve_jump(target, rows) {
        Some(row) => JumpOutcome::Landed(row),
        None => JumpOutcome::OlderThanLoaded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::transcript::row_from_message;

    fn at(minute: i64) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(1_700_000_000_000 + minute * 60_000)
            .unwrap()
    }

    fn room(network: &str, target: &str) -> SearchConversation {
        SearchConversation {
            network_id: network.to_owned(),
            network_name: network.to_owned(),
            conversation: ConversationKey::new(network, target),
            label: target.to_owned(),
        }
    }

    fn line<'a>(minute: i64, author: &'a str, body: &'a str, msgid: &'a str) -> SearchLine<'a> {
        SearchLine {
            time: at(minute),
            author,
            kind: "message",
            body,
            msgid,
        }
    }

    fn hit(network: &str, target: &str, minute: i64, body: &str) -> SearchHit {
        let query = SearchQuery::parse("x").unwrap();
        let mut hit = room(network, target)
            .hit(&query, &line(minute, "anna", &format!("x {body}"), ""))
            .expect("the line holds the query");
        hit.text = body.to_owned();
        hit
    }

    // matching

    #[test]
    fn a_query_matches_ignoring_case_in_both_directions() {
        assert!(matches_line("BUILD", "dax", "the build is green").is_some());
        assert!(matches_line("build", "dax", "The BUILD is green").is_some());
    }

    #[test]
    fn a_blank_query_matches_nothing() {
        assert!(matches_line("", "anna", "hello").is_none());
        assert!(matches_line("   ", "anna", "hello").is_none());
        assert!(SearchQuery::parse("  \t").is_none());
    }

    #[test]
    fn the_query_is_trimmed() {
        let query = SearchQuery::parse("  build  ").unwrap();
        assert_eq!(query.text(), "build");
        assert!(query.matches_line("dax", "the build is green").is_some());
    }

    #[test]
    fn the_author_matches_without_marking_the_text() {
        let found = matches_line("anna", "Anna", "tea?").expect("the nick matches");
        assert!(found.author_match);
        assert!(found.matches.is_empty());
    }

    #[test]
    fn a_body_match_marks_every_occurrence_and_not_the_author() {
        let found = matches_line("go", "dax", "Go go gadget").unwrap();
        assert!(!found.author_match);
        assert_eq!(found.matches, vec![0..2, 3..5]);
    }

    #[test]
    fn a_phrase_that_crosses_from_the_nick_into_the_body_matches_as_ctrl_f_does() {
        let found = matches_line("anna h", "anna", "hello").expect("find mode matches it");
        assert!(found.matches.is_empty());
    }

    #[test]
    fn mirc_codes_are_not_text() {
        let query = SearchQuery::parse("bold").unwrap();
        let conversation = room("omarchy", "#ricing");
        let found = conversation
            .hit(&query, &line(0, "anna", "\x02bold\x02 and \x0304,05red\x03", ""))
            .expect("the plain text holds bold");
        assert_eq!(found.text, "bold and red");
        assert_eq!(found.matches, vec![0..4]);
        let code = SearchQuery::parse("\x02").unwrap();
        assert!(conversation
            .hit(&code, &line(0, "anna", "\x02bold\x02", ""))
            .is_none());
        let colour = SearchQuery::parse("04,05").unwrap();
        assert!(conversation
            .hit(&colour, &line(0, "anna", "\x0304,05red\x03", ""))
            .is_none());
    }

    #[test]
    fn matching_folds_non_ascii_case() {
        let found = matches_line("CAFÉ", "anna", "un Café au lait").unwrap();
        assert_eq!(found.matches, vec![3..8]);
    }

    #[test]
    fn a_range_stays_on_char_boundaries_when_folding_changes_a_length() {
        // `İ` (two bytes) folds to `i` and a combining dot: two chars, three bytes.
        let body = "xİx";
        let found = matches_line("x", "anna", body).unwrap();
        assert_eq!(found.matches, vec![0..1, 3..4]);
        for range in &found.matches {
            assert!(body.is_char_boundary(range.start) && body.is_char_boundary(range.end));
        }
        // A query that ends inside the expansion highlights the whole original char.
        let found = matches_line("xi", "anna", body).unwrap();
        assert_eq!(found.matches, vec![0..3]);
    }

    #[test]
    fn a_hit_keeps_the_time_to_the_millisecond_the_log_keeps() {
        let query = SearchQuery::parse("tea").unwrap();
        let mut loaded = line(1, "anna", "tea", "m-1");
        loaded.time += chrono::Duration::nanoseconds(123_456);
        let found = room("omarchy", "#ricing").hit(&query, &loaded).unwrap();
        assert_eq!(found.time, at(1));
    }

    #[test]
    fn only_chat_lines_are_searched() {
        let query = SearchQuery::parse("joined").unwrap();
        let conversation = room("omarchy", "#ricing");
        let mut event = line(0, "", "anna joined", "");
        event.kind = "event";
        assert!(conversation.hit(&query, &event).is_none());
        for kind in ["message", "notice", "action"] {
            let mut chat = line(0, "anna", "she joined late", "");
            chat.kind = kind;
            assert!(conversation.hit(&query, &chat).is_some(), "{kind}");
        }
    }

    // ordering and the cap

    #[test]
    fn hits_merge_newest_first_across_conversations_and_sources() {
        let memory = vec![hit("omarchy", "#ricing", 5, "memory five")];
        let disk = vec![
            hit("omarchy", "anna", 9, "disk nine"),
            hit("omarchy", "#ricing", 2, "disk two"),
        ];
        let merged = merge_results(vec![memory, disk], SEARCH_CAP);
        let texts: Vec<_> = merged.hits.iter().map(|hit| hit.text.as_str()).collect();
        assert_eq!(texts, ["disk nine", "memory five", "disk two"]);
        assert!(!merged.more);
    }

    #[test]
    fn equal_times_order_by_network_conversation_then_source_order() {
        let first = merge_results(
            vec![vec![
                hit("oftc", "#a", 1, "oftc a"),
                hit("libera", "#z", 1, "libera z"),
                hit("libera", "#b", 1, "libera b one"),
                hit("libera", "#b", 1, "libera b two"),
            ]],
            SEARCH_CAP,
        );
        let texts: Vec<_> = first.hits.iter().map(|hit| hit.text.as_str()).collect();
        assert_eq!(texts, ["libera b one", "libera b two", "libera z", "oftc a"]);
        // The same hits arriving the other way round list the same way.
        let mut reversed = vec![
            hit("oftc", "#a", 1, "oftc a"),
            hit("libera", "#z", 1, "libera z"),
        ];
        reversed.reverse();
        let again = merge_results(vec![reversed], SEARCH_CAP);
        assert_eq!(again.hits[0].text, "libera z");
    }

    #[test]
    fn two_hundred_and_fifty_matches_keep_the_newest_two_hundred_and_say_more() {
        let hits: Vec<_> = (0..250)
            .map(|minute| hit("omarchy", "#ricing", minute, &format!("line {minute}")))
            .collect();
        let merged = merge_results(vec![hits], SEARCH_CAP);
        assert_eq!(merged.hits.len(), SEARCH_CAP);
        assert!(merged.more);
        assert_eq!(merged.hits[0].text, "line 249");
        assert_eq!(merged.hits[SEARCH_CAP - 1].text, "line 50");
    }

    #[test]
    fn exactly_two_hundred_matches_say_nothing_more() {
        let hits: Vec<_> = (0..200)
            .map(|minute| hit("omarchy", "#ricing", minute, &format!("line {minute}")))
            .collect();
        let merged = merge_results(vec![hits], SEARCH_CAP);
        assert_eq!(merged.hits.len(), 200);
        assert!(!merged.more);
    }

    #[test]
    fn a_line_both_sources_found_counts_once() {
        let query = SearchQuery::parse("tea").unwrap();
        let conversation = room("omarchy", "#ricing");
        let by_msgid = |source| {
            vec![conversation
                .hit(&query, &line(3, "anna", &format!("tea {source}"), "id-1"))
                .unwrap()]
        };
        // The same msgid at the same time: the memory copy wins.
        let merged = merge_results(vec![by_msgid("memory"), by_msgid("disk")], SEARCH_CAP);
        assert_eq!(merged.hits.len(), 1);
        assert!(merged.hits[0].text.contains("memory"));
        // Two lines with no msgid at the same time and body are the same line.
        let anchored = || vec![conversation.hit(&query, &line(4, "anna", "tea", "")).unwrap()];
        assert_eq!(merge_results(vec![anchored(), anchored()], SEARCH_CAP).hits.len(), 1);
        // The same words a minute apart are two lines.
        let later = vec![conversation.hit(&query, &line(5, "anna", "tea", "")).unwrap()];
        assert_eq!(merge_results(vec![anchored(), later], SEARCH_CAP).hits.len(), 2);
    }

    #[test]
    fn absorbing_batches_keeps_the_newest_and_remembers_more() {
        let mut results = SearchResults::default();
        results.absorb(
            (0..150).map(|n| hit("omarchy", "#a", n, &format!("a{n}"))).collect(),
            SEARCH_CAP,
        );
        assert!(!results.more);
        results.absorb(
            (100..250).map(|n| hit("omarchy", "#b", n, &format!("b{n}"))).collect(),
            SEARCH_CAP,
        );
        assert_eq!(results.hits.len(), SEARCH_CAP);
        assert!(results.more);
        assert_eq!(results.hits[0].text, "b249");
        results.absorb(Vec::new(), SEARCH_CAP);
        assert!(results.more, "an empty batch does not forget the overflow");
    }

    // the jump target

    #[test]
    fn a_line_with_a_msgid_jumps_by_msgid() {
        let query = SearchQuery::parse("tea").unwrap();
        let found = room("omarchy", "#ricing")
            .hit(&query, &line(1, "anna", "tea?", "abc123"))
            .unwrap();
        assert_eq!(found.target, JumpTarget::Msgid("abc123".to_owned()));
    }

    #[test]
    fn a_line_without_a_msgid_jumps_by_its_anchor_fields() {
        let query = SearchQuery::parse("tea").unwrap();
        let found = room("omarchy", "#ricing")
            .hit(&query, &line(1, "anna", "\x02tea\x02?", ""))
            .unwrap();
        assert_eq!(
            found.target,
            JumpTarget::Anchor {
                author: "anna".to_owned(),
                kind: "message".to_owned(),
                body: "tea?".to_owned(),
                ms: at(1).timestamp_millis(),
            }
        );
    }

    fn chat_row(id: u64, author: &str, time: DateTime<Utc>, body: &str, msgid: &str) -> Row {
        let mut row = row_from_message(id, author, &minute_of(time), body, false);
        row.msgid = msgid.to_owned();
        row
    }

    #[test]
    fn a_jump_finds_its_row_by_msgid() {
        let rows = vec![
            chat_row(1, "anna", at(0), "morning", "m-1"),
            chat_row(2, "dax", at(1), "tea?", "m-2"),
        ];
        let target = JumpTarget::Msgid("m-2".to_owned());
        assert_eq!(resolve_jump(&target, &rows), Some(1));
        assert_eq!(jump_outcome(&target, &rows), JumpOutcome::Landed(1));
    }

    #[test]
    fn a_jump_finds_its_row_by_anchor_when_there_is_no_msgid() {
        let rows = vec![
            chat_row(1, "anna", at(0), "tea?", ""),
            chat_row(2, "dax", at(5), "tea?", ""),
            chat_row(3, "anna", at(9), "tea?", ""),
        ];
        let target = JumpTarget::Anchor {
            author: "anna".to_owned(),
            kind: "message".to_owned(),
            body: "tea?".to_owned(),
            ms: at(9).timestamp_millis(),
        };
        assert_eq!(resolve_jump(&target, &rows), Some(2));
    }

    #[test]
    fn a_line_that_is_not_loaded_is_older_than_the_loaded_history() {
        let rows = vec![chat_row(1, "anna", at(0), "morning", "m-1")];
        let by_id = JumpTarget::Msgid("gone".to_owned());
        assert_eq!(resolve_jump(&by_id, &rows), None);
        assert_eq!(jump_outcome(&by_id, &rows), JumpOutcome::OlderThanLoaded);
        let by_content = JumpTarget::Anchor {
            author: "anna".to_owned(),
            kind: "message".to_owned(),
            body: "an older line".to_owned(),
            ms: at(-500).timestamp_millis(),
        };
        assert_eq!(jump_outcome(&by_content, &rows), JumpOutcome::OlderThanLoaded);
        assert_eq!(jump_outcome(&by_content, &[]), JumpOutcome::OlderThanLoaded);
    }

    #[test]
    fn the_footer_says_where_the_search_stands() {
        let none = SearchResults::default();
        assert_eq!(footer_text(true, &none, false), "Type to search every conversation");
        assert_eq!(footer_text(false, &none, true), "Searching…");
        assert_eq!(footer_text(false, &none, false), "No results");
        let some = SearchResults {
            hits: vec![hit("omarchy", "#a", 1, "x"), hit("omarchy", "#a", 2, "y")],
            more: false,
        };
        assert_eq!(footer_text(false, &some, false), "2 results");
        assert_eq!(footer_text(false, &some, true), "2 results · Searching…");
        let more = SearchResults { more: true, ..some };
        assert_eq!(footer_text(false, &more, false), "200+ results, refine your search");
    }

    #[test]
    fn a_line_from_today_shows_only_its_minute() {
        use chrono::Local;
        let time = Local
            .with_ymd_and_hms(2026, 10, 19, 9, 5, 0)
            .unwrap()
            .with_timezone(&Utc);
        let day = |d| chrono::NaiveDate::from_ymd_opt(2026, 10, d).unwrap();
        assert_eq!(time_label(time, day(19)), "09:05");
        assert_eq!(time_label(time, day(20)), "Oct 19 09:05");
    }

    #[test]
    fn a_target_is_a_saved_place_the_transcript_can_restore() {
        let target = JumpTarget::Msgid("m-2".to_owned());
        assert!(matches!(target.saved_place(), SavedPlace::Line { .. }));
    }
}
