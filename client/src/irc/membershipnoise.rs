//! Port of `tui/internal/irc/membershipnoise.go` (Go), `src/irc/ircmembershipnoise.h`
//! (C++), and the caption fold of `foldMembership` and `collapseEventBody` in
//! `src/irc/irceventreducer.cpp` (C++).
//!
//! Join, part, quit and nick lines are one caption each. A run of them can fold
//! into one caption (`Folded`), show one by one (`Every`), or disappear
//! (`Hidden`). The setting is `/pref joins`. The fold works on caption text, so
//! it needs no transcript type: the model folds stored lines through
//! `fold_membership`, and the reducer folds arriving lines through the same call.
//!
//! Pure: no clock, no I/O. `same` compares two nicks under the network's case
//! mapping; the caller supplies it.

use serde::{Deserialize, Serialize};

/// How join, part, quit and nick lines show (`IrcMembershipNoise`). `Folded` is
/// the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MembershipNoise {
    /// A run of membership lines collapses into one caption, and a leave that is
    /// undone at once by a rejoin is dropped.
    #[default]
    Folded,
    /// Every line shows on its own.
    Every,
    /// The lines are omitted, including history playback.
    Hidden,
}

impl MembershipNoise {
    /// Catalog order, which the slash popup offers after `/pref joins `.
    pub const ALL: [MembershipNoise; 3] = [
        MembershipNoise::Folded,
        MembershipNoise::Every,
        MembershipNoise::Hidden,
    ];

    /// The stored and typed token (`MembershipNoiseToken`).
    pub fn token(self) -> &'static str {
        match self {
            MembershipNoise::Folded => "folded",
            MembershipNoise::Every => "every",
            MembershipNoise::Hidden => "hidden",
        }
    }

    /// The level for a stored or typed word, ignoring case and surrounding space
    /// (`ParseMembershipNoise`). `None` for anything else; the caller applies the
    /// ordinary default.
    pub fn from_token(token: &str) -> Option<Self> {
        let folded = token.trim().to_lowercase();
        Self::ALL.into_iter().find(|level| level.token() == folded)
    }
}

/// The parenthetical account, or empty when it adds nothing (`ShownAccount`).
/// An account is hidden when it is the nick's own, empty, or `*`.
pub fn shown_account(same_as_nick: bool, account: &str) -> &str {
    if account.is_empty() || account == "*" || same_as_nick {
        ""
    } else {
        account
    }
}

/// One join line (`ircJoinLine`): `nick joined`, or `nick (account) joined`.
pub fn join_line(nick: &str, shown_account: &str) -> String {
    if shown_account.is_empty() {
        format!("{nick} joined")
    } else {
        format!("{nick} ({shown_account}) joined")
    }
}

/// One part line (`ircPartLine`).
pub fn part_line(nick: &str) -> String {
    format!("{nick} left")
}

/// One quit line (`ircQuitLine`).
pub fn quit_line(nick: &str) -> String {
    format!("{nick} quit")
}

/// One nick-change line (`ircNickLine`).
pub fn nick_line(old_nick: &str, new_nick: &str) -> String {
    format!("{old_nick} is now {new_nick}")
}

/// How one incoming membership line meets the caption before it (`FoldKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldKind {
    /// Appends with the ordinary same-suffix collapse.
    Merge,
    /// Drops a leave that the incoming join undoes.
    Cancel,
    /// Keeps the first old nick and the latest new nick.
    Chain,
}

/// The result of folding one incoming line onto an existing caption
/// (`MembershipFold`). `drop_row` means the whole caption cancelled out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MembershipFold {
    pub kind: FoldKind,
    pub drop_row: bool,
    pub body: String,
}

