//! What the slash-complete popup shows for the composer text.
//! Composer text becomes a probe. A bare `/word` gives the verb rows ranked against the
//! catalog visible on the surface. `/pref` gives its names, then its values. Ranking is
//! Qt's, not the fuzzy matcher the sheets use. A row must start with the typed letter
//! then scores exact (100), prefix (80), substring (50), or an ordered subsequence (20
//! plus a closeness term). A typed alias scores exact too.
//! Pure. The popup's highlight, its Escape memory and the Tab and Enter insertions live
//! in `model/slash_complete.rs`.

use super::command::{lookup, visible_on, ComposerSurface, VerbSpec};
use super::membershipnoise::MembershipNoise;
use super::pref::{pref_catalog, pref_find, PrefSpec, PrefValueKind};

/// Most verb rows the popup shows (Go `slashRankLimit`, Qt `rank` keeps six).
/// `/pref` value rows are not capped.
pub const RANK_LIMIT: usize = 6;

/// One popup row. `label` is what Tab and Enter insert, such as `/version` or
/// `/pref joins every`. `usage` is the text beside it (Go `SlashHit`, C++ `IrcSlashHit`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlashHit {
    pub label: String,
    pub usage: String,
}

/// The rows for what is typed, and the typed needle they match (Go `SlashProbe`, C++
/// `IrcSlashProbe`). It only exists when it has rows, so an open probe always has one.
/// Callers hold it as `Option<SlashProbe>`, where `None` is the closed popup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlashProbe {
    needle: String,
    hits: Vec<SlashHit>,
}

impl SlashProbe {
    /// The typed text after the slash, lowercased. For `/pref joins `, it is `pref joins `.
    pub fn needle(&self) -> &str {
        &self.needle
    }

    /// The rows, best first.
    pub fn hits(&self) -> &[SlashHit] {
        &self.hits
    }

 /// Whether a row has exactly this label (Go `SlashProbe.HasLabel`, C++
    /// `IrcSlashProbe::containsLabel`).
    #[cfg(test)]
    pub fn has_label(&self, label: &str) -> bool {
        self.hits.iter().any(|hit| hit.label == label)
    }

    /// Whether the typed text already names row `index`, by its name or by one of its
    /// aliases. Enter then sends rather than inserts (Go `slashHitIsExact`, Qt
    /// `tokenPassesSelected`).
    pub fn names_exactly(&self, index: usize) -> bool {
        let Some(hit) = self.hits.get(index) else {
            return false;
        };
        let Some(name) = hit.label.strip_prefix('/') else {
            return false;
        };
        if name == self.needle {
            return true;
        }
        lookup(name).is_some_and(|spec| spec.aliases.contains(&self.needle.as_str()))
    }
}

/// Projects composer text onto the popup rows for `surface`. `None` means the popup
/// stays closed (Go `ProjectSlash`, C++ `IrcSlashComplete::project`).
pub fn project_slash(composer_text: &str, surface: ComposerSurface) -> Option<SlashProbe> {
    match openable_needle(composer_text) {
        Some(needle) => {
            let hits = rank_slash(&needle, surface);
            open_probe(needle, hits)
        }
        None => project_pref(composer_text),
    }
}

/// Builds an open probe, or `None` when there is no needle or no row.
fn open_probe(needle: String, hits: Vec<SlashHit>) -> Option<SlashProbe> {
    (!needle.is_empty() && !hits.is_empty()).then_some(SlashProbe { needle, hits })
}

/// The text after a leading `/` when the popup may open: one non-space character
/// after the slash, and no space anywhere after it. `//` and `/ x` stay closed
/// (Go `openableNeedle`, C++ `IrcSlashComplete::openableNeedle`).
fn openable_needle(composer_text: &str) -> Option<String> {
    let body = composer_text.trim_start().strip_prefix('/')?;
    if body.is_empty() || body.starts_with('/') || body.contains(char::is_whitespace) {
        return None;
    }
    Some(body.to_lowercase())
}

/// Scores one verb row against the needle: the better of its name and an exact alias
/// (Go `scoreSpec`, C++ `IrcSlashComplete::scoreSpec`).
fn score_spec(needle: &str, spec: &VerbSpec) -> i32 {
    let mut best = score_token(needle, spec.name);
    if best < 100 && spec.aliases.contains(&needle) {
        best = 100;
    }
    best
}

