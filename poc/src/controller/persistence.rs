//! What the controller keeps between runs, and what each intent saves. Port of the
//! Phase 8 parts of `tui/internal/controller/controller.go` (`LoadStoredPreferences`,
//! `SetReopenDirects`, `AddSession`, `restoreOpenDirects`, `noteOpenDirectsMotd`,
//! `rememberOpenDirect`) and of `src/irc/irccontroller.cpp` and `src/irc/ircconnection.cpp`
//! (C++).
//!
//! The controller owns the state. The Connect sheet asks for a save (`save_profile`) and a
//! start (`Controller::start`), and reads what was saved (`stored_profiles`). Every write
//! goes through a store bound to the controller's `Backing`, so an ephemeral controller (the
//! demo, the tests) writes nothing.
//!
//! A profile that is complete registers a session at load. `start` activates it through the
//! transport factory: the build without a socket refuses, and the refusal is the network's
//! Status line, as any other start failure.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::desktop::notifier::NullNotifier;
use crate::irc::conversation::target_looks_like_service;
use crate::irc::event::IrcConversationCause;
use crate::irc::pref::PrefName;
use crate::irc::viewnotify::ViewNotify;
use crate::session::clock::Clock;
use crate::session::config::SessionConfig;
use crate::session::events::SessionState;
use crate::storage::credentialstore::{CredentialKey, CredentialState, PURPOSE_NICKSERV, PURPOSE_PASSWORD};
use crate::storage::profilestore::Profile;
use crate::storage::settings::Status;
use crate::storage::Backing;

use super::controller::Controller;
use super::transports::LiveTransports;

/// The secrets an Apply hands over. `None` leaves a slot as it is; `Some("")` removes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SecretEdits {
    pub password: Option<String>,
    pub nickserv: Option<String>,
}

/// What saving a profile did: the file's outcome, and where each secret now lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveOutcome {
    pub status: Status,
    pub password: CredentialState,
    pub nickserv: CredentialState,
}

impl Controller {
    /// A controller over the user's settings file, with the saved state loaded and each
    /// complete profile's session registered. Call `activate_startup` after it.
    pub fn new_user(clock: Rc<dyn Clock>) -> Self {
        let mut controller = Self::with_backing(
            clock,
            Box::new(NullNotifier),
            Box::new(LiveTransports),
            Backing::user(),
        );
        controller.load_stored();
        controller
    }

    /// Loads the saved preferences, sidebar layout and auto-away settings, and the Connect
    /// profiles. A complete profile registers its session (`AddSession`). An ephemeral
    /// controller has nothing to load.
    pub fn load_stored(&mut self) {
        if self.backing.is_ephemeral() {
            return;
        }
        self.prefs = self.preference_store.preferences();
        self.reducer.set_membership_noise(self.prefs.joins);
        self.autoaway.config = self.preference_store.autoaway();
        self.network_order = self.preference_store.network_order();
        self.network_collapsed = self.preference_store.collapsed_networks().into_iter().collect();
        for profile in self.profile_store.profiles() {
            self.profiles.insert(profile.network_id.clone(), profile);
        }
        let complete: Vec<Profile> = self
            .profiles
            .values()
            .filter(|profile| profile.is_complete())
            .cloned()
            .collect();
        for profile in complete {
            self.register_session(&profile);
        }
    }

    /// Starts each saved profile that asks to connect on startup, and returns how many
    /// started (`ActivateStartup`). A refused start shows on that network's Status.
    pub fn activate_startup(&mut self) -> usize {
        let ids: Vec<String> = self
            .profiles
            .values()
            .filter(|profile| profile.connect_on_startup && profile.is_complete())
            .map(|profile| profile.network_id.clone())
            .collect();
        ids.iter().filter(|id| self.start(id)).count()
    }

    /// The saved profile of a network, if it has one.
    pub fn profile(&self, network_id: &str) -> Option<&Profile> {
        self.profiles.get(network_id)
    }

