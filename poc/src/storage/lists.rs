//! The per-network lists the controller keeps across runs: `/ignore`, `/mute`,
//! `/highlight` and the `/monitor` nicks. Port of the list stores in
//! `tui/internal/controller/stores.go` (Go) and `src/irc/ircignore.cpp`,
//! `src/irc/ircmute.cpp`, `src/irc/irchighlight.cpp` and the monitor list of
//! `src/irc/ircmonitor.cpp` (C++).
//!
//! The lists share one shape: a per-network, insertion-ordered list where membership
//! and removal compare through the network's case mapping, and adding a duplicate is
//! a no-op. `CaseListStore` is that shape once. A store loads its lists from a
//! `ListBacking` when it is built and saves them after each change. `IniListBacking`
//! writes `<group>/<network>/<key>` string lists, the layout Qt and the TUI use.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::irc::casemapping::IrcCaseMapping;
use crate::irc::reducer::ClosedStore;

use super::settings::Settings;
use super::Backing;

/// Where a store's lists are kept between runs. `load` returns what an earlier run
/// saved, keyed by network id; `save` replaces it with the current lists.
pub trait ListBacking {
    fn load(&self) -> BTreeMap<String, Vec<String>>;
    fn save(&self, lists: &BTreeMap<String, Vec<String>>);
}

/// A backing that keeps the lists in memory. Clones share the same lists, so a store
/// built over a clone sees what an earlier store saved, as a restart would.
#[cfg(test)]
#[derive(Clone, Default)]
pub struct MemoryBacking(Rc<RefCell<BTreeMap<String, Vec<String>>>>);

#[cfg(test)]
impl ListBacking for MemoryBacking {
    fn load(&self) -> BTreeMap<String, Vec<String>> {
        self.0.borrow().clone()
    }

    fn save(&self, lists: &BTreeMap<String, Vec<String>>) {
        *self.0.borrow_mut() = lists.clone();
    }
}

/// The shared case-mapped list (`caseListStore`).
pub struct CaseListStore {
    lists: BTreeMap<String, Vec<String>>,
    backing: Box<dyn ListBacking>,
}

/// The `/ignore` list (`IgnoreStore`, `IrcIgnoreStore`).
pub type IgnoreStore = CaseListStore;
/// The `/mute` list (`MuteStore`, `IrcMuteStore`).
pub type MuteStore = CaseListStore;
/// The `/monitor` nick list (`MonitorStore`, `IrcMonitorStore`).
pub type MonitorStore = CaseListStore;
/// The `/highlight` word list (`HighlightStore`, `IrcHighlightStore`).
pub type HighlightStore = CaseListStore;

impl CaseListStore {
    /// A store over `backing`, holding whatever the backing saved.
    pub fn new(backing: Box<dyn ListBacking>) -> Self {
        let lists = backing.load();
        Self { lists, backing }
    }

    /// A store that keeps its lists for the life of the process only.
    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self::new(Box::new(MemoryBacking::default()))
    }

    /// The stored values for `network_id`, in insertion order (`Nicks`, `Targets`,
    /// `Words`). Empty for an empty network id.
    pub fn entries(&self, network_id: &str) -> Vec<String> {
        if network_id.is_empty() {
            return Vec::new();
        }
        self.lists.get(network_id).cloned().unwrap_or_default()
    }

    /// The stored values with duplicates removed under `mapping` (`Listed`).
    pub fn listed(&self, network_id: &str, mapping: IrcCaseMapping) -> Vec<String> {
        let mut unique: Vec<String> = Vec::new();
        for value in self.entries(network_id) {
            if !contains_value(&unique, &value, mapping) {
                unique.push(value);
            }
        }
        unique
    }

    /// Whether `value` is stored for `network_id` under `mapping` (`Contains`).
    pub fn contains(&self, network_id: &str, value: &str, mapping: IrcCaseMapping) -> bool {
        if network_id.is_empty() || value.is_empty() {
            return false;
        }
        contains_value(&self.entries(network_id), value, mapping)
    }

    /// Stores `value` and reports whether it was newly added (`Add`). An empty
    /// network id or value, or a value already present, adds nothing.
    pub fn add(&mut self, network_id: &str, value: &str, mapping: IrcCaseMapping) -> bool {
        if network_id.is_empty() || value.is_empty() {
            return false;
        }
        let current = self.lists.entry(network_id.to_owned()).or_default();
        if contains_value(current, value, mapping) {
            return false;
        }
        current.push(value.to_owned());
        self.persist();
        true
    }

    /// Drops every value that matches `value` under `mapping` and reports whether
    /// anything changed (`Remove`). A network left with nothing is forgotten.
    pub fn remove(&mut self, network_id: &str, value: &str, mapping: IrcCaseMapping) -> bool {
        if network_id.is_empty() || value.is_empty() {
            return false;
        }
        let Some(current) = self.lists.get_mut(network_id) else {
            return false;
        };
        let before = current.len();
        current.retain(|candidate| !mapping.equals(candidate, value));
        if current.len() == before {
            return false;
        }
        if current.is_empty() {
            self.lists.remove(network_id);
        }
        self.persist();
        true
    }

    /// Drops the whole list for `network_id` (`Forget`). A removed network's open queries use it.
    pub fn forget(&mut self, network_id: &str) {
        if network_id.is_empty() {
            return;
        }
        if self.lists.remove(network_id).is_some() {
            self.persist();
        }
    }

    fn persist(&self) {
        self.backing.save(&self.lists);
    }
}

