//! Port of `tui/internal/controller/stores.go` (Go) and `src/irc/ircignore.cpp`,
//! `src/irc/ircmute.cpp`, `src/irc/irchighlight.cpp` and the monitor list of
//! `src/irc/ircmonitorcoordinator.cpp` (C++).
//!
//! The four lists share one shape: a per-network, insertion-ordered list where
//! membership and removal compare through the network's case mapping, and adding
//! a duplicate is a no-op. `CaseListStore` is that shape once; `IgnoreStore`,
//! `MuteStore`, `MonitorStore` and `HighlightStore` are the same type under the
//! names the C++ and Go stores use.
//!
//! Persistence is a seam. A store loads its lists from a `ListBacking` when it is
//! built and saves them after each change. Phase 8 binds the backing to the INI
//! settings file; until then `MemoryBacking` keeps them for the process.
//!
//! The inbound ignore predicate (`ignore_drops_inbound`) and the usability checks
//! for `/ignore`, `/mute` and `/monitor` targets are pure functions here, so the
//! dispatcher and the session filter share one rule.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::irc::casemapping::IrcCaseMapping;
use crate::irc::message::IrcMessage;
use crate::irc::prefixnick::prefix_nick;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::viewnotify::ViewNotify;
use crate::irc::wiretext::wire_text;

use super::controller::Controller;

/// Where a store's lists are kept between runs. `load` returns what an earlier run
/// saved, keyed by network id; `save` replaces it with the current lists.
pub trait ListBacking {
    fn load(&self) -> BTreeMap<String, Vec<String>>;
    fn save(&self, lists: &BTreeMap<String, Vec<String>>);
}

/// A backing that keeps the lists in memory. Clones share the same lists, so a store
/// built over a clone sees what an earlier store saved, as a restart would.
#[derive(Clone, Default)]
pub struct MemoryBacking(Rc<RefCell<BTreeMap<String, Vec<String>>>>);

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

    /// Drops the whole list for `network_id` (`Forget`).
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
fn contains_value(values: &[String], value: &str, mapping: IrcCaseMapping) -> bool {
    values
        .iter()
        .any(|candidate| mapping.equals(candidate, value))
}

/// Whether `nick` is a plausible `/ignore` or `/monitor` target: non-empty, not a
/// channel, and free of the wildcard and mask punctuation `!`, `@`, `*` and `,`
/// (`ignoreNickIsUsable`).
pub fn ignore_nick_is_usable(nick: &str, features: &IrcServerFeatures) -> bool {
    if nick.is_empty() || features.is_channel(nick) {
        return false;
    }
    !nick.contains(['!', '@', '*', ','])
}

/// Whether `target` may be muted: a channel, or a nick that passes
/// `ignore_nick_is_usable` (`muteTargetIsUsable`).
pub fn mute_target_is_usable(target: &str, features: &IrcServerFeatures) -> bool {
    if target.is_empty() {
        return false;
    }
    if features.is_channel(target) {
        return true;
    }
    ignore_nick_is_usable(target, features)
}

/// The inbound ignore predicate (`ircIgnoreDropsInbound`). A message from an ignored
/// nick is dropped when it is a NOTICE or an INVITE, or a PRIVMSG or TAGMSG sent to
/// us directly. Channel traffic from an ignored nick still arrives, as in Qt.
pub fn ignore_drops_inbound(
    message: &IrcMessage,
    self_nick: &str,
    nicks: &[String],
    features: &IrcServerFeatures,
) -> bool {
    let sender = prefix_nick(message);
    if sender.is_empty() || nicks.is_empty() {
        return false;
    }
    let mapping = features.case_mapping();
    if !contains_value(nicks, &sender, mapping) {
        return false;
    }
    match message.command.as_str() {
        "NOTICE" | "INVITE" => true,
        "PRIVMSG" | "TAGMSG" => {
            let target = message.param(0).map(wire_text).unwrap_or_default();
            if target.is_empty() || features.is_channel(&target) {
                return false;
            }
            mapping.equals(&target, self_nick)
        }
        _ => false,
    }
}

impl Controller {
    /// The `/ignore` nicks of `network_id`, deduplicated under its case mapping.
    pub fn ignored_nicks(&self, network_id: &str) -> Vec<String> {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.ignores.borrow().listed(network_id, mapping)
    }

    /// Adds a nick to the ignore list and reports whether it was newly added. The
    /// session's inbound filter reads the same list, so the change applies at once.
    pub fn ignore_nick(&mut self, network_id: &str, nick: &str) -> bool {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.ignores.borrow_mut().add(network_id, nick, mapping)
    }

    /// Removes a nick from the ignore list and reports whether anything changed.
    pub fn unignore_nick(&mut self, network_id: &str, nick: &str) -> bool {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.ignores.borrow_mut().remove(network_id, nick, mapping)
    }

    /// Stores a mute, or lifts one, and sets the conversation's muted flag to match
    /// (`applyMute`). Returns whether the mute list changed.
    pub fn apply_mute(&mut self, network_id: &str, target: &str, muted: bool) -> bool {
        if network_id.is_empty() || target.is_empty() {
            return false;
        }
        let mapping = self.reducer.server_features(network_id).case_mapping();
        let changed = if muted {
            self.mutes.add(network_id, target, mapping)
        } else {
            self.mutes.remove(network_id, target, mapping)
        };
        let key = self.reducer.conversation_key(network_id, target);
        self.reducer.set_muted(&key, muted);
        self.queue(vec![ViewNotify::Conversations]);
        changed
    }

    /// Whether `target` is on the mute list of `network_id`.
    pub fn is_muted_target(&self, network_id: &str, target: &str) -> bool {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.mutes.contains(network_id, target, mapping)
    }