    /// The saved profiles, in sidebar order (`Networks`).
    pub fn stored_profiles(&self) -> Vec<Profile> {
        self.network_order()
            .iter()
            .filter_map(|id| self.profiles.get(id).cloned())
            .collect()
    }

    /// Saves a profile and its secrets (`IrcConnection::apply`'s store half). The secrets
    /// are written first, then the profile file. The controller keeps the profile even when
    /// the file refuses, so the session sees the edit.
    pub fn save_profile(&mut self, mut profile: Profile, edits: SecretEdits) -> SaveOutcome {
        let password = self.store_secret(&profile.network_id, PURPOSE_PASSWORD, edits.password);
        let nickserv = self.store_secret(&profile.network_id, PURPOSE_NICKSERV, edits.nickserv);
        // Only durable storage can say a secret is saved. The memory store never can.
        let durable = self.credentials.durable();
        profile.secret_saved = durable && password != CredentialState::Missing;
        profile.nickserv_saved = durable && nickserv != CredentialState::Missing;
        let status = self.profile_store.save(&profile);
        self.profiles.insert(profile.network_id.clone(), profile.clone());
        self.queue(vec![ViewNotify::Conversations, ViewNotify::Status]);
        SaveOutcome {
            status,
            password,
            nickserv,
        }
    }

    /// Forgets a network's profile and its secrets (`forgetNetwork`'s store half).
    pub(super) fn remove_profile(&mut self, network_id: &str) -> Status {
        self.applied_profiles.remove(network_id);
        for purpose in [PURPOSE_PASSWORD, PURPOSE_NICKSERV] {
            self.credentials.remove(&CredentialKey::new(network_id, purpose));
        }
        self.profiles.remove(network_id);
        self.profile_store.remove(network_id)
    }

    /// Starts a saved network (`Start`, for a network that is not a seeded demo). A session
    /// built from the same profile is reused. A changed profile, or none yet, rebuilds the
    /// session through the transport factory. A missing or incomplete profile is refused.
    pub(super) fn start_profile(&mut self, network_id: &str) -> bool {
        let profile = match self.profiles.get(network_id) {
            Some(profile) if profile.is_complete() => profile.clone(),
            // No usable profile: the build's answer for an unknown network decides the reason.
            _ => return self.refuse_start(network_id),
        };
        let reusable = self.applied_profiles.get(network_id) == Some(&profile)
            && self.manager.find(network_id).is_some();
        if !reusable {
            self.manager.discard(network_id);
            self.autoaway_forget_network(network_id);
            self.monitor_forget_presence(network_id);
            let transport = match self.transports.open(network_id) {
                Ok(transport) => transport,
                Err(message) => {
                    self.applied_profiles.remove(network_id);
                    self.refuse_with(network_id, &message);
                    return false;
                }
            };
            let config = self.session_config(&profile);
            if let Err(error) = self.manager.create(config, transport, self.clock.clone()) {
                self.refuse_with(network_id, &error.to_string());
                return false;
            }
            self.applied_profiles.insert(network_id.to_owned(), profile);
            self.attach_store_hooks(network_id);
        }
        self.manager.activate(network_id)
    }

    /// Records the channels a session autojoins, and saves them to the network's profile
    /// (`AutojoinChannelsChanged`). A network with no profile keeps nothing.
    pub(super) fn persist_autojoin(
        &mut self,
        network_id: &str,
        channels: Vec<String>,
        keys: BTreeMap<String, String>,
    ) {
        let Some(profile) = self.profiles.get_mut(network_id) else {
            return;
        };
        if profile.autojoin_channels == channels && profile.autojoin_keys == keys {
            return;
        }
        profile.autojoin_channels = channels;
        profile.autojoin_keys = keys;
        let saved = profile.clone();
        self.profile_store.save(&saved);
    }

    /// Saves the `/pref` values. An ephemeral controller keeps them in memory only.
    pub(super) fn persist_preferences(&mut self) {
        self.preference_store.save_preferences(&self.prefs);
    }

