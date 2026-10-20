//! Nick sheet rows: the channel's members, fuzzy-matched on the nick. Ties
//! keep panel (PREFIX) order, which is the order `Conversation::members`
//! already holds. Port of `refreshNickMatches`, `canOpenDirectMessage`, and
//! `firstOpenableNickIndex` in OmaircWindow.qml.

use super::fuzzy::rank;
use super::mock::Member;

/// One row of the nick sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct NickTarget {
    pub member: Member,
    /// Byte offsets into the nick of the characters the query matched.
    pub indices: Vec<usize>,
}

/// Members whose nick fuzzy-matches the query, best match first, then panel
/// order. An empty query keeps everyone, including the local user (who is
/// listed but inert).
pub fn nick_targets(members: &[Member], query: &str) -> Vec<NickTarget> {
    rank(query, members.iter().cloned(), |member: &Member| {
        [(member.nick.as_str(), 1)]
    })
    .into_iter()
    .map(|ranked| {
        let [hit] = ranked.fields;
        NickTarget {
            member: ranked.item,
            indices: hit.map(|hit| hit.indices).unwrap_or_default(),
        }
    })
    .collect()
}

/// Enter on a nick opens or creates a DM unless it is empty or the local nick.
/// Qt compares nicks exactly.
pub fn can_open_direct(nick: &str, self_nick: &str) -> bool {
    !nick.is_empty() && nick != self_nick
}

/// Where the selection starts when the sheet opens: the first openable row,
/// or row 0 when none is openable.
pub fn first_selectable(targets: &[NickTarget], self_nick: &str) -> usize {
    targets
        .iter()
        .position(|target| can_open_direct(&target.member.nick, self_nick))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(nick: &str) -> Member {
        Member {
            nick: nick.to_string(),
            prefix: String::new(),
            realname: String::new(),
            presence: "online".to_string(),
            bot: false,
            status: None,
            avatar: None,
        }
    }

    fn roster() -> Vec<Member> {
        vec![
            member("fred"),
            member("anna"),
            member("Mira"),
            member("dax"),
        ]
    }

    fn nicks(targets: &[NickTarget]) -> Vec<&str> {
        targets.iter().map(|t| t.member.nick.as_str()).collect()
    }

    #[test]
    fn empty_query_keeps_panel_order_including_self() {
        let members = roster();
        let targets = nick_targets(&members, "");
        assert_eq!(nicks(&targets), ["fred", "anna", "Mira", "dax"]);
        assert!(targets.iter().all(|t| t.indices.is_empty()));
    }

    #[test]
    fn query_is_trimmed_and_case_folded() {
        let members = roster();
        let targets = nick_targets(&members, "  MI ");
        assert_eq!(nicks(&targets), ["Mira"]);
        assert_eq!(targets[0].indices, [0, 1]);
    }

    #[test]
    fn fuzzy_subsequence_matches_a_nick() {
        let members = roster();
        let targets = nick_targets(&members, "dx");
        assert_eq!(nicks(&targets), ["dax"]);
        assert_eq!(targets[0].indices, [0, 2]);
    }

    #[test]
    fn a_better_match_outranks_an_earlier_panel_row() {
        // "a-n" is scattered and sits first in the panel, "anna" is contiguous.
        let members = vec![member("a-n"), member("anna")];
        let targets = nick_targets(&members, "an");
        assert_eq!(nicks(&targets), ["anna", "a-n"]);
    }

    #[test]
    fn ties_keep_panel_order() {
        // Both nicks hit the same way, so the panel order is the tie-break.
        let members = vec![member("kai-a"), member("max-a")];
        let targets = nick_targets(&members, "a");
        assert_eq!(nicks(&targets), ["kai-a", "max-a"]);
    }

    #[test]
    fn no_match_is_an_empty_list() {
        let members = roster();
        assert!(nick_targets(&members, "zed").is_empty());
    }

    #[test]
    fn self_and_empty_nicks_are_not_openable() {
        assert!(can_open_direct("mira", "fred"));
        assert!(!can_open_direct("fred", "fred"));
        assert!(!can_open_direct("", "fred"));
        // Qt compares exactly, so a case variant of the local nick opens a DM.
        assert!(can_open_direct("Fred", "fred"));
    }

    #[test]
    fn selection_starts_on_first_non_self_nick() {
        let members = roster();
        let targets = nick_targets(&members, "");
        assert_eq!(first_selectable(&targets, "fred"), 1);
        assert_eq!(first_selectable(&targets, "anna"), 0);
    }

    #[test]
    fn selection_falls_back_to_zero_when_nothing_is_openable() {
        let only_self = vec![member("fred")];
        let targets = nick_targets(&only_self, "");
        assert_eq!(first_selectable(&targets, "fred"), 0);
        assert_eq!(first_selectable(&[], "fred"), 0);
    }
}
