//! Port of `tui/internal/storage/opendirect.go` (Go) and `src/irc/ircopendirect.cpp`
//! (C++): the direct messages to reopen at startup, and the ones the user closed.
//!
//! Two per-network lists, both case-mapped: `openDirects/<network>/targets` holds the
//! queries to restore, and `dismissedDirects/<network>/targets` holds the ones the user
//! closed, so a message from them does not bring them back. Only the user's own actions
//! write them: a query the user opened or replied in is open, a close dismisses it.
//! The lists are the shared `CaseListStore`, so they keep the same rules as the
//! ignore and mute lists.

use crate::irc::casemapping::IrcCaseMapping;

use super::lists::{CaseListStore, IniListBacking};
use super::Backing;

/// The Qt group that holds the open queries.
pub const OPEN_DIRECTS_GROUP: &str = "openDirects";
/// The Qt group that holds the queries the user closed.
pub const DISMISSED_DIRECTS_GROUP: &str = "dismissedDirects";
/// The key under each network that holds its target list.
pub const TARGETS_KEY: &str = "targets";

/// The open and the dismissed direct messages of every network.
pub struct OpenDirectStore {
    open: CaseListStore,
    dismissed: CaseListStore,
}

impl OpenDirectStore {
    /// The store over `backing`, holding whatever an earlier run saved.
    pub fn new(backing: &Backing) -> Self {
        Self {
            open: CaseListStore::new(Box::new(IniListBacking::new(
                backing.clone(),
                OPEN_DIRECTS_GROUP,
                TARGETS_KEY,
            ))),
            dismissed: CaseListStore::new(Box::new(IniListBacking::new(
                backing.clone(),
                DISMISSED_DIRECTS_GROUP,
                TARGETS_KEY,
            ))),
        }
    }

    /// The open queries of `network_id`, in the order they were added (`Targets`).
    #[cfg(test)]
    pub fn targets(&self, network_id: &str) -> Vec<String> {
        self.open.entries(network_id)
    }

    /// The open queries with case-variant duplicates removed (`Listed`).
    pub fn listed(&self, network_id: &str, mapping: IrcCaseMapping) -> Vec<String> {
        self.open.listed(network_id, mapping)
    }

    /// Records a query as open (`Add`). False when it already is.
    pub fn add(&mut self, network_id: &str, target: &str, mapping: IrcCaseMapping) -> bool {
        self.open.add(network_id, target, mapping)
    }

    /// Forgets an open query (`Remove`). False when it was not open.
    pub fn remove(&mut self, network_id: &str, target: &str, mapping: IrcCaseMapping) -> bool {
        self.open.remove(network_id, target, mapping)
    }

    /// Follows a NICK in the open queries (`Rekey`).
    pub fn rekey(&mut self, network_id: &str, old: &str, new: &str, mapping: IrcCaseMapping) -> bool {
        self.open.rekey(network_id, old, new, mapping)
    }

    /// Whether the user closed this query (`IsDismissed`).
    #[cfg(test)]
    pub fn is_dismissed(&self, network_id: &str, target: &str, mapping: IrcCaseMapping) -> bool {
        self.dismissed.contains(network_id, target, mapping)
    }

    /// Marks a query closed by the user (`Dismiss`).
    pub fn dismiss(&mut self, network_id: &str, target: &str, mapping: IrcCaseMapping) -> bool {
        self.dismissed.add(network_id, target, mapping)
    }

    /// Lets a closed query come back (`Undismiss`).
    pub fn undismiss(&mut self, network_id: &str, target: &str, mapping: IrcCaseMapping) -> bool {
        self.dismissed.remove(network_id, target, mapping)
    }

    /// Follows a NICK in the closed queries (`RekeyDismissed`).
    pub fn rekey_dismissed(
        &mut self,
        network_id: &str,
        old: &str,
        new: &str,
        mapping: IrcCaseMapping,
    ) -> bool {
        self.dismissed.rekey(network_id, old, new, mapping)
    }

    /// Drops both lists of a network the user removed (`Forget`).
    pub fn forget(&mut self, network_id: &str) {
        self.open.forget(network_id);
        self.dismissed.forget(network_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::casemapping::IrcCaseMappingKind;
    use crate::util::testdir::TempDir;

    fn rfc1459() -> IrcCaseMapping {
        IrcCaseMapping::new(IrcCaseMappingKind::Rfc1459)
    }

    #[test]
    fn open_and_dismissed_are_separate_lists() {
        let mut store = OpenDirectStore::new(&Backing::Ephemeral);
        assert!(store.add("net", "Anna", rfc1459()));
        assert!(!store.add("net", "anna", rfc1459()), "a case variant is the same query");
        assert_eq!(store.targets("net"), ["Anna"], "the first spelling is kept");
        assert!(store.dismiss("net", "anna", rfc1459()));
        assert!(store.is_dismissed("net", "ANNA", rfc1459()));
        assert_eq!(store.targets("net"), ["Anna"], "a dismiss does not close the list");
        assert!(store.undismiss("net", "Anna", rfc1459()));
        assert!(!store.is_dismissed("net", "anna", rfc1459()));
    }

    #[test]
    fn rekey_follows_a_nick_change_in_both_lists() {
        let mut store = OpenDirectStore::new(&Backing::Ephemeral);
        store.add("net", "anna", rfc1459());
        store.dismiss("net", "anna", rfc1459());
        assert!(store.rekey("net", "anna", "anna_", rfc1459()));
        assert!(store.rekey_dismissed("net", "anna", "anna_", rfc1459()));
        assert_eq!(store.targets("net"), ["anna_"]);
        assert!(store.is_dismissed("net", "anna_", rfc1459()));
    }

    #[test]
    fn forget_drops_both_lists_of_one_network_only() {
        let mut store = OpenDirectStore::new(&Backing::Ephemeral);
        store.add("a", "dax", rfc1459());
        store.dismiss("a", "dax", rfc1459());
        store.add("b", "dax", rfc1459());
        store.forget("a");
        assert!(store.targets("a").is_empty());
        assert!(!store.is_dismissed("a", "dax", rfc1459()));
        assert_eq!(store.targets("b"), ["dax"]);
    }

    #[test]
    fn the_lists_survive_a_restart_under_the_qt_groups() {
        let dir = TempDir::new("opendirect");
        let backing = Backing::Disk(dir.path().join("omairc.conf"));
        let mut first = OpenDirectStore::new(&backing);
        first.add("net", "anna", rfc1459());
        first.add("net", "dax", rfc1459());
        first.dismiss("net", "dax", rfc1459());

        let text = std::fs::read_to_string(dir.path().join("omairc.conf")).expect("written");
        // QSettings writes a nested group as its top section and `\` in the key names.
        assert!(text.contains("[openDirects]"), "{text}");
        assert!(text.contains("net\\targets="), "{text}");
        assert!(text.contains("[dismissedDirects]"), "{text}");

        let second = OpenDirectStore::new(&backing);
        assert_eq!(second.targets("net"), ["anna", "dax"]);
        assert!(second.is_dismissed("net", "dax", rfc1459()));
    }
}