    /// The `/highlight` words of `network_id`, deduplicated under its case mapping.
    pub fn highlight_words(&self, network_id: &str) -> Vec<String> {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.highlights.listed(network_id, mapping)
    }

    /// Adds a highlight word and feeds the list to the reducer. Returns whether the
    /// word was newly added.
    pub fn add_highlight_word(&mut self, network_id: &str, word: &str) -> bool {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        let changed = self.highlights.add(network_id, word, mapping);
        self.sync_highlight_words(network_id);
        changed
    }

    /// Removes a highlight word and feeds the list to the reducer. Returns whether
    /// anything changed.
    pub fn remove_highlight_word(&mut self, network_id: &str, word: &str) -> bool {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        let changed = self.highlights.remove(network_id, word, mapping);
        self.sync_highlight_words(network_id);
        changed
    }

    /// The `/monitor` nicks of `network_id`, deduplicated under its case mapping.
    pub fn watched_nicks(&self, network_id: &str) -> Vec<String> {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.monitors.listed(network_id, mapping)
    }

    /// Pushes the stored highlight words of `network_id` into the reducer
    /// (`syncHighlightWords`).
    pub(super) fn sync_highlight_words(&mut self, network_id: &str) {
        if network_id.is_empty() {
            return;
        }
        let words = self.highlights.entries(network_id);
        self.reducer.set_highlight_words(network_id, &words);
    }

    /// Wires a freshly created session to the stores: the inbound ignore filter, the
    /// highlight words, and the stored mutes (`AddSession`). Called again whenever
    /// the network's features change, because the filter captures the case mapping.
    pub(super) fn attach_store_hooks(&mut self, network_id: &str) {
        self.install_ignore_filter(network_id);
        self.sync_highlight_words(network_id);
        for target in self.mutes.entries(network_id) {
            let key = self.reducer.conversation_key(network_id, &target);
            self.reducer.set_muted(&key, true);
        }
    }

    /// Installs the inbound drop predicate on `network_id`'s session. The list is read
    /// live, so only the network's case mapping is captured here.
    pub(super) fn install_ignore_filter(&self, network_id: &str) {
        let Some(session) = self.session(network_id) else {
            return;
        };
        let ignores = Rc::clone(&self.ignores);
        let features = self.reducer.server_features(network_id).clone();
        let network = network_id.to_owned();
        session.set_ignore_filter(Some(Box::new(move |message: &IrcMessage, _: &str, self_nick: &str| {
            let nicks = ignores.borrow().entries(&network);
            ignore_drops_inbound(message, self_nick, &nicks, &features)
        })));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::casemapping::IrcCaseMappingKind;
    use crate::irc::message::IrcMessage;

    fn rfc1459() -> IrcCaseMapping {
        IrcCaseMapping::new(IrcCaseMappingKind::Rfc1459)
    }

    fn features() -> IrcServerFeatures {
        let mut features = IrcServerFeatures::new();
        features.apply_token("CHANTYPES=#&");
        features
    }

    fn line(text: &str) -> IrcMessage {
        crate::irc::parser::parse(text.as_bytes()).expect("a parseable line")
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
    fn nick_usability_rejects_channels_and_mask_punctuation() {
        let features = features();
        assert!(ignore_nick_is_usable("anna", &features));
        assert!(!ignore_nick_is_usable("", &features));
        assert!(!ignore_nick_is_usable("#omarchy", &features));
        for bad in ["a!b", "a@b", "a*", "a,b"] {
            assert!(!ignore_nick_is_usable(bad, &features), "{bad}");
        }
    }

    #[test]
    fn mute_targets_accept_channels_and_usable_nicks() {
        let features = features();
        assert!(mute_target_is_usable("#omarchy", &features));
        assert!(mute_target_is_usable("anna", &features));
        assert!(!mute_target_is_usable("a*", &features));
        assert!(!mute_target_is_usable("", &features));
    }

    #[test]
    fn ignored_notices_and_invites_are_dropped_from_anyone_ignored() {
        let features = features();
        let nicks = vec!["Spam".to_owned()];
        let notice = line(":spam!u@h NOTICE fred :hi");
        assert!(ignore_drops_inbound(&notice, "fred", &nicks, &features));
        let invite = line(":spam!u@h INVITE fred #room");
        assert!(ignore_drops_inbound(&invite, "fred", &nicks, &features));
    }

    #[test]
    fn ignored_privmsg_is_dropped_only_when_it_is_addressed_to_us() {
        let features = features();
        let nicks = vec!["spam".to_owned()];
        let direct = line(":SPAM!u@h PRIVMSG fred :hi");
        assert!(ignore_drops_inbound(&direct, "fred", &nicks, &features), "case-mapped sender");
        let channel = line(":spam!u@h PRIVMSG #omarchy :hi");
        assert!(!ignore_drops_inbound(&channel, "fred", &nicks, &features));
        let other_dm = line(":spam!u@h PRIVMSG anna :hi");
        assert!(!ignore_drops_inbound(&other_dm, "fred", &nicks, &features));
        let tagmsg = line(":spam!u@h TAGMSG fred");
        assert!(ignore_drops_inbound(&tagmsg, "fred", &nicks, &features));
    }

    #[test]
    fn other_traffic_and_unlisted_senders_pass() {
        let features = features();
        let nicks = vec!["spam".to_owned()];
        let kind = line(":spam!u@h JOIN #omarchy");
        assert!(!ignore_drops_inbound(&kind, "fred", &nicks, &features));
        let stranger = line(":anna!u@h NOTICE fred :hi");
        assert!(!ignore_drops_inbound(&stranger, "fred", &nicks, &features));
        let anything = line(":spam!u@h NOTICE fred :hi");
        assert!(!ignore_drops_inbound(&anything, "fred", &[], &features), "no list, no drop");
    }
}
