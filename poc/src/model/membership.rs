//! Join, part, quit and nick lines as the transcript shows them, under `/pref
//! joins`.
//!
//! The line formats, the noise setting and the caption fold are the protocol
//! vocabulary and live in `irc/membershipnoise` (Qt `src/irc/ircmembershipnoise.h`
//! and `foldMembership`). This module applies them to the stored lines of one
//! conversation. The PoC folds the stored lines each time a conversation is
//! shown, so a `/pref joins` change reaches the whole transcript at once. Qt
//! applies the setting only to lines that arrive after it.

use super::mock::MockMessage;
use crate::irc::membershipnoise::{
    fold_membership, join_line, part_line, quit_line, MembershipNoise,
};

/// Whether a stored line is a membership line, the kind `/pref joins` shapes.
pub fn is_membership(message: &MockMessage) -> bool {
    matches!(message.kind.as_str(), "join" | "part" | "quit" | "nick")
}

/// The caption a membership line shows. A stored body is the caption; an empty
/// one is the author with the verb the lone-line formats add.
pub fn membership_text(message: &MockMessage) -> String {
    if !message.body.is_empty() {
        return message.body.clone();
    }
    match message.kind.as_str() {
        "part" => part_line(&message.author),
        "quit" => quit_line(&message.author),
        _ => join_line(&message.author, ""),
    }
}

/// The stored lines as the transcript shows them, in display order. `Folded`
/// merges each run of membership lines into one caption, `Every` keeps each
/// line, and `Hidden` drops them. Chat, day and whois lines pass through, and
/// they end a run.
pub fn membership_view(messages: &[MockMessage], noise: MembershipNoise) -> Vec<MockMessage> {
    let mut shown: Vec<MockMessage> = Vec::with_capacity(messages.len());
    for message in messages {
        if !is_membership(message) {
            shown.push(message.clone());
            continue;
        }
        match noise {
            MembershipNoise::Hidden => {}
            MembershipNoise::Every => shown.push(message.clone()),
            MembershipNoise::Folded => fold_into(&mut shown, message),
        }
    }
    shown
}

/// Adds one membership line in folded mode. It folds into the caption above when
/// that is a caption of the same run, else it starts one. A fold that cancels the
/// whole caption removes it.
fn fold_into(shown: &mut Vec<MockMessage>, message: &MockMessage) {
    let text = membership_text(message);
    let folded = shown
        .last()
        .filter(|last| is_membership(last))
        .map(|last| fold_membership(&last.body, &text, same_nick));
    match folded {
        Some(fold) if fold.drop_row => {
            shown.pop();
        }
        Some(fold) => {
            if let Some(last) = shown.last_mut() {
                last.body = fold.body;
            }
        }
        None => shown.push(MockMessage {
            kind: "join".to_string(),
            author: String::new(),
            time: String::new(),
            day: message.day.clone(),
            body: text,
        }),
    }
}