/// Whether any of `values` equals `value` under `mapping`.
pub(crate) fn contains_value(values: &[String], value: &str, mapping: IrcCaseMapping) -> bool {
    values
        .iter()
        .any(|candidate| mapping.equals(candidate, value))
}

/// Keeps a list store in the settings file, one `<group>/<network>/<key>` string list
/// per network with entries (`IrcIgnoreStore::save` and its siblings). The file is
/// read on every load and rewritten on every save; an ephemeral backing keeps nothing.
pub struct IniListBacking {
    backing: Backing,
    group: &'static str,
    key: &'static str,
}

impl IniListBacking {
    pub fn new(backing: Backing, group: &'static str, key: &'static str) -> Self {
        Self {
            backing,
            group,
            key,
        }
    }
}

impl ListBacking for IniListBacking {
    fn load(&self) -> BTreeMap<String, Vec<String>> {
        let settings = self.backing.settings();
        let mut lists = BTreeMap::new();
        for network_id in settings.child_groups(self.group) {
            let group = format!("{}/{network_id}", self.group);
            let values = settings.string_list(&group, self.key).unwrap_or_default();
            if !values.is_empty() {
                lists.insert(network_id, values);
            }
        }
        lists
    }

    fn save(&self, lists: &BTreeMap<String, Vec<String>>) {
        let group = self.group;
        let key = self.key;
        let _ = self.backing.edit(|settings: &mut Settings| {
            settings.remove_group(group);
            for (network_id, values) in lists {
                if !values.is_empty() {
                    settings.set_string_list(&format!("{group}/{network_id}"), key, values);
                }
            }
        });
    }
}

impl CaseListStore {
    /// Renames `old` to `new` under `mapping` (`Rekey`). A spelling change keeps the
    /// entry's place. A `new` that already exists absorbs `old`. Returns whether
    /// anything changed.
    pub fn rekey(&mut self, network_id: &str, old: &str, new: &str, mapping: IrcCaseMapping) -> bool {
        if network_id.is_empty() || old.is_empty() || new.is_empty() {
            return false;
        }
        let Some(current) = self.lists.get_mut(network_id) else {
            return false;
        };
        let Some(index) = current.iter().position(|candidate| mapping.equals(candidate, old)) else {
            return false;
        };
        if mapping.equals(&current[index], new) {
            if current[index] == new {
                return false;
            }
            current[index] = new.to_owned();
        } else {
            current.remove(index);
            if !contains_value(current, new, mapping) {
                current.push(new.to_owned());
            }
        }
        self.persist();
        true
    }
}

/// The controller's closed list, shared with the reducer. The controller keeps one
/// handle and gives the reducer a clone, so both see the same lists.
impl ClosedStore for Rc<RefCell<CaseListStore>> {
    fn contains(&self, network_id: &str, target: &str, mapping: IrcCaseMapping) -> bool {
        self.borrow().contains(network_id, target, mapping)
    }

    fn add(&self, network_id: &str, target: &str, mapping: IrcCaseMapping) {
        self.borrow_mut().add(network_id, target, mapping);
    }