/// How well `token` matches the typed `needle` (Go `scoreToken`, C++
/// `IrcSlashComplete::scoreToken`). Both are lowercase. `-1` means no match.
pub fn score_token(needle: &str, token: &str) -> i32 {
    let (Some(needle_first), Some(token_first)) = (needle.chars().next(), token.chars().next())
    else {
        return -1;
    };
    if needle_first != token_first {
        return -1;
    }
    if needle == token {
        return 100;
    }
    if token.starts_with(needle) {
        return 80;
    }
    if token.contains(needle) {
        return 50;
    }
    // An ordered subsequence. `end` is one past the last matched character, so a
    // tight match scores higher than a spread-out one.
    let token_chars: Vec<char> = token.chars().collect();
    let mut end = 0usize;
    for ch in needle.chars() {
        match token_chars[end..]
            .iter()
            .position(|&candidate| candidate == ch)
        {
            Some(offset) => end += offset + 1,
            None => return -1,
        }
    }
    20 + (needle.chars().count() as i32 * 10) / end as i32
}

/// Sorts scored rows by score, best first, then by their catalog index.
fn sorted_hits(mut scored: Vec<(usize, i32, SlashHit)>) -> Vec<SlashHit> {
    scored.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    scored.into_iter().map(|(_, _, hit)| hit).collect()
}

/// The verb rows that match `needle` on `surface`, best first, at most `RANK_LIMIT`.
/// Ties keep catalog order (Go `rankSlash`, C++ `IrcSlashComplete::rank`).
fn rank_slash(needle: &str, surface: ComposerSurface) -> Vec<SlashHit> {
    let scored: Vec<(usize, i32, SlashHit)> = visible_on(surface)
        .into_iter()
        .enumerate()
        .filter_map(|(index, spec)| {
            let score = score_spec(needle, spec);
            (score >= 0).then(|| {
                let hit = SlashHit {
                    label: format!("/{}", spec.name),
                    usage: spec.usage.to_string(),
                };
                (index, score, hit)
            })
        })
        .collect();
    sorted_hits(scored).into_iter().take(RANK_LIMIT).collect()
}

/// The `/pref` rows: names after `/pref `, then `on`/`off` or the joins levels after
/// `/pref <name> `. Go `projectPref`, C++ `IrcSlashComplete::projectPref`.
fn project_pref(composer_text: &str) -> Option<SlashProbe> {
    let body = composer_text.trim_start().strip_prefix('/')?;
    let verb_space = body.find(' ')?;
    if body[..verb_space].to_lowercase() != "pref" {
        return None;
    }
    let rest = &body[verb_space + 1..];
    if rest.starts_with(' ') {
        return None;
    }

    let needle = body.to_lowercase();
    match rest.find(' ') {
        None => pref_name_rows(needle, &rest.to_lowercase()),
        Some(name_space) => pref_value_rows(needle, &rest[..name_space], &rest[name_space + 1..]),
    }
}

/// The names that match `token`. An empty token lists all of them, in catalog order.
fn pref_name_rows(needle: String, token: &str) -> Option<SlashProbe> {
    let hits: Vec<SlashHit> = if token.is_empty() {
        pref_catalog().iter().map(pref_name_hit).collect()
    } else {
        let scored: Vec<(usize, i32, SlashHit)> = pref_catalog()
            .iter()
            .enumerate()
            .filter_map(|(index, spec)| {
                let score = score_token(token, spec.token);
                (score >= 0).then(|| (index, score, pref_name_hit(spec)))
            })
            .collect();
        sorted_hits(scored)
    };
    open_probe(needle, hits)
}

/// The row for one `/pref` name: its label, and its description as the usage.
fn pref_name_hit(spec: &PrefSpec) -> SlashHit {
    SlashHit {
        label: format!("/pref {}", spec.token),
        usage: spec.label.to_string(),
    }
}