/// The transcript compares nicks ignoring ASCII case, as the seeded world does.
fn same_nick(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::transcript::rows_from_messages;

    fn line(kind: &str, author: &str, body: &str) -> MockMessage {
        MockMessage {
            kind: kind.to_string(),
            author: author.to_string(),
            time: String::new(),
            day: "2026-09-12".to_string(),
            body: body.to_string(),
        }
    }

    fn chat(author: &str, body: &str) -> MockMessage {
        MockMessage {
            kind: "message".to_string(),
            author: author.to_string(),
            time: "10:00".to_string(),
            day: "2026-09-12".to_string(),
            body: body.to_string(),
        }
    }

    /// The caption of each event row the transcript shows, in order.
    fn event_captions(messages: &[MockMessage], noise: MembershipNoise) -> Vec<String> {
        rows_from_messages(&membership_view(messages, noise))
            .iter()
            .filter_map(|row| row.event_text.clone())
            .collect()
    }

    #[test]
    fn membership_text_uses_the_verb_for_each_kind() {
        assert_eq!(membership_text(&line("join", "sol", "")), "sol joined");
        assert_eq!(membership_text(&line("part", "sol", "")), "sol left");
        assert_eq!(membership_text(&line("quit", "sol", "")), "sol quit");
        assert_eq!(
            membership_text(&line("nick", "sol", "sol is now sol_")),
            "sol is now sol_"
        );
        assert_eq!(
            membership_text(&line("join", "sol", "sol (acct) joined")),
            "sol (acct) joined"
        );
    }

    #[test]
    fn folded_run_collapses_into_one_caption() {
        let messages = [
            chat("anna", "hi"),
            line("join", "sol", ""),
            line("join", "nora", ""),
            line("join", "kai", ""),
            chat("anna", "welcome"),
        ];
        assert_eq!(
            event_captions(&messages, MembershipNoise::Folded),
            ["sol, nora, kai joined"]
        );
        let shown = membership_view(&messages, MembershipNoise::Folded);
        assert_eq!(shown.len(), 3, "chat, one caption, chat");
    }

    #[test]
    fn every_shows_each_line() {
        let messages = [line("join", "sol", ""), line("join", "nora", "")];
        assert_eq!(
            event_captions(&messages, MembershipNoise::Every),
            ["sol joined", "nora joined"]
        );
    }

    #[test]
    fn hidden_drops_membership_and_keeps_chat() {
        let messages = [
            chat("anna", "before"),
            line("join", "sol", ""),
            line("part", "sol", ""),
            chat("anna", "after"),
        ];
        let shown = membership_view(&messages, MembershipNoise::Hidden);
        let bodies: Vec<&str> = shown.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, ["before", "after"]);
    }

    #[test]
    fn chat_ends_a_run() {
        let messages = [
            line("join", "sol", ""),
            chat("anna", "hi"),
            line("join", "nora", ""),
        ];
        assert_eq!(
            event_captions(&messages, MembershipNoise::Folded),
            ["sol joined", "nora joined"]
        );
    }

    #[test]
    fn a_join_by_the_nick_that_left_cancels_both() {
        let messages = [
            chat("anna", "before"),
            line("part", "sol", ""),
            line("join", "sol", ""),
            chat("anna", "after"),
        ];
        let shown = membership_view(&messages, MembershipNoise::Folded);
        assert_eq!(shown.len(), 2, "the departure and its return vanish");
        assert!(shown.iter().all(|m| m.kind == "message"));
    }

    #[test]
    fn a_join_by_the_nick_that_quit_cancels_its_clause_when_that_is_the_last() {
        let messages = [
            line("join", "sol", ""),
            line("quit", "nora", ""),
            line("join", "nora", ""),
        ];
        assert_eq!(
            event_captions(&messages, MembershipNoise::Folded),
            ["sol joined"]
        );
    }

    #[test]
    fn a_join_only_cancels_against_the_last_clause_as_qt_does() {
        // Qt compares with the run's last clause only, so the earlier quit stays.
        let messages = [
            line("quit", "nora", ""),
            line("join", "sol", ""),
            line("join", "nora", ""),
        ];
        assert_eq!(
            event_captions(&messages, MembershipNoise::Folded),
            ["nora quit, sol, nora joined"]
        );
    }

    #[test]
    fn a_join_with_an_account_still_cancels_the_bare_part() {
        let messages = [
            line("part", "sol", ""),
            line("join", "sol", "sol (acct) joined"),
        ];
        assert!(membership_view(&messages, MembershipNoise::Folded).is_empty());
    }

    #[test]
    fn a_join_by_another_nick_keeps_the_part() {
        let messages = [line("part", "sol", ""), line("join", "nora", "")];
        assert_eq!(
            event_captions(&messages, MembershipNoise::Folded),
            ["sol left, nora joined"]
        );
    }

    #[test]
    fn nick_changes_chain_to_the_first_old_and_last_new() {
        let messages = [
            line("nick", "a", "a is now b"),
            line("nick", "b", "b is now c"),
        ];
        assert_eq!(
            event_captions(&messages, MembershipNoise::Folded),
            ["a is now c"]
        );
    }

    #[test]
    fn mixed_clauses_keep_their_order() {
        let messages = [
            line("join", "anna", ""),
            line("part", "sol", ""),
            line("join", "nora", ""),
        ];
        assert_eq!(
            event_captions(&messages, MembershipNoise::Folded),
            ["anna joined, sol left, nora joined"]
        );
    }

    #[test]
    fn folded_shows_the_seeded_joins_unchanged() {
        // Two joins with chat between them stay two captions, as the seed shows them.
        let messages = [
            chat("mira", "hi"),
            line("join", "sol", ""),
            chat("anna", "welcome"),
            line("join", "nora", ""),
        ];
        assert_eq!(
            event_captions(&messages, MembershipNoise::Folded),
            ["sol joined", "nora joined"]
        );
    }
}
