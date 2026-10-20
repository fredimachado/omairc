//! Nick sheet rows: the channel's members, filtered, in panel order (the
//! order `Conversation::members` already holds). Port of `refreshNickMatches`,
//! `canOpenDirectMessage`, and `firstOpenableNickIndex` in OmaircWindow.qml.

use super::mock::Member;

/// Members whose nick contains the trimmed, case-folded query. An empty query
/// keeps everyone, including the local user (who is listed but inert).
pub fn nick_targets<'a>(members: &'a [Member], query: &str) -> Vec<&'a Member> {
    let needle = query.trim().to_lowercase();
    members
        .iter()
        .filter(|member| needle.is_empty() || member.nick.to_lowercase().contains(&needle))
        .collect()
}

/// Enter on a nick opens or creates a DM unless it is empty or the local nick.
/// Qt compares nicks exactly.
pub fn can_open_direct(nick: &str, self_nick: &str) -> bool {
    !nick.is_empty() && nick != self_nick
}

/// Where the selection starts when the sheet opens: the first openable row,
/// or row 0 when none is openable.
pub fn first_selectable(targets: &[&Member], self_nick: &str) -> usize {
    targets
        .iter()
        .position(|member| can_open_direct(&member.nick, self_nick))
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

    #[test]
    fn empty_query_keeps_panel_order_including_self() {
        let members = roster();
        let nicks: Vec<&str> = nick_targets(&members, "")
            .iter()
            .map(|m| m.nick.as_str())
            .collect();
        assert_eq!(nicks, ["fred", "anna", "Mira", "dax"]);
    }

    #[test]
    fn query_is_trimmed_and_case_folded() {
        let members = roster();
        let nicks: Vec<&str> = nick_targets(&members, "  MI ")
            .iter()
            .map(|m| m.nick.as_str())
            .collect();
        assert_eq!(nicks, ["Mira"]);
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
