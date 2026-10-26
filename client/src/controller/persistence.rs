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
//! transport factory the shell hands in: the live one dials through the net hub, and a refusal
//! is the network's Status line, as any other start failure.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::irc::avatarurl::avatar_metadata_value;
use crate::irc::capability::IrcCapability;
use crate::irc::conversation::target_looks_like_service;
use crate::irc::event::IrcConversationCause;
use crate::irc::pref::PrefName;
use crate::irc::presence::AVATAR_KEY;
use crate::irc::viewnotify::ViewNotify;
use crate::session::clock::Clock;
use crate::session::config::SessionConfig;
use crate::session::events::SessionState;
use crate::storage::conversationlog::FileConversationLog;
use crate::irc::event::Event;
use crate::storage::credentialstore::{
    read_slot, read_slot_sync, remove_slot, write_slot, CredentialKey, CredentialState,
    LEGACY_PURPOSE_PASSWORD, PURPOSE_NICKSERV, PURPOSE_PASSWORD, ReadSlot,
};
use crate::storage::paths;
use crate::storage::profilestore::{Profile, DEFAULT_PORT};
use crate::storage::settings::Status;
use crate::storage::Backing;
use crate::util::Secret;

use super::controller::Controller;
use super::transports::TransportFactory;

/// The secrets an Apply hands over. `None` leaves a slot as it is; an empty `Secret` removes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SecretEdits {
    pub password: Option<Secret>,
    pub nickserv: Option<Secret>,
}

/// The profile and secrets a live session was built from (`IrcAppliedSession`).
#[derive(Clone, PartialEq, Eq)]
pub struct AppliedSession {
    pub profile: Profile,
    pub password: Secret,
    pub nickserv_password: Secret,
}

/// What saving a profile did: the file's outcome, and where each secret now lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveOutcome {
    pub status: Status,
    pub password: CredentialState,
    pub nickserv: CredentialState,
}