    fn remove(&self, network_id: &str, target: &str, mapping: IrcCaseMapping) {
        self.borrow_mut().remove(network_id, target, mapping);
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
    fn add_is_case_mapped_and_a_duplicate_is_a_no_op() {
        let mut store = IgnoreStore::in_memory();
        assert!(store.add("net", "Foo", rfc1459()));
        assert!(!store.add("net", "foo", rfc1459()), "a case-variant is a duplicate");
        assert_eq!(store.entries("net"), ["Foo"], "the first spelling is kept");
        assert!(store.contains("net", "FOO", rfc1459()));
        assert!(!store.add("", "x", rfc1459()), "an empty network adds nothing");
        assert!(!store.add("net", "", rfc1459()), "an empty value adds nothing");
    }

    #[test]
    fn remove_drops_every_case_variant_and_forgets_an_emptied_network() {
        let mut store = MuteStore::in_memory();
        store.add("net", "one", rfc1459());
        store.add("net", "two", rfc1459());
        assert!(store.remove("net", "ONE", rfc1459()));
        assert!(!store.remove("net", "one", rfc1459()), "already gone");
        assert_eq!(store.entries("net"), ["two"]);
        assert!(store.remove("net", "two", rfc1459()));
        assert!(store.entries("net").is_empty());
        assert!(!store.remove("", "two", rfc1459()));
    }

    #[test]
    fn lists_are_per_network_and_forget_drops_one() {
        let mut store = HighlightStore::in_memory();
        store.add("a", "deploy", rfc1459());
        store.add("b", "deploy", rfc1459());
        store.forget("a");
        assert!(store.entries("a").is_empty());
        assert_eq!(store.entries("b"), ["deploy"]);
        assert!(store.entries("").is_empty(), "an empty network id has no list");
    }

    #[test]
    fn listed_dedupes_under_the_mapping_but_entries_keep_every_stored_value() {
        // A list built straight from the backing can hold two spellings of one nick.
        let backing = MemoryBacking::default();
        backing.save(&BTreeMap::from([(
            "net".to_owned(),
            vec!["Anna".to_owned(), "anna".to_owned(), "dax".to_owned()],
        )]));
        let store = MonitorStore::new(Box::new(backing));
        assert_eq!(store.listed("net", rfc1459()), ["Anna", "dax"]);
        assert_eq!(store.entries("net").len(), 3);
    }

    #[test]
    fn a_store_built_over_the_same_backing_sees_what_was_saved() {
        let backing = MemoryBacking::default();
        let mut first = IgnoreStore::new(Box::new(backing.clone()));
        first.add("net", "Spam", rfc1459());
        first.add("net", "bot", rfc1459());
        first.remove("net", "bot", rfc1459());

        let second = IgnoreStore::new(Box::new(backing));
        assert_eq!(second.entries("net"), ["Spam"], "the restart reads the saved list");
    }

    #[test]
    fn a_no_op_change_does_not_save() {
        let backing = MemoryBacking::default();
        let mut store = MuteStore::new(Box::new(backing.clone()));
        store.add("net", "one", rfc1459());
        // Put a sentinel in the backing; a no-op must not overwrite it.
        backing.save(&BTreeMap::from([("net".to_owned(), vec!["sentinel".to_owned()])]));
        assert!(!store.add("net", "ONE", rfc1459()));
        assert!(!store.remove("net", "missing", rfc1459()));
        store.forget("other");
        assert_eq!(backing.load()["net"], ["sentinel"]);
    }


    #[test]
    fn ini_lists_survive_a_restart_under_the_qt_group_layout() {
        let dir = TempDir::new("lists");
        let path = dir.path().join("omairc.conf");
        let backing = Backing::Disk(path.clone());
        let mut first = MuteStore::new(Box::new(IniListBacking::new(backing.clone(), "mutes", "targets")));
        first.add("net", "#ricing", rfc1459());
        first.add("net", "anna", rfc1459());
        first.add("other", "dax", rfc1459());

        let text = std::fs::read_to_string(&path).expect("the INI is written");
        assert!(text.contains("[mutes]"), "the group is the Qt name: {text}");
        assert!(text.contains("targets="), "the key is the Qt name: {text}");

        let second = MuteStore::new(Box::new(IniListBacking::new(backing, "mutes", "targets")));
        assert_eq!(second.entries("net"), ["#ricing", "anna"]);
        assert_eq!(second.entries("other"), ["dax"]);
    }

    #[test]
    fn an_ephemeral_backing_writes_no_file() {
        let mut store = IgnoreStore::new(Box::new(IniListBacking::new(
            Backing::Ephemeral,
            "ignores",
            "nicks",
        )));
        assert!(store.add("net", "spam", rfc1459()));
        assert_eq!(store.entries("net"), ["spam"], "the process keeps its own copy");
    }

    #[test]
    fn rekey_renames_in_place_or_folds_into_an_existing_entry() {
        let mut store = MonitorStore::in_memory();
        store.add("net", "Anna", rfc1459());
        store.add("net", "dax", rfc1459());
        assert!(store.rekey("net", "anna", "anna", rfc1459()), "the spelling changes");
        assert_eq!(store.entries("net"), ["anna", "dax"]);
        assert!(store.rekey("net", "anna", "dax", rfc1459()), "a nick that already exists folds in");
        assert_eq!(store.entries("net"), ["dax"]);
        assert!(!store.rekey("net", "missing", "x", rfc1459()));
    }
}
