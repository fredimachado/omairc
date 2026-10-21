//! Mention washes: a chat line whose body names the local nick is tinted, and
//! its left edge gets a bar. Port of `containsWord` and `isNickMention` in
//! `src/irc/irceventreducer.cpp`, and the look of `src/qml/MentionWash.qml`.
//! The washed rows are the chat lines that someone else wrote; a whois or an
//! event line never washes, and a DM washes only on a body hit.

use super::color::Color;
use super::style::{mix_colors, Style};
use super::transcript::{Row, RowKind};

/// Qt's `isIdentifierCharacter`: letters, digits, and the punctuation IRC nicks
/// may use. A mention must be bounded by something else.
fn is_identifier_char(ch: char) -> bool {
    ch.is_alphanumeric() || "-_[]\\`^{}|~".contains(ch)
}

/// Whether `body` holds `nick` as a whole word, ignoring case. The characters
/// on each side of the match must not be identifier characters, so `fred`
/// matches `fred:` and `fred's` but not `fredo` or `fred-bot`.
pub fn mentions_nick(body: &str, nick: &str) -> bool {
    let needle: Vec<char> = nick.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return false;
    }
    let haystack: Vec<char> = body.chars().flat_map(char::to_lowercase).collect();
    if haystack.len() < needle.len() {
        return false;
    }
    (0..=haystack.len() - needle.len()).any(|at| {
        let end = at + needle.len();
        haystack[at..end] == needle[..]
            && (at == 0 || !is_identifier_char(haystack[at - 1]))
            && haystack
                .get(end)
                .is_none_or(|&after| !is_identifier_char(after))
    })
}

/// Whether a transcript row is washed for `self_nick`. Only a message or action
/// from someone else can be washed, and only when its body names the nick.
pub fn is_mention_row(row: &Row, self_nick: &str) -> bool {
    if row.kind != RowKind::Message || self_nick.is_empty() {
        return false;
    }
    let from_self = row
        .author
        .as_deref()
        .is_some_and(|author| author.eq_ignore_ascii_case(self_nick));
    !from_self && mentions_nick(&row.body.plain, self_nick)
}

/// The wash fill and the left bar for a mention row (`MentionWash.qml`): the
/// accent mixed into the page, a little more of it in dark mode for the bar.
pub fn mention_wash(style: &Style) -> (Color, Color) {
    let dark = style.dark_mode();
    let wash = mix_colors(
        style.page_color(),
        style.accent_color(),
        if dark { 0.09 } else { 0.06 },
    );
    let bar = mix_colors(
        style.page_color(),
        style.accent_color(),
        if dark { 0.44 } else { 0.34 },
    );
    (wash, bar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::controller::Controller;
    use crate::irc::event::MsgId;
    use crate::irc::world::{Message, MessageKind, Origin};
    use chrono::{Local, TimeZone, Utc};
    use crate::model::transcript::{row_from_message, rows_from_messages};

    #[test]
    fn the_nick_must_be_a_whole_word() {
        assert!(mentions_nick("looks good, fred.", "fred"));
        assert!(mentions_nick("fred: hi", "fred"));
        assert!(mentions_nick("@fred", "fred"));
        assert!(mentions_nick("fred's", "fred"));
        // `[` and `]` are nick characters in Qt's identifier set, so they bind a word.
        assert!(!mentions_nick("[fred]", "fred"));
        assert!(!mentions_nick("fredo", "fred"));
        assert!(!mentions_nick("alfred", "fred"));
        assert!(!mentions_nick("fred-bot", "fred"));
        assert!(!mentions_nick("fred_", "fred"));
        assert!(!mentions_nick("fred|away", "fred"));
    }

    #[test]
    fn the_match_ignores_case() {
        assert!(mentions_nick("Fred, look", "fred"));
        assert!(mentions_nick("hi FRED", "Fred"));
        assert!(mentions_nick("ÄNNA here", "änna"));
    }

    #[test]
    fn an_empty_nick_matches_nothing() {
        assert!(!mentions_nick("anything", ""));
        assert!(!mentions_nick("", "fred"));
    }

    #[test]
    fn a_body_hit_at_a_later_position_is_found() {
        assert!(mentions_nick("fredo and fred", "fred"));
        assert!(mentions_nick("nope fred", "fred"));
    }

    #[test]
    fn only_chat_lines_from_someone_else_wash() {
        let message = row_from_message(1, "mira", "09:41", "hi fred", false);
        assert!(is_mention_row(&message, "fred"));

        let own = row_from_message(2, "fred", "09:42", "hi fred", false);
        assert!(!is_mention_row(&own, "fred"), "own lines never wash");

        let mut action = row_from_message(3, "mira", "09:43", "waves at fred", false);
        action.action = true;
        assert!(
            is_mention_row(&action, "fred"),
            "an action washes like a message"
        );

        let mut whois = row_from_message(4, "mira", "09:44", "fred", false);
        whois.kind = RowKind::Whois;
        assert!(!is_mention_row(&whois, "fred"));

        let mut event = row_from_message(5, "mira", "", "fred", false);
        event.kind = RowKind::Event;
        assert!(!is_mention_row(&event, "fred"));

        assert!(!is_mention_row(&message, ""), "no nick, no wash");
    }

    #[test]
    fn the_seeded_ricing_mention_is_the_one_washed_row() {
        let mut controller = Controller::seeded_for_tests();
        controller.select_conversation("omarchy", "#ricing");
        let rows = rows_from_messages(controller.messages(), None);
        let washed: Vec<&Row> = rows
            .iter()
            .filter(|row| is_mention_row(row, "fred"))
            .collect();
        assert_eq!(washed.len(), 1);
        assert_eq!(washed[0].body.plain, "looks good, fred.");
    }

    #[test]
    fn a_direct_message_washes_only_on_a_body_hit() {
        let at = |minute: u32| {
            Local
                .with_ymd_and_hms(2026, 10, 19, 9, minute, 0)
                .unwrap()
                .with_timezone(&Utc)
        };
        let line = |body: &str, minute: u32| Message {
            kind: MessageKind::Message,
            author: "anna".to_string(),
            body: body.to_string(),
            time: at(minute),
            server_time: None,
            msgid: MsgId::default(),
            origin: Origin::Live,
            mentioned: false,
            collapsible: false,
            sequence: minute as i64,
            only_visible_to_you: false,
        };
        let rows = rows_from_messages(&[line("plain dm", 0), line("fred, you there?", 1)], None);
        let chats: Vec<&Row> = rows.iter().filter(|row| row.kind == RowKind::Message).collect();
        assert!(!is_mention_row(chats[0], "fred"));
        assert!(is_mention_row(chats[1], "fred"));
    }

    #[test]
    fn the_bar_and_the_wash_are_distinct_fills() {
        let style = Style::new(crate::model::theme::Theme::defaults_light(), 1.0);
        let (wash, bar) = mention_wash(&style);
        assert_ne!(wash, bar);
        assert_ne!(wash, style.page_color());
    }
}