    /// Saves the sidebar order and the folded networks.
    pub(super) fn persist_network_layout(&mut self) {
        let order = self.network_order.clone();
        self.preference_store.save_network_order(&order);
        let collapsed: Vec<String> = self.network_collapsed.iter().cloned().collect();
        self.preference_store.save_collapsed_networks(&collapsed);
    }

    /// Saves the `/autoaway` settings. The one-shot reason is not kept.
    pub(super) fn persist_autoaway(&mut self) {
        let config = self.autoaway.config.clone();
        self.preference_store.save_autoaway(&config);
    }

    /// The side effects a changed `/pref` or Connect toggle has after it is stored: turning
    /// `directs` on restores the open queries of every registered network (`SetReopenDirects`).
    pub(super) fn after_preference_change(&mut self, name: PrefName) {
        self.persist_preferences();
        if name != PrefName::Directs || !self.prefs.directs {
            return;
        }
        for network_id in self.manager.network_ids() {
            let registered = self
                .manager
                .find(&network_id)
                .is_some_and(|session| session.state() == SessionState::Registered);
            if registered {
                self.restore_open_directs(&network_id);
            }
        }
    }

    /// The MOTD-end step for a network, once per connection (`noteOpenDirectsMotd`): restore
    /// the open queries, then release the held query playback into them.
    pub(super) fn note_open_directs_motd(&mut self, network_id: &str) {
        if network_id.is_empty() || !self.open_directs_motd_seen.insert(network_id.to_owned()) {
            return;
        }
        self.restore_open_directs(network_id);
        if self.reducer.release_pending_query_playback(network_id) {
            self.queue(vec![ViewNotify::Conversations, ViewNotify::Messages]);
        }
    }

    /// Reopens the saved queries that are not loaded (`restoreOpenDirects`). With `directs`
    /// off nothing is restored. After the first MOTD a query that no longer restores is
    /// pruned from the list.
    pub(super) fn restore_open_directs(&mut self, network_id: &str) {
        if !self.prefs.directs || network_id.is_empty() {
            return;
        }
        let prune = self.open_directs_motd_seen.contains(network_id);
        let features = self.reducer.server_features(network_id).clone();
        let mapping = features.case_mapping();
        let mut created = false;
        for target in self.open_directs.listed(network_id, mapping) {
            if !self.persistable_direct_target(network_id, &target) {
                if prune {
                    self.open_directs.remove(network_id, &target, mapping);
                }
                continue;
            }
            let key = self.reducer.conversation_key(network_id, &target);
            if self.reducer.find(&key).is_some() {
                continue;
            }
            if self
                .reducer
                .ensure_conversation(&key, &target, IrcConversationCause::Restore)
                .is_some()
            {
                created = true;
            } else if prune {
                self.open_directs.remove(network_id, &target, mapping);
            }
        }
        if created {
            self.queue(vec![ViewNotify::Conversations]);
        }
    }

    /// Remembers a direct message the user opened, sent in, or was replied in
    /// (`rememberOpenDirect`). A channel, a service and an unknown target are not kept.
    pub(super) fn remember_open_direct(&mut self, network_id: &str, target: &str) {
        if !self.persistable_direct_target(network_id, target) {
            return;
        }
        let key = self.reducer.conversation_key(network_id, target);
        let conversation = self.reducer.find(&key);
        if conversation.is_some_and(|conversation| conversation.is_channel()) {
            return;
        }
        let stored = conversation
            .map(|conversation| conversation.target.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| target.to_owned());
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.open_directs.undismiss(network_id, &stored, mapping);
        self.open_directs.add(network_id, &stored, mapping);
    }

    /// Forgets a direct message the user closed, and dismisses it so a message from it does
    /// not bring it back (`forgetOpenDirect`).
    pub(super) fn forget_open_direct(&mut self, network_id: &str, target: &str) {
        if network_id.is_empty() || target.is_empty() {
            return;
        }
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.open_directs.remove(network_id, target, mapping);
        self.open_directs.dismiss(network_id, target, mapping);
    }