/// Folds `incoming` onto the caption `existing` (`FoldMembership`). A join by the
/// nick that just left or quit cancels that departure. A nick change chains onto
/// the one before it. Anything else, including a caption that is not one Omairc
/// writes, merges as plain text. `same` compares two nicks.
pub fn fold_membership(
    existing: &str,
    incoming: &str,
    same: impl Fn(&str, &str) -> bool,
) -> MembershipFold {
    let (Some(mut clauses), Some(next)) = (parse_clauses(existing), parse_clauses(incoming)) else {
        return merge(existing, incoming);
    };
    if next.len() != 1 {
        return merge(existing, incoming);
    }
    let incoming_clause = &next[0];
    let decision = match (clauses.last(), incoming_clause) {
        (Some(Clause::Left(tokens) | Clause::Quit(tokens)), Clause::Joined(joined))
            if joined.len() == 1
                && tokens.last().is_some_and(|nick| {
                    same(membership_nick(nick), membership_nick(&joined[0]))
                }) =>
        {
            Decision::Cancel
        }
        (Some(Clause::Nick { new, .. }), Clause::Nick { old, .. }) if same(new, old) => {
            Decision::Chain
        }
        _ => Decision::Merge,
    };
    match decision {
        Decision::Cancel => {
            if let Some(Clause::Left(tokens) | Clause::Quit(tokens)) = clauses.last_mut() {
                tokens.pop();
                if tokens.is_empty() {
                    clauses.pop();
                }
            }
            if clauses.is_empty() {
                return MembershipFold {
                    kind: FoldKind::Cancel,
                    drop_row: true,
                    body: String::new(),
                };
            }
            MembershipFold {
                kind: FoldKind::Cancel,
                drop_row: false,
                body: render_clauses(&clauses),
            }
        }
        Decision::Chain => {
            if let (Some(Clause::Nick { new, .. }), Clause::Nick { new: next_new, .. }) =
                (clauses.last_mut(), incoming_clause)
            {
                next_new.clone_into(new);
            }
            MembershipFold {
                kind: FoldKind::Chain,
                drop_row: false,
                body: render_clauses(&clauses),
            }
        }
        Decision::Merge => merge(existing, incoming),
    }
}

/// Two captions of the same verb merge their names: `a joined` and `b joined`
/// become `a, b joined` (`collapseEventBody`). Otherwise they join with a comma.
pub fn collapse_event_body(existing: &str, incoming: &str) -> String {
    for suffix in [" joined", " left", " quit"] {
        if existing.ends_with(suffix) && incoming.ends_with(suffix) {
            let head = &existing[..existing.len() - suffix.len()];
            let tail = &incoming[..incoming.len() - suffix.len()];
            return format!("{head}, {tail}{suffix}");
        }
    }
    format!("{existing}, {incoming}")
}

fn merge(existing: &str, incoming: &str) -> MembershipFold {
    MembershipFold {
        kind: FoldKind::Merge,
        drop_row: false,
        body: collapse_event_body(existing, incoming),
    }
}

enum Decision {
    Cancel,
    Chain,
    Merge,
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
        Clause::Nick { old, new } => nick_line(old, new),
    }
}

fn render_clauses(clauses: &[Clause]) -> String {
    clauses
        .iter()
        .map(render_clause)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Splits a caption into clauses (`parseMembershipClauses`). `None` when the text
/// is not one Omairc writes, or when it does not re-render to the same text.
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
    if clauses.is_empty() || render_clauses(&clauses) != body {
        return None;
    }
    Some(clauses)
}

/// The comma-separated nicks of a clause, empty entries dropped (`splitCommaSpace`).
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

#[cfg(test)]
mod tests {
    use super::*;

    fn same(left: &str, right: &str) -> bool {
        left.eq_ignore_ascii_case(right)
    }

    /// Folds `incoming` onto `existing`, returning the caption or None when the
    /// fold cancels the whole run out.
    fn fold_text(existing: &str, incoming: &str) -> Option<String> {
        let fold = fold_membership(existing, incoming, same);
        (!fold.drop_row).then_some(fold.body)
    }

    #[test]
    fn noise_levels_parse_and_round_trip() {
        for level in MembershipNoise::ALL {
            assert_eq!(MembershipNoise::from_token(level.token()), Some(level));
        }
        assert_eq!(
            MembershipNoise::from_token("  EVERY "),
            Some(MembershipNoise::Every)
        );
        assert_eq!(MembershipNoise::from_token("loud"), None);
        assert_eq!(MembershipNoise::default(), MembershipNoise::Folded);
    }

    #[test]
    fn lone_lines_read_as_the_verb_sentence() {
        assert_eq!(join_line("sol", ""), "sol joined");
        assert_eq!(join_line("sol", "acct"), "sol (acct) joined");
        assert_eq!(part_line("sol"), "sol left");
        assert_eq!(quit_line("sol"), "sol quit");
        assert_eq!(nick_line("sol", "sol_"), "sol is now sol_");
    }