/// The values that match `value` for `name_word`. An empty value lists all of them.
/// A value with a space in it, or an unknown name, has no rows.
fn pref_value_rows(needle: String, name_word: &str, value: &str) -> Option<SlashProbe> {
    if value.contains(' ') {
        return None;
    }
    let spec = pref_find(name_word)?;
    let words: Vec<&str> = match spec.value_kind {
        PrefValueKind::Noise => MembershipNoise::ALL
            .iter()
            .map(|level| level.token())
            .collect(),
        PrefValueKind::Toggle => vec!["on", "off"],
    };
    let prefix = format!("/pref {} ", spec.token);
    let folded = value.to_lowercase();

    let hits: Vec<SlashHit> = if folded.is_empty() {
        words
            .iter()
            .map(|word| {
                let label = format!("{prefix}{word}");
                SlashHit {
                    label: label.clone(),
                    usage: label,
                }
            })
            .collect()
    } else {
        let scored: Vec<(usize, i32, SlashHit)> = words
            .iter()
            .enumerate()
            .filter_map(|(index, word)| {
                let score = score_token(&folded, word);
                (score >= 0).then(|| {
                    let label = format!("{prefix}{word}");
                    let hit = SlashHit {
                        label: label.clone(),
                        usage: label,
                    };
                    (index, score, hit)
                })
            })
            .collect();
        sorted_hits(scored)
    };
    open_probe(needle, hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::command::ComposerSurface::{Conversation, Status};

    fn labels(probe: &Option<SlashProbe>) -> Vec<String> {
        probe
            .as_ref()
            .map(|probe| probe.hits().iter().map(|hit| hit.label.clone()).collect())
            .unwrap_or_default()
    }

    fn hit_labels(hits: &[SlashHit]) -> Vec<&str> {
        hits.iter().map(|hit| hit.label.as_str()).collect()
    }

    // tui/internal/irc/slashcomplete_test.go.

    #[test]
    fn openable_needle_takes_one_unspaced_word_after_a_single_slash() {
        let cases = [
            ("/j", Some("j")),
            ("/JOIN", Some("join")),
            ("/", None),
            ("//", None),
            ("///", None),
            ("/join #x", None),
            ("/a b", None),
            ("  /j", Some("j")),
            ("", None),
            ("hello", None),
            (" /", None),
        ];
        for (text, want) in cases {
            assert_eq!(
                openable_needle(text).as_deref(),
                want,
                "openable_needle({text:?})"
            );
        }
    }

    #[test]
    fn rank_slash_resolves_an_alias_and_orders_by_score() {
        let join = rank_slash("j", Conversation);
        assert_eq!(hit_labels(&join), ["/join"]);

        let topic = rank_slash("t", Conversation);
        assert!(topic.len() >= 2);
        assert_eq!(topic[0].label, "/topic");

        let time = rank_slash("t", Status);
        assert_eq!(time[0].label, "/time");
    }

    #[test]
    fn rank_slash_caps_the_rows_at_six() {
        let hits = rank_slash("m", Conversation);
        assert_eq!(
            hit_labels(&hits),
            ["/me", "/msg", "/mode", "/monitor", "/monitored", "/mute"]
        );
        assert!(
            !hit_labels(&hits).contains(&"/muted"),
            "/muted falls past the cap"
        );
        assert_eq!(RANK_LIMIT, 6);
    }

    #[test]
    fn a_bare_verb_prefix_projects_the_matching_rows() {
        let probe = project_slash("/jo", Conversation).expect("/jo opens");
        assert_eq!(probe.needle(), "jo");
        assert!(probe.has_label("/join"));
    }

    #[test]
    fn status_drops_me_but_a_conversation_ranks_it_first() {
        let status = project_slash("/me", Status).expect("status still opens for /me");
        assert!(!status.has_label("/me"), "Status completion excludes /me");
        assert_eq!(status.hits()[0].label, "/mode");

        let conversation = project_slash("/me", Conversation).expect("opens");
        assert!(conversation.has_label("/me"));
        assert_eq!(conversation.hits()[0].label, "/me");
    }

    #[test]
    fn the_closed_shapes_keep_the_popup_shut() {
        for text in [
            "/", "//", "/join #x", "hello", "", "//join", "//hi", "///x", "/ ",
        ] {
            assert!(
                project_slash(text, Conversation).is_none(),
                "{text:?} must stay closed"
            );
        }
    }

    #[test]
    fn a_bare_pref_word_completes_the_verb_row() {
        let bare = project_slash("/pref", Conversation).expect("opens");
        assert_eq!(hit_labels(bare.hits()), ["/pref"]);
    }

    #[test]
    fn a_pref_space_lists_the_names_with_their_labels() {
        let probe = project_slash("/pref ", Conversation).expect("lists toggles");
        assert_eq!(
            hit_labels(probe.hits()),
            [
                "/pref directs",
                "/pref avatars",
                "/pref unread",
                "/pref joins"
            ]
        );
        assert_eq!(probe.hits()[0].usage, "Reopen direct messages on startup");
    }

    #[test]
    fn a_pref_name_prefix_and_its_case_pick_one_name() {
        let one = project_slash("/pref a", Conversation).expect("opens");
        assert!(one.hits().len() == 1 && one.hits()[0].label == "/pref avatars");
        let upper = project_slash("/pref DIRECTS", Conversation).expect("opens");
        assert_eq!(hit_labels(upper.hits()), ["/pref directs"]);
    }

    #[test]
    fn a_toggle_offers_on_and_off() {
        let probe = project_slash("/pref avatars ", Conversation).expect("lists on/off");
        assert_eq!(
            hit_labels(probe.hits()),
            ["/pref avatars on", "/pref avatars off"]
        );
        assert_eq!(probe.hits()[0].usage, "/pref avatars on");

        let partial = project_slash("/pref avatars o", Conversation).expect("opens");
        assert_eq!(partial.hits().len(), 2);
        assert_eq!(partial.hits()[0].label, "/pref avatars on");
    }

    #[test]
    fn the_joins_level_offers_folded_every_and_hidden() {
        assert_eq!(
            labels(&project_slash("/pref joins ", Conversation)),
            [
                "/pref joins folded",
                "/pref joins every",
                "/pref joins hidden"
            ]
        );
        assert_eq!(
            labels(&project_slash("/pref joins f", Conversation)),
            ["/pref joins folded"]
        );
    }

    #[test]
    fn the_pref_closed_shapes_keep_the_popup_shut() {
        for text in [
            "/pref avatars x",
            "/pref bogus ",
            "/pref  directs",
            "/pref avatars on x",
        ] {
            assert!(
                project_slash(text, Conversation).is_none(),
                "{text:?} must stay closed"
            );
        }
    }

    #[test]
    fn a_probe_has_label_and_a_closed_probe_has_none() {
        let probe = SlashProbe {
            needle: "j".into(),
            hits: vec![SlashHit {
                label: "/join".into(),
                usage: "/join [channel] [key][, ...]".into(),
            }],
        };
        assert!(probe.has_label("/join"));
        assert!(!probe.has_label("/j"));
        assert!(!None::<SlashProbe>.is_some_and(|p| p.has_label("/join")));
    }

    #[test]
    fn an_open_probe_needs_a_needle_and_a_row() {
        assert!(open_probe(
            String::new(),
            vec![SlashHit {
                label: "/join".into(),
                usage: String::new()
            }]
        )
        .is_none());
        assert!(open_probe("j".into(), Vec::new()).is_none());
    }

    // (`slashProjectClosed`, `slashProjectOpen`).

    #[test]
    fn slash_project_closed_matches_the_core_inputs() {
        for text in [
            "",
            "/",
            "/ ",
            "//hi",
            "///x",
            "/join #omarchy",
            "hello",
            "//join",
        ] {
            assert!(
                project_slash(text, Conversation).is_none(),
                "{text:?} must stay closed"
            );
        }
    }

    #[test]
    fn slash_project_open_ranks_the_core_rows_first() {
        let join = project_slash("/jo", Conversation).expect("opens");
        assert_eq!(join.hits()[0].label, "/join");
        assert_eq!(join.hits()[0].usage, "/join [channel] [key][, ...]");

        let alias = project_slash("/j", Conversation).expect("opens");
        assert_eq!(alias.hits()[0].label, "/join");

        let leave = project_slash("/LEAVE", Conversation).expect("opens");
        assert_eq!(
            leave.hits()[0].label,
            "/part",
            "a typed alias ranks its verb first"
        );

        let msg = project_slash("/msg", Conversation).expect("opens");
        assert_eq!(msg.hits()[0].label, "/msg");
        assert_eq!(msg.hits()[0].usage, "/msg <nick> <text>");

        let ping = project_slash("/pi", Conversation).expect("opens");
        assert_eq!(ping.hits()[0].label, "/ping");
        assert_eq!(ping.hits()[0].usage, "/ping [nick]");

        let list = project_slash("/li", Conversation).expect("opens");
        assert!(list.has_label("/list"));

        let invite = project_slash("/inv", Conversation).expect("opens");
        assert_eq!(invite.hits()[0].label, "/invite");
    }

    #[test]
    fn slash_project_open_status_scopes_the_rows() {
        let me_status = project_slash("/me", Status).expect("opens");
        assert_eq!(me_status.hits()[0].label, "/mode");
        assert!(!me_status.has_label("/me"));

        let me_conversation = project_slash("/me", Conversation).expect("opens");
        assert_eq!(me_conversation.hits()[0].label, "/me");

        let topic_status = project_slash("/t", Status).expect("opens");
        assert!(!topic_status.has_label("/topic"));
        assert_eq!(topic_status.hits()[0].label, "/time");

        let topic_conversation = project_slash("/t", Conversation).expect("opens");
        assert_eq!(topic_conversation.hits()[0].label, "/topic");
        assert!(topic_conversation.has_label("/time"));
        assert!(!topic_conversation.has_label("/notice"));

        let op_conversation = project_slash("/o", Conversation).expect("opens");
        assert_eq!(op_conversation.hits()[0].label, "/op");
        assert!(
            project_slash("/o", Status).is_none(),
            "op is conversation-only"
        );
    }

    #[test]
    fn slash_project_open_status_pairs_and_families() {
        let ignore = project_slash("/ig", Status).expect("opens");
        assert_eq!(ignore.hits()[0].label, "/ignore");
        assert!(ignore.has_label("/ignored"));

        let monitor = project_slash("/mon", Status).expect("opens");
        assert_eq!(monitor.hits()[0].label, "/monitor");
        assert!(monitor.has_label("/monitored"));

        let mute = project_slash("/mu", Status).expect("opens");
        assert_eq!(mute.hits()[0].label, "/mute");
        assert!(mute.has_label("/muted"));

        let highlight = project_slash("/high", Status).expect("opens");
        assert_eq!(highlight.hits()[0].label, "/highlight");
        assert!(highlight.has_label("/highlights"));

        assert!(project_slash("/auto", Status)
            .expect("opens")
            .has_label("/autoaway"));
    }

    #[test]
    fn q_completes_to_query_and_quit_completes_to_disconnect() {
        let query = project_slash("/q", Conversation).expect("opens");
        assert_eq!(query.hits()[0].label, "/query");
        assert!(!query.has_label("/disconnect"));

        let quit = project_slash("/quit", Conversation).expect("opens");
        assert_eq!(quit.hits()[0].label, "/disconnect");
        assert_eq!(quit.hits()[0].usage, "/disconnect [reason]");
        assert!(
            quit.names_exactly(0),
            "the quit alias names the row in full"
        );
    }

    // The scoring tiers and the names-exactly rule.

    #[test]
    fn score_tiers_follow_qt() {
        assert_eq!(score_token("version", "version"), 100);
        assert_eq!(score_token("ver", "version"), 80);
        assert_eq!(score_token("ion", "idion"), 50);
        assert_eq!(score_token("ls", "list"), 26);
        assert_eq!(score_token("lt", "list"), 25);
        assert_eq!(score_token("on", "version"), -1);
        assert_eq!(score_token("ion", "version"), -1);
        assert_eq!(score_token("", "version"), -1);
        assert_eq!(score_token("v", ""), -1);
    }

    #[test]
    fn the_case_of_the_catalog_and_the_needle_does_not_change_the_match() {
        assert_eq!(labels(&project_slash("/LIST", Conversation)), ["/list"]);
        assert_eq!(
            labels(&project_slash("/Pref Av", Conversation)),
            ["/pref avatars"]
        );
    }

    #[test]
    fn an_alias_names_its_row_and_a_prefix_does_not() {
        let alias = project_slash("/j", Conversation).expect("opens");
        assert!(alias.names_exactly(0), "/j names /join by alias");
        let partial = project_slash("/jo", Conversation).expect("opens");
        assert!(!partial.names_exactly(0), "/jo is a partial word");
        let pref = project_slash("/pref avatars", Conversation).expect("opens");
        assert!(pref.names_exactly(0), "a full /pref name names its row");
    }

    #[test]
    fn a_hit_past_the_end_or_a_non_label_never_names_itself() {
        let probe = SlashProbe {
            needle: "j".into(),
            hits: vec![SlashHit {
                label: "join".into(),
                usage: String::new(),
            }],
        };
        assert!(
            !probe.names_exactly(0),
            "a label without its slash names nothing"
        );
        assert!(!probe.names_exactly(9));
    }
}