    /// A line we sent into an existing direct message means the user engaged the query, so
    /// it is remembered for restore (`noteSelfAuthoredDirect`).
    pub(super) fn note_self_authored_direct(&mut self, network_id: &str, author: &str, target: &str) {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        if !mapping.equals(author, &self.current_nick(network_id)) {
            return;
        }
        let key = self.reducer.conversation_key(network_id, target);
        if self
            .reducer
            .find(&key)
            .is_some_and(|conversation| !conversation.is_channel())
        {
            self.remember_open_direct(network_id, target);
        }
    }

    /// A NICK moves the saved queries, dismissals and scroll places with the person
    /// (`Rekey`).
    pub(super) fn rekey_nick_lists(&mut self, network_id: &str, old: &str, new: &str) {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.open_directs.rekey(network_id, old, new, mapping);
        self.open_directs.rekey_dismissed(network_id, old, new, mapping);
        self.scroll_places.rekey(network_id, old, new, mapping);
    }

    /// Whether `target` is a direct message worth keeping: a name that is not a channel and
    /// does not look like a network service (`persistableDirectTarget`).
    fn persistable_direct_target(&self, network_id: &str, target: &str) -> bool {
        if network_id.is_empty() || target.is_empty() {
            return false;
        }
        let features = self.reducer.server_features(network_id);
        if features.is_channel(target) {
            return false;
        }
        !target_looks_like_service(target, features)
    }

    /// Writes one secret slot and reports where it now is. `None` leaves the slot as it is,
    /// and reports whether the store already holds it.
    fn store_secret(&mut self, network_id: &str, purpose: &'static str, edit: Option<String>) -> CredentialState {
        let key = CredentialKey::new(network_id, purpose);
        match edit {
            None if self.credentials.read(&key).is_some() => CredentialState::SessionOnly,
            None => CredentialState::Missing,
            Some(text) if text.is_empty() => self.credentials.remove(&key),
            Some(text) => self.credentials.write(&key, &text),
        }
    }

    /// Builds and registers a session for a profile through the transport factory. A
    /// refused transport leaves no session, and the start reports the reason.
    fn register_session(&mut self, profile: &Profile) {
        if self.manager.find(&profile.network_id).is_some() {
            return;
        }
        let Ok(transport) = self.transports.open(&profile.network_id) else {
            return;
        };
        let config = self.session_config(profile);
        if self
            .manager
            .create(config, transport, self.clock.clone())
            .is_ok()
        {
            self.applied_profiles
                .insert(profile.network_id.clone(), profile.clone());
            self.attach_store_hooks(&profile.network_id);
        }
    }

    /// The session settings a profile gives (`IrcConnection::reconcile`'s config).
    fn session_config(&self, profile: &Profile) -> SessionConfig {
        let nick = profile.nick.trim();
        let mut config = SessionConfig::with_defaults(
            &profile.network_id,
            &profile.resolved_name(),
            profile.host.trim(),
            nick,
        );
        config.port = profile.port;
        config.tls_enabled = profile.tls;
        if !profile.username.trim().is_empty() {
            config.username = profile.username.trim().to_owned();
        }
        if !profile.realname.trim().is_empty() {
            config.realname = profile.realname.trim().to_owned();
        }
        config.password = self.secret_text(&profile.network_id, PURPOSE_PASSWORD);
        config.nickserv_password = self.secret_text(&profile.network_id, PURPOSE_NICKSERV);
        config.sasl_account = profile.account.clone();
        config.autojoin_channels = profile.autojoin_channels.clone();
        config.autojoin_keys = profile.autojoin_keys.clone();
        config
    }

    fn secret_text(&self, network_id: &str, purpose: &'static str) -> String {
        self.credentials
            .read(&CredentialKey::new(network_id, purpose))
            .unwrap_or_default()
    }
}