impl Controller {
    /// A controller over the user's settings file, with the saved state loaded and each
    /// complete profile's session registered. `transports` opens each network's transport.
    /// Call `activate_startup` after it.
    pub fn new_user(clock: Rc<dyn Clock>, transports: Box<dyn TransportFactory>) -> Self {
        let mut controller = Self::with_backing(clock, transports, Backing::user());
        if !controller.backing.is_ephemeral() {
            controller.reducer.set_conversation_log(Some(Box::new(
                FileConversationLog::new(paths::transcript_root()),
            )));
        }
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
            if self.profile_credentials_loading(&profile)
                || !self.startup_connect_allowed(&profile)
            {
                self.pending_session_registrations
                    .insert(profile.network_id.clone());
            } else {
                self.register_session(&profile);
            }
        }
    }

    /// Drains in-flight keychain reads and finishes session registration or startup connects
    /// that were waiting on them.
    pub(super) fn poll_credential_reads(&mut self) {
        self.credentials.poll_reads();
        let register: Vec<String> = self
            .profiles
            .values()
            .filter(|profile| {
                profile.is_complete()
                    && self.manager.find(&profile.network_id).is_none()
                    && !self.profile_credentials_loading(profile)
                    && self.startup_connect_allowed(profile)
            })
            .map(|profile| profile.network_id.clone())
            .collect();
        for network_id in register {
            if let Some(profile) = self.profiles.get(&network_id).cloned() {
                self.register_session(&profile);
            }
        }
        let waiting: Vec<String> = self
            .pending_session_registrations
            .iter()
            .filter(|network_id| {
                self.profiles.get(*network_id).is_some_and(|profile| {
                    !self.profile_credentials_loading(profile)
                        && self.startup_connect_allowed(profile)
                })
            })
            .cloned()
            .collect();
        for network_id in waiting {
            self.pending_session_registrations.remove(&network_id);
            if let Some(profile) = self.profiles.get(&network_id).cloned() {
                self.register_session(&profile);
            }
        }
        let startup: Vec<String> = self.pending_startup_connects.iter().cloned().collect();
        for network_id in startup {
            let profile = match self.profiles.get(&network_id) {
                Some(profile) if profile.is_complete() => profile.clone(),
                _ => {
                    self.pending_startup_connects.remove(&network_id);
                    continue;
                }
            };
            if self.profile_credentials_loading(&profile) {
                continue;
            }
            self.pending_startup_connects.remove(&network_id);
            if self.startup_connect_allowed(&profile) {
                let _ = self.start(&network_id);
            } else {
                self.refuse_with(
                    &network_id,
                    "The saved password could not be read from secure storage.",
                );
            }
        }
    }

    /// Starts each saved profile that asks to connect on startup, and returns how many
    /// started (`ActivateStartup`). A refused start shows on that network's Status.
    pub fn activate_startup(&mut self) -> usize {
        self.poll_credential_reads();
        let ids: Vec<String> = self
            .profiles
            .values()
            .filter(|profile| profile.connect_on_startup && profile.is_complete())
            .map(|profile| profile.network_id.clone())
            .collect();
        let mut started = 0;
        for id in ids {
            let profile = self.profiles.get(&id).expect("filtered above");
            if self.profile_credentials_loading(profile) {
                self.pending_startup_connects.insert(id);
                continue;
            }
            if !self.startup_connect_allowed(profile) {
                continue;
            }
            if self.start(&id) {
                started += 1;
            }
        }
        started
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
        // The saved profile still names the keys the secrets are under, so a changed host,
        // username or nick moves them (`IrcConnection::apply`'s rekey).
        let stored = self.profiles.get(&profile.network_id).cloned();
        let (was_saved, was_nickserv_saved) = stored
            .as_ref()
            .map_or((false, false), |stored| (stored.secret_saved, stored.nickserv_saved));
        let password =
            self.store_secret(stored.as_ref(), &profile, PURPOSE_PASSWORD, edits.password);
        let nickserv =
            self.store_secret(stored.as_ref(), &profile, PURPOSE_NICKSERV, edits.nickserv);
        profile.secret_saved = saved_flag(&password, was_saved);
        profile.nickserv_saved = saved_flag(&nickserv, was_nickserv_saved);
        let status = self.profile_store.save(&profile);
        self.profiles.insert(profile.network_id.clone(), profile.clone());
        self.queue(vec![ViewNotify::Conversations, ViewNotify::Status]);
        SaveOutcome {
            status,
            password,
            nickserv,
        }
    }

    /// Deletes one secret slot from the keychain immediately (`removeStoredPassword` /
    /// `removeStoredNickServ`). Clears the profile's saved flag once the key is gone.
    /// Returns `Missing` when gone, or `Error` when a delete failed so forget stays up.
    pub fn forget_secret(&mut self, network_id: &str, purpose: &'static str) -> CredentialState {
        let Some(mut profile) = self.profiles.get(network_id).cloned() else {
            return CredentialState::Missing;
        };
        self.queue_slot_removals(&profile, purpose);
        let mut remove_error = None;
        for key in self.credentials.pending().slot(network_id, purpose) {
            if let CredentialState::Error(message) = self.delete_key(&key) {
                remove_error = Some(message);
            }
        }
        let settled = remove_error
            .map(CredentialState::Error)
            .unwrap_or_else(|| self.credential_slot_state(&profile, purpose));
        if matches!(settled, CredentialState::Missing) {
            match purpose {
                PURPOSE_PASSWORD => profile.secret_saved = false,
                PURPOSE_NICKSERV => profile.nickserv_saved = false,
                _ => {}
            }
            let _ = self.profile_store.save(&profile);
            self.profiles.insert(network_id.to_owned(), profile);
        }
        settled
    }

    /// Forgets a network's profile and its secrets (`forgetNetwork`'s store half). The current
    /// keys and every obsolete key of the network are deleted; one the store refuses stays owed.
    pub(super) fn remove_profile(&mut self, network_id: &str) -> Status {
        let profile = self.profiles.get(network_id).cloned();
        if profile.is_none() {
            return Status::Absent;
        }
        let status = self.profile_store.remove(network_id);
        if matches!(status, Status::AccessError | Status::FormatError) {
            return status;
        }
        self.applied_sessions.remove(network_id);
        if let Some(profile) = profile {
            self.queue_slot_removals(&profile, PURPOSE_PASSWORD);
            self.queue_slot_removals(&profile, PURPOSE_NICKSERV);
        }
        for key in self.credentials.pending().network(network_id) {
            self.delete_key(&key);
        }
        self.profiles.remove(network_id);
        status
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
        if self.profile_credentials_loading(&profile) {
            self.pending_startup_connects.insert(network_id.to_owned());
            return false;
        }
        if !self.startup_connect_allowed(&profile) {
            self.refuse_with(
                network_id,
                "The saved password could not be read from secure storage.",
            );
            return false;
        }
        let candidate = self.applied_session_for(&profile);
        let reusable = self.applied_sessions.get(network_id) == Some(&candidate)
            && self.manager.find(network_id).is_some();
        if !reusable {
            self.manager.discard(network_id);
            self.applied_profile_avatars.remove(network_id);
            self.autoaway_forget_network(network_id);
            self.monitor_forget_presence(network_id);
            let transport = match self.transports.open(network_id) {
                Ok(transport) => transport,
                Err(message) => {
                    self.applied_sessions.remove(network_id);
                    self.refuse_with(network_id, &message);
                    return false;
                }
            };
            let config = self.session_config(&profile);
            if let Err(error) = self.manager.create(config, transport, self.clock.clone()) {
                self.refuse_with(network_id, &error.to_string());
                return false;
            }
            self.applied_sessions.insert(network_id.to_owned(), candidate);
            self.attach_store_hooks(network_id);
        }
        self.manager.activate(network_id)
    }

    /// Whether a saved network may connect with its declared secrets (`startupConnectAllowed`).
    pub(super) fn startup_connect_allowed(&self, profile: &Profile) -> bool {
        !self.secret_unreadable(profile, PURPOSE_PASSWORD, profile.secret_saved, true)
            && !self.secret_unreadable(
                profile,
                PURPOSE_NICKSERV,
                profile.nickserv_saved,
                false,
            )
    }

    /// Whether Apply may start a network after `save_profile` (`submitConnection`).
    pub fn connect_allowed_after_save(&self, profile: &Profile, saved: &SaveOutcome) -> bool {
        if matches!(saved.password, CredentialState::Error(_))
            || matches!(saved.nickserv, CredentialState::Error(_))
        {
            return false;
        }
        if matches!(saved.password, CredentialState::Loading)
            || matches!(saved.nickserv, CredentialState::Loading)
        {
            return true;
        }
        self.startup_connect_allowed(profile)
    }

    /// Stores the avatar URL the server confirmed into the network's profile
    /// (`IrcConnection::persistAvatarUrl`).
    pub(super) fn persist_avatar_url(&mut self, network_id: &str, url: &str) {
        if network_id.is_empty() {
            return;
        }
        let Some(profile) = self.profiles.get_mut(network_id) else {
            return;
        };
        if profile.avatar_url == url {
            return;
        }
        profile.avatar_url = url.to_owned();
        let saved = profile.clone();
        self.profile_store.save(&saved);
        if let Some(applied) = self.applied_sessions.get_mut(network_id) {
            applied.profile.avatar_url = url.to_owned();
        }
    }

    /// Publishes the saved avatar once per registration when metadata caps are ready
    /// (`IrcController::applyProfileAvatarOnConnect`).
    pub(super) fn apply_profile_avatar_on_connect(&mut self, network_id: &str) {
        if network_id.is_empty() || self.applied_profile_avatars.contains(network_id) {
            return;
        }
        let avatar_url = self
            .profiles
            .get(network_id)
            .and_then(|profile| avatar_metadata_value(&profile.avatar_url))
            .unwrap_or_default();
        if avatar_url.is_empty() {
            return;
        }
        let capabilities = self
            .capabilities
            .get(network_id)
            .cloned()
            .unwrap_or_default();
        if !capabilities.contains(IrcCapability::MemberMetadata)
            || !capabilities.contains(IrcCapability::Batch)
        {
            return;
        }
        let Some(session) = self.manager.find(network_id) else {
            return;
        };
        if session.set_own_metadata(AVATAR_KEY, &avatar_url).is_some() {
            self.applied_profile_avatars.insert(network_id.to_owned());
        }
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
        profile.autojoin_channels = channels.clone();
        profile.autojoin_keys = keys.clone();
        let saved = profile.clone();
        self.profile_store.save(&saved);
        if let Some(applied) = self.applied_sessions.get_mut(network_id) {
            applied.profile.autojoin_channels = channels;
            applied.profile.autojoin_keys = keys;
        }
        self.background_profile_syncs.insert(network_id.to_owned());
    }

    /// Network ids whose saved profile changed in the background since the last take.
    pub fn take_background_profile_syncs(&mut self) -> Vec<String> {
        std::mem::take(&mut self.background_profile_syncs).into_iter().collect()
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
        let spliced = self.reducer.release_pending_query_playback(network_id);
        self.note_kept_replay();
        if spliced {
            self.queue(vec![ViewNotify::Conversations, ViewNotify::Messages]);
        }
        if let Some(session) = self.manager.find(network_id) {
            self.request_znc_playback(&session);
            self.request_chat_history_catch_up(&session);
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
        self.closed
            .borrow_mut()
            .remove(network_id, &stored, mapping);
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
        self.closed.borrow_mut().add(network_id, target, mapping);
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
        self.closed.borrow_mut().rekey(network_id, old, new, mapping);
        self.scroll_places.rekey(network_id, old, new, mapping);
        self.playback_times
            .borrow_mut()
            .rekey(network_id, old, new, mapping);
        self.playback
            .borrow_mut()
            .rekey(&self.reducer, network_id, old, new);
    }

    /// A live message, notice, or action from someone else that is about to
    /// recreate a direct message the user closed (`otherReopenedDirect`).
    pub(super) fn other_reopened_direct(&self, event: &Event) -> Option<(String, String)> {
        let (key, author, stored) = match event {
            Event::Message(line) => (
                line.conversation.clone(),
                line.author.clone(),
                line.target.clone(),
            ),
            Event::Notice(line) => (
                line.conversation.clone(),
                line.author.clone(),
                line.target.clone(),
            ),
            Event::Action(line) => (
                line.conversation.clone(),
                line.author.clone(),
                line.target.clone(),
            ),
            _ => return None,
        };
        let features = self.reducer.server_features(&key.network_id);
        if features.is_channel(&key.normalized_target) {
            return None;
        }
        let mapping = features.case_mapping();
        if mapping.equals(&author, &self.current_nick(&key.network_id)) {
            return None;
        }
        if self.reducer.find(&key).is_some() {
            return None;
        }
        let stored = if stored.is_empty() {
            key.normalized_target.clone()
        } else {
            stored
        };
        let closed = self.closed.borrow();
        if !closed.contains(&key.network_id, &stored, mapping)
            && !closed.contains(&key.network_id, &key.normalized_target, mapping)
        {
            return None;
        }
        Some((key.network_id, stored))
    }

    /// Whether `target` is a direct message worth keeping: a name that is not a channel and
    /// does not look like a network service (`persistableDirectTarget`).
    pub(super) fn persistable_direct_target(&self, network_id: &str, target: &str) -> bool {
        if network_id.is_empty() || target.is_empty() {
            return false;
        }
        let features = self.reducer.server_features(network_id);
        if features.is_channel(target) {
            return false;
        }
        !target_looks_like_service(target, features)
    }

    /// Writes one secret slot of `profile` and reports where it now is. `stored` is the saved
    /// profile the slot came from. When its key differs from the new one, the old key is owed a
    /// delete (`IrcConnection::applySecret`'s rememberObsoleteKey). A typed secret replaces the
    /// slot, a blank one clears it, and `None` moves the secret from the old key, or leaves the
    /// slot as it is when the key did not change.
    fn store_secret(
        &mut self,
        stored: Option<&Profile>,
        profile: &Profile,
        purpose: &'static str,
        edit: Option<Secret>,
    ) -> CredentialState {
        let key = CredentialKey::for_profile(profile, purpose);
        let previous = stored
            .map(|stored| CredentialKey::for_profile(stored, purpose))
            .filter(|previous| *previous != key);
        if let Some(previous) = &previous {
            self.credentials.pending_mut().add(previous.clone());
        }
        match (edit, previous) {
            (None, Some(previous)) => self.carry_secret(&previous, profile, purpose),
            (None, None) => self.settled_state(&key),
            (Some(secret), _) => {
                let state = if secret.is_empty() {
                    remove_slot(self.credentials.as_mut(), profile, purpose)
                } else {
                    // The credential store takes the plaintext: it is the edge the secret is written to.
                    write_slot(self.credentials.as_mut(), profile, purpose, secret.expose())
                };
                if is_settled(&state) {
                    self.retire_obsolete(profile, purpose);
                }
                state
            }
        }
    }

    /// Moves a secret from the key a profile had to the key it has now, when Apply left the
    /// secret blank. The old key is deleted once the new one holds the secret. An old key that
    /// holds nothing is only deleted. A read the store cannot answer leaves both keys alone.
    fn carry_secret(
        &mut self,
        from: &CredentialKey,
        profile: &Profile,
        purpose: &'static str,
    ) -> CredentialState {
        let key = CredentialKey::for_profile(profile, purpose);
        let from_profile = profile_for_credential_key(from);
        match read_slot(self.credentials.as_ref(), &from_profile, from.purpose) {
            ReadSlot::Loading => CredentialState::Loading,
            ReadSlot::Ready(Ok(Some(text))) => {
                let mut carried = Secret::from(text);
                let state = write_slot(self.credentials.as_mut(), profile, purpose, carried.expose());
                carried.wipe();
                if is_settled(&state) {
                    self.retire_obsolete(profile, purpose);
                }
                state
            }
            ReadSlot::Ready(Ok(None)) => {
                let state = self.settled_state(&key);
                if is_settled(&state) {
                    self.delete_key(from);
                }
                state
            }
            ReadSlot::Ready(Err(message)) => CredentialState::Error(message),
        }
    }

    /// Deletes the owed keys of one slot once the slot's secret sits under its current key, or
    /// was cleared on purpose (`flushObsoleteKeys`). The current key itself is never deleted,
    /// even when an earlier rename left it owed.
    fn retire_obsolete(&mut self, profile: &Profile, purpose: &'static str) {
        let current = CredentialKey::for_profile(profile, purpose);
        let owed = self.credentials.pending().slot(&profile.network_id, purpose);
        for key in owed {
            if key == current {
                self.credentials.pending_mut().forget(&key);
            } else {
                self.delete_key(&key);
            }
        }
    }

    /// Deletes one key. A refused delete stays owed. Returns the store settle state.
    fn delete_key(&mut self, key: &CredentialKey) -> CredentialState {
        let state = self.credentials.remove(key);
        match &state {
            CredentialState::Missing => self.credentials.pending_mut().forget(key),
            _ => self.credentials.pending_mut().add(key.clone()),
        }
        state
    }

    /// Where a slot stands when nothing is written: held when its key has a secret, missing when
    /// it has none, and the store's error when it cannot say.
    fn settled_state(&self, key: &CredentialKey) -> CredentialState {
        let profile = profile_for_credential_key(key);
        self.credential_slot_state_from_read(
            read_slot(self.credentials.as_ref(), &profile, key.purpose),
        )
    }

    fn applied_session_for(&self, profile: &Profile) -> AppliedSession {
        AppliedSession {
            profile: profile.clone(),
            password: self.secret_text(profile, PURPOSE_PASSWORD),
            nickserv_password: self.secret_text(profile, PURPOSE_NICKSERV),
        }
    }

    fn queue_slot_removals(&mut self, profile: &Profile, purpose: &'static str) {
        self.credentials
            .pending_mut()
            .add(CredentialKey::for_profile(profile, purpose));
        if purpose == PURPOSE_PASSWORD {
            self.credentials.pending_mut().add(
                CredentialKey::for_profile(profile, LEGACY_PURPOSE_PASSWORD),
            );
        }
    }

    fn secret_unreadable(
        &self,
        profile: &Profile,
        purpose: &'static str,
        saved: bool,
        required: bool,
    ) -> bool {
        let tracked = required || saved;
        if tracked && self.slot_read_loading(profile, purpose) {
            return true;
        }
        match self.credential_slot_state(profile, purpose) {
            CredentialState::Error(_) | CredentialState::Loading => tracked,
            CredentialState::Missing | CredentialState::SessionOnly if saved => true,
            _ => false,
        }
    }

    fn credential_slot_state(&self, profile: &Profile, purpose: &'static str) -> CredentialState {
        self.credential_slot_state_from_read(
            read_slot(self.credentials.as_ref(), profile, purpose),
        )
    }

    fn credential_slot_state_from_read(&self, read: ReadSlot) -> CredentialState {
        match read {
            ReadSlot::Loading => CredentialState::Loading,
            ReadSlot::Ready(Ok(Some(text))) => {
                Secret::from(text).wipe();
                if self.credentials.durable() {
                    CredentialState::Available
                } else {
                    CredentialState::SessionOnly
                }
            }
            ReadSlot::Ready(Ok(None)) => CredentialState::Missing,
            ReadSlot::Ready(Err(message)) => CredentialState::Error(message),
        }
    }

    fn slot_read_loading(&self, profile: &Profile, purpose: &'static str) -> bool {
        let primary = CredentialKey::for_profile(profile, purpose);
        if self.credentials.deferred_read_inflight(&primary) {
            return true;
        }
        if purpose == PURPOSE_PASSWORD {
            let legacy = CredentialKey::for_profile(profile, LEGACY_PURPOSE_PASSWORD);
            return self.credentials.deferred_read_inflight(&legacy);
        }
        false
    }

    fn profile_credentials_loading(&self, profile: &Profile) -> bool {
        (profile.secret_saved && self.slot_read_loading(profile, PURPOSE_PASSWORD))
            || (profile.nickserv_saved && self.slot_read_loading(profile, PURPOSE_NICKSERV))
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
            self.applied_sessions
                .insert(profile.network_id.clone(), self.applied_session_for(profile));
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
        config.password = self.secret_text(profile, PURPOSE_PASSWORD);
        config.nickserv_password = self.secret_text(profile, PURPOSE_NICKSERV);
        config.sasl_account = profile.sasl_account();
        config.autojoin_channels = profile.autojoin_channels.clone();
        config.autojoin_keys = profile.autojoin_keys.clone();
        config
    }

    /// A profile's stored secret, or an empty one when the store has none or cannot answer. A
    /// session starts without the password in that case, and the Connect sheet says why.
    fn secret_text(&self, profile: &Profile, purpose: &'static str) -> Secret {
        read_slot_sync(self.credentials.as_ref(), profile, purpose)
            .ok()
            .flatten()
            .map(Secret::from)
            .unwrap_or_default()
    }
}

