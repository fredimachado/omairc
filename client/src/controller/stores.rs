//! Port of `tui/internal/controller/stores.go`,
//! `tui/internal/controller/stores.go`, `tui/internal/controller/stores.go` and the monitor list of
//! `tui/internal/controller/monitorcoordinator.go`.
//!
//! The four lists are `CaseListStore`s, which live in `storage/lists.rs` with their
//! persistence: a `ListBacking` loads them when a store is built and saves them after
//! each change. The controller binds each one to the settings file.
//!
//! The inbound ignore predicate (`ignore_drops_inbound`) and the usability checks
//! for `/ignore`, `/mute` and `/monitor` targets are pure functions here, so the
//! dispatcher and the session filter share one rule.

use std::rc::Rc;

use crate::irc::message::IrcMessage;
use crate::irc::prefixnick::prefix_nick;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::viewnotify::ViewNotify;
use crate::irc::wiretext::wire_text;
use crate::storage::lists::contains_value;
pub use crate::storage::lists::{HighlightStore, IgnoreStore, MonitorStore, MuteStore};

use super::controller::Controller;


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
    // The sidebar's mute mark and the product CLI read these (Phases 5 and 9); cfg(test) comes off with the first reader.
    #[cfg(test)]
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
    // The sidebar's mute mark and the product CLI read these (Phases 5 and 9); cfg(test) comes off with the first reader.
    #[cfg(test)]
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
        self.install_history_resume(network_id);
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
        self.install_history_resume(network_id);
    }

    /// Wires CHATHISTORY self-JOIN resume for `network_id`.
    pub(super) fn install_history_resume(&self, network_id: &str) {
        let Some(session) = self.manager.find(network_id) else {
            return;
        };
        let playback = Rc::clone(&self.playback);
        let times = Rc::clone(&self.playback_times);
        let net = network_id.to_owned();
        session.set_history_resume(Some(Box::new(move |target, mapping| {
            let coordinator = playback.borrow();
            coordinator.history_resume_instant(&net, target, mapping, &times.borrow())
        })));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::message::IrcMessage;

    fn features() -> IrcServerFeatures {
        let mut features = IrcServerFeatures::new();
        features.apply_token("CHANTYPES=#&");
        features
    }

    fn line(text: &str) -> IrcMessage {
        crate::irc::parser::parse(text.as_bytes()).expect("a parseable line")
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
