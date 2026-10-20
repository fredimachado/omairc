//! Join, part, quit and nick lines, and how a run of them shows (`/pref joins`).
//!
//! Qt folds these as they arrive (`IrcEventReducer::appendEvent` and
//! `foldMembership` in `src/irc/irceventreducer.cpp`); the line formats are in
//! `src/irc/ircmembershipnoise.h`. The PoC folds the stored lines each time a
//! conversation is shown, so a `/pref joins` change reaches the whole transcript
//! at once. Qt applies the setting only to lines that arrive after it.

use super::mock::MockMessage;
use super::prefs::MembershipNoise;

/// Whether a stored line is a membership line, the kind `/pref joins` shapes.
pub fn is_membership(message: &MockMessage) -> bool {
    matches!(message.kind.as_str(), "join" | "part" | "quit" | "nick")
}

/// The caption a membership line shows. A stored body is the caption; an empty
/// one is the author with the verb `ircJoinLine`, `ircPartLine` or `ircQuitLine`
/// adds.
pub fn membership_text(message: &MockMessage) -> String {
    if !message.body.is_empty() {
        return message.body.clone();
    }
    match message.kind.as_str() {
        "part" => format!("{} left", message.author),
        "quit" => format!("{} quit", message.author),
        _ => format!("{} joined", message.author),
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

/// Adds one membership line in folded mode. It joins the caption above when
/// that is a caption of the same run (`collapsible` in Qt), else it starts one.
fn fold_into(shown: &mut Vec<MockMessage>, message: &MockMessage) {
    let text = membership_text(message);
    match shown.last_mut().filter(|last| is_membership(last)) {
        Some(last) => match fold_text(&last.body, &text) {
            Some(body) => last.body = body,
            // The run cancelled out (a part undone by a join): the caption goes.
            None => {
                shown.pop();
            }
        },
        None => shown.push(MockMessage {
            kind: "join".to_string(),
            author: String::new(),
            time: String::new(),
            day: message.day.clone(),
            body: text,
        }),
    }
}

/// Folds `incoming` into the caption `existing` (Qt `foldMembership`). Returns the
/// caption's new text, or None when the run cancels out entirely.
fn fold_text(existing: &str, incoming: &str) -> Option<String> {
    let (Some(mut clauses), Some(next)) = (parse_clauses(existing), parse_clauses(incoming))
    else {
        return Some(collapse_body(existing, incoming));
    };
    if next.len() != 1 {
        return Some(collapse_body(existing, incoming));
    }
    let incoming_clause = &next[0];
    let Some(last) = clauses.last_mut() else {
        return Some(collapse_body(existing, incoming));
    };
    match (last, incoming_clause) {
        // A join by the nick that just left or quit cancels that departure.
        (Clause::Left(tokens) | Clause::Quit(tokens), Clause::Joined(joined))
            if joined.len() == 1
                && tokens.last().is_some_and(|nick| {
                    same_nick(membership_nick(nick), membership_nick(&joined[0]))
                }) =>
        {
            tokens.pop();
            if tokens.is_empty() {
                clauses.pop();
            }
            if clauses.is_empty() {
                None
            } else {
                Some(render_all(&clauses))
            }
        }
        // A nick change chains onto the one before it: a -> b, then b -> c is a -> c.
        (Clause::Nick { new, .. }, Clause::Nick { old, new: next_new })
            if same_nick(new.as_str(), old.as_str()) =>
        {
            *new = next_new.clone();
            Some(render_all(&clauses))
        }
        _ => Some(collapse_body(existing, incoming)),
    }
}

/// Two captions of the same verb merge their names: `a joined` and `b joined`
/// become `a, b joined` (Qt `collapseEventBody`).
fn collapse_body(existing: &str, incoming: &str) -> String {
    for suffix in [" joined", " left", " quit"] {
        if existing.ends_with(suffix) && incoming.ends_with(suffix) {
            let head = &existing[..existing.len() - suffix.len()];
            let tail = &incoming[..incoming.len() - suffix.len()];
            return format!("{head}, {tail}{suffix}");
        }
    }
    format!("{existing}, {incoming}")
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Clause {
    Joined(Vec<String>),
    Left(Vec<String>),
    Quit(Vec<String>),
    Nick { old: String, new: String },
}

#[derive(Clone, Copy)]
enum Verb {
    Joined,
    Left,
    Quit,
    Nick,
}

const VERBS: [(&str, Verb); 4] = [
    (" joined", Verb::Joined),
    (" left", Verb::Left),
    (" quit", Verb::Quit),
    (" is now ", Verb::Nick),
];

fn render_clause(clause: &Clause) -> String {
    match clause {
        Clause::Joined(tokens) => format!("{} joined", tokens.join(", ")),
        Clause::Left(tokens) => format!("{} left", tokens.join(", ")),
        Clause::Quit(tokens) => format!("{} quit", tokens.join(", ")),
        Clause::Nick { old, new } => format!("{old} is now {new}"),
    }
}

fn render_all(clauses: &[Clause]) -> String {
    clauses
        .iter()
        .map(render_clause)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Splits a caption into clauses. None when the text is not one Omairc writes,
/// so the caller merges it as plain text (Qt `parseMembershipClauses`).
fn parse_clauses(body: &str) -> Option<Vec<Clause>> {
    if body.is_empty() {
        return None;
    }
    let mut clauses = Vec::new();
    let mut index = 0usize;
    while index < body.len() {
        let rest = &body[index..];
        // The verb found earliest in what is left decides the clause.
        let (at, verb, len) = VERBS
            .iter()
            .filter_map(|(suffix, verb)| rest.find(suffix).map(|at| (at, *verb, suffix.len())))
            .min_by_key(|(at, _, _)| *at)?;
        if at == 0 {
            return None;
        }
        match verb {
            Verb::Nick => {
                let cursor = at + len;
                let end = rest[cursor..]
                    .find(',')
                    .map_or(rest.len(), |comma| cursor + comma);
                let new = &rest[cursor..end];
                if new.is_empty() || new.contains(' ') {
                    return None;
                }
                clauses.push(Clause::Nick {
                    old: rest[..at].to_string(),
                    new: new.to_string(),
                });
                index += end;
            }
            Verb::Joined | Verb::Left | Verb::Quit => {
                let tokens = split_tokens(&rest[..at]);
                if tokens.is_empty() {
                    return None;
                }
                clauses.push(match verb {
                    Verb::Joined => Clause::Joined(tokens),
                    Verb::Left => Clause::Left(tokens),
                    _ => Clause::Quit(tokens),
                });
                index += at + len;
            }
        }
        if index >= body.len() {
            break;
        }
        if !body[index..].starts_with(", ") {
            return None;
        }
        index += 2;
        if index >= body.len() {
            return None;
        }
    }
    if clauses.is_empty() || render_all(&clauses) != body {
        return None;
    }
    Some(clauses)
}

fn split_tokens(text: &str) -> Vec<String> {
    text.split(", ")
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// The nick in a joined token, which may carry an account: `sol (acct)` is `sol`.
fn membership_nick(token: &str) -> &str {
    token
        .strip_suffix(')')
        .and_then(|_| token.find(" ("))
        .filter(|at| *at > 0)
        .map_or(token, |at| &token[..at])
}

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
    fn a_caption_that_does_not_parse_merges_as_plain_text() {
        assert_eq!(
            collapse_body("sol joined", "nora left"),
            "sol joined, nora left"
        );
        assert_eq!(collapse_body("a joined", "b joined"), "a, b joined");
        assert!(parse_clauses("").is_none());
        assert!(parse_clauses("sol").is_none(), "no verb at all");
        assert!(parse_clauses("sol joined, ").is_none(), "a dangling separator");
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