fn profile_for_credential_key(key: &CredentialKey) -> Profile {
    Profile {
        network_id: key.network_id.clone(),
        name: String::new(),
        host: key.host.clone(),
        port: DEFAULT_PORT,
        tls: true,
        connect_on_startup: false,
        secret_saved: false,
        nickserv_saved: false,
        nick: key.username.clone(),
        username: key.username.clone(),
        realname: String::new(),
        account: String::new(),
        bouncer_network: String::new(),
        autojoin_channels: Vec::new(),
        autojoin_keys: BTreeMap::new(),
        icon_color: None,
        avatar_url: String::new(),
    }
}

/// Whether a slot's saved flag says its secret is in the keychain. Only a secret the keychain
/// holds is saved, and a session-only one is not. A refused read or write says nothing about the
/// secret, so the flag keeps what the profile had (`IrcConnection::handleCredentialWrite`).
fn saved_flag(state: &CredentialState, was_saved: bool) -> bool {
    match state {
        CredentialState::Available => true,
        CredentialState::SessionOnly | CredentialState::Missing => false,
        CredentialState::Loading | CredentialState::Error(_) => was_saved,
    }
}

/// Whether a slot's secret has a settled place: held in the store or in memory, or known to be
/// absent. A refused write or delete is not settled, so no old key is deleted behind it.
fn is_settled(state: &CredentialState) -> bool {
    matches!(
        state,
        CredentialState::Available | CredentialState::SessionOnly | CredentialState::Missing
    )
}