    #[test]
    fn the_account_shows_only_when_it_adds_something() {
        assert_eq!(shown_account(false, "acct"), "acct");
        assert_eq!(shown_account(true, "acct"), "");
        assert_eq!(shown_account(false, "*"), "");
        assert_eq!(shown_account(false, ""), "");
    }

    #[test]
    fn a_join_by_the_nick_that_left_cancels_the_departure() {
        let fold = fold_membership("sol left", "sol joined", same);
        assert_eq!(fold.kind, FoldKind::Cancel);
        assert!(fold.drop_row);
        assert_eq!(fold_text("sol left", "sol joined"), None);
    }

    #[test]
    fn a_join_cancels_only_the_last_clause() {
        // Only the run's last clause is compared, so the earlier quit stays.
        assert_eq!(
            fold_text("nora quit, sol joined", "nora joined").as_deref(),
            Some("nora quit, sol, nora joined"),
            "the last clause is a join, so the quit is not cancelled and the join merges"
        );
        assert_eq!(
            fold_text("sol joined, nora quit", "nora joined").as_deref(),
            Some("sol joined"),
            "the trailing quit by nora cancels, leaving sol"
        );
    }

    #[test]
    fn a_cancel_leaves_the_other_clauses_in_order() {
        let fold = fold_membership("anna joined, sol left, nora quit", "nora joined", same);
        assert_eq!(fold.kind, FoldKind::Cancel);
        assert!(!fold.drop_row);
        assert_eq!(fold.body, "anna joined, sol left");
    }

    #[test]
    fn a_join_with_an_account_still_cancels_the_bare_leave() {
        let fold = fold_membership("sol left", "sol (acct) joined", same);
        assert!(fold.drop_row);
    }

    #[test]
    fn a_join_by_another_nick_keeps_the_leave() {
        assert_eq!(
            fold_text("sol left", "nora joined").as_deref(),
            Some("sol left, nora joined")
        );
    }

    #[test]
    fn nick_changes_chain_to_the_first_old_and_the_last_new() {
        let fold = fold_membership("a is now b", "b is now c", same);
        assert_eq!(fold.kind, FoldKind::Chain);
        assert_eq!(fold.body, "a is now c");
    }

    #[test]
    fn same_verb_captions_merge_their_names() {
        assert_eq!(collapse_event_body("a joined", "b joined"), "a, b joined");
        assert_eq!(
            collapse_event_body("sol joined", "nora left"),
            "sol joined, nora left"
        );
    }

    #[test]
    fn a_caption_that_does_not_parse_merges_as_plain_text() {
        let fold = fold_membership("sol joined", "nora left", same);
        assert_eq!(fold.kind, FoldKind::Merge);
        assert_eq!(fold.body, "sol joined, nora left");
        assert!(parse_clauses("").is_none());
        assert!(parse_clauses("sol").is_none(), "no verb at all");
        assert!(
            parse_clauses("sol joined, ").is_none(),
            "a dangling separator"
        );
        assert!(
            parse_clauses(" joined").is_none(),
            "no nick before the verb"
        );
        assert!(
            parse_clauses("a is now b c").is_none(),
            "a nick with a space"
        );
    }

    #[test]
    fn a_parsed_caption_renders_back_to_itself() {
        let body = "anna joined, sol left, nora quit, a is now b";
        let clauses = parse_clauses(body).expect("parses");
        assert_eq!(render_clauses(&clauses), body);
    }

    #[test]
    fn a_nick_with_an_account_folds_by_its_nick() {
        assert_eq!(membership_nick("sol (acct)"), "sol");
        assert_eq!(membership_nick("sol"), "sol");
        assert_eq!(
            membership_nick(" (acct)"),
            " (acct)",
            "nothing before the account"
        );
    }

    #[test]
    fn the_fold_uses_the_caller_supplied_nick_comparison() {
        // A case-sensitive comparison refuses the cancel that ASCII folding allows.
        let strict = |left: &str, right: &str| left == right;
        let fold = fold_membership("Sol left", "sol joined", strict);
        assert_eq!(fold.kind, FoldKind::Merge);
        assert_eq!(fold.body, "Sol left, sol joined");
    }
}
