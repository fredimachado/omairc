//! Port of `tui/internal/storage/credentialstore.go` (Go) and
//! `src/storage/secretservicecredentialstore.cpp` (C++), behind a trait.
//!
//! A network's server and NickServ passwords live in the OS keychain, never in the
//! settings file: Windows Credential Manager, the macOS login keychain, or the Secret
//! Service on Linux. Each backend keeps the entry names Qt and the TUI write
//! ([`CredentialKey::key_name`]), so a secret saved by one client is read by the others.
//! When no backend answers, [`platform_store`] falls back to [`MemoryCredentialStore`],
//! which keeps secrets for this run only and reports `SessionOnly`.

use std::collections::BTreeMap;

use crate::storage::profilestore::Profile;

pub(crate) mod defer;
// Re-exported for controller tests; production backends import `defer` directly.
#[cfg(test)]
pub(crate) use defer::DeferringCredentialStore;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod pending;
#[cfg(any(test, target_os = "linux"))]
mod secretattrs;
#[cfg(windows)]
mod windows;

pub use pending::PendingRemovals;

/// The outcome of a slot read on the UI thread. `Loading` means a worker still has the
/// keyring call in flight; the caller must poll and try again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadSlot {
    Loading,
    Ready(Result<Option<String>, String>),
}

/// Where a secret stands after a write or a delete (`CredentialStore::State`), as the Connect
/// sheet words it. `Error` carries the OS's reason; it never holds the password.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CredentialState {
    /// No secret is stored for the key.
    #[default]
    Missing,
    /// The secret is kept only in memory, for this run, because no secure storage
    /// backend exists.
    SessionOnly,
    /// The secret is in the OS keychain.
    Available,
    /// A deferred keychain read is still in flight on another thread.
    Loading,
    /// The keychain refused the operation, with the reason it gave.
    Error(String),
}

/// The server-password slot of a network. Qt and the TUI omit the purpose segment
/// for this slot; an empty purpose is the canonical key name.
pub const PURPOSE_PASSWORD: &str = "";
/// An earlier Rust build once wrote server passwords under this suffix. Reads and deletes still
/// reach those entries until a successful write migrates them.
pub const LEGACY_PURPOSE_PASSWORD: &str = "password";
/// The NickServ slot of a network.
pub const PURPOSE_NICKSERV: &str = "nickserv";

/// Whether `key` belongs to the secret slot named by `purpose`.
pub fn purpose_matches_slot(purpose: &str, key: &CredentialKey) -> bool {
    if purpose == PURPOSE_PASSWORD {
        key.purpose == PURPOSE_PASSWORD || key.purpose == LEGACY_PURPOSE_PASSWORD
    } else {
        key.purpose == purpose
    }
}

/// Every key name a slot may be stored under, canonical first.
pub fn key_names_for_slot(profile: &Profile, purpose: &'static str) -> Vec<String> {
    let primary = CredentialKey::for_profile(profile, purpose);
    let mut names = vec![primary.key_name()];
    if purpose == PURPOSE_PASSWORD {
        let legacy = CredentialKey::for_profile(profile, LEGACY_PURPOSE_PASSWORD);
        if legacy.key_name() != names[0] {
            names.push(legacy.key_name());
        }
    }
    names
}

/// Reads one slot without blocking on the keyring. When the answer is [`ReadSlot::Loading`],
/// call [`CredentialStore::poll_reads`] and try again.
pub fn read_slot(
    store: &dyn CredentialStore,
    profile: &Profile,
    purpose: &'static str,
) -> ReadSlot {
    let primary = CredentialKey::for_profile(profile, purpose);
    match store.read_deferred(&primary) {
        ReadSlot::Loading => ReadSlot::Loading,
        ReadSlot::Ready(Ok(Some(text))) => ReadSlot::Ready(Ok(Some(text))),
        ReadSlot::Ready(Ok(None)) if purpose == PURPOSE_PASSWORD => {
            let legacy = CredentialKey::for_profile(profile, LEGACY_PURPOSE_PASSWORD);
            store.read_deferred(&legacy)
        }
        ReadSlot::Ready(answer) => ReadSlot::Ready(answer),
    }
}

/// Reads one slot synchronously. Only for tests and for stores that answer immediately.
pub fn read_slot_sync(
    store: &dyn CredentialStore,
    profile: &Profile,
    purpose: &'static str,
) -> Result<Option<String>, String> {
    match read_slot(store, profile, purpose) {
        ReadSlot::Ready(answer) => answer,
        ReadSlot::Loading => Err("checking secure storage".to_owned()),
    }
}

/// Writes one slot under the canonical key and drops a legacy server-password entry when
/// the write settles.
pub fn write_slot(
    store: &mut dyn CredentialStore,
    profile: &Profile,
    purpose: &'static str,
    password: &str,
) -> CredentialState {
    let key = CredentialKey::for_profile(profile, purpose);
    let state = store.write(&key, password);
    if purpose == PURPOSE_PASSWORD && slot_write_settled(&state) {
        let legacy = CredentialKey::for_profile(profile, LEGACY_PURPOSE_PASSWORD);
        let _ = store.remove(&legacy);
    }
    state
}

/// Deletes every key a slot may occupy.
pub fn remove_slot(
    store: &mut dyn CredentialStore,
    profile: &Profile,
    purpose: &'static str,
) -> CredentialState {
    let key = CredentialKey::for_profile(profile, purpose);
    let mut state = store.remove(&key);
    if purpose == PURPOSE_PASSWORD {
        let legacy = CredentialKey::for_profile(profile, LEGACY_PURPOSE_PASSWORD);
        match store.remove(&legacy) {
            CredentialState::Error(message) => state = CredentialState::Error(message),
            _ => {}
        }
    }
    state
}

fn slot_write_settled(state: &CredentialState) -> bool {
    matches!(
        state,
        CredentialState::Available | CredentialState::SessionOnly | CredentialState::Missing
    )
}

/// One secret: the network it belongs to, the account it is for (username and host), and
/// which slot of it. The same fields Go's `CredentialKey` and Qt's `CredentialKey` carry,
/// so the entry name matches theirs.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CredentialKey {
    pub network_id: String,
    pub username: String,
    pub host: String,
    pub purpose: &'static str,
}

impl CredentialKey {
    /// The key of one secret slot of `profile`. The username is the profile's, or its nick
    /// when that is empty (Go's `credentialKeyFor`, Qt's `IrcConnection::credentialKey`).
    pub fn for_profile(profile: &Profile, purpose: &'static str) -> Self {
        let username = if profile.username.is_empty() {
            &profile.nick
        } else {
            &profile.username
        };
        Self {
            network_id: profile.network_id.clone(),
            username: username.clone(),
            host: profile.host.clone(),
            purpose,
        }
    }

    /// The entry name: `omairc/v1/<len>:<networkId>/<len>:<username>/<len>:<host>`, and
    /// `/<len>:<purpose>` when the purpose is set. Each length is the value's UTF-8 byte
    /// count, so a slash inside a value cannot make two keys collide (Go's
    /// `CredentialKeyName`, Qt's `SecretServiceCredentialStore::keyName`).
    pub fn key_name(&self) -> String {
        let mut name = format!(
            "omairc/v1/{}/{}/{}",
            length_prefixed(&self.network_id),
            length_prefixed(&self.username),
            length_prefixed(&self.host),
        );
        if !self.purpose.is_empty() {
            name.push('/');
            name.push_str(&length_prefixed(self.purpose));
        }
        name
    }
}

fn length_prefixed(value: &str) -> String {
    format!("{}:{value}", value.len())
}

/// Reads, writes and removes network secrets. The controller owns one, and the Connect
/// sheet never touches it directly.
pub trait CredentialStore {
    /// Whether a secret written here outlives the process. A store that is not durable
    /// must not be reported as "saved".
    fn durable(&self) -> bool;

    /// The stored secret: `Ok(None)` when there is none, and `Err` with the backend's reason
    /// when it could not answer, such as a call that timed out. A caller must not read an
    /// `Err` as a missing secret.
    fn read(&self, key: &CredentialKey) -> Result<Option<String>, String>;

    /// Like [`read`], but never waits on a slow keyring. The default answers immediately.
    fn read_deferred(&self, key: &CredentialKey) -> ReadSlot {
        ReadSlot::Ready(self.read(key))
    }

    /// Settles in-flight [`read_deferred`] calls. The UI loop calls this on each tick.
    fn poll_reads(&self) {}

    /// Drops a settled read error for `key` and starts one new deferred read. Observing an
    /// error must not retry; only this method, [`write`], or [`remove`] may. The default is a
    /// no-op so stores that answer immediately still compile.
    fn retry_read(&self, _key: &CredentialKey) {}

    /// Whether a deferred read is in flight for `key`, without starting a new one.
    fn deferred_read_inflight(&self, _key: &CredentialKey) -> bool {
        false
    }

    /// Stores a secret and reports where it now lives.
    fn write(&mut self, key: &CredentialKey, password: &str) -> CredentialState;

    /// Deletes a secret and reports the state after the delete. `Missing` is a delete that
    /// is done, whether or not there was anything to delete.
    fn remove(&mut self, key: &CredentialKey) -> CredentialState;

    /// The deletes this store still owes (Qt's obsolete keys). The controller records a key
    /// here when a rename leaves it behind, and forgets it once its delete succeeds.
    fn pending(&self) -> &PendingRemovals;

    /// The same set, for recording and forgetting keys.
    fn pending_mut(&mut self) -> &mut PendingRemovals;
}

/// The OS keychain for this platform, or [`MemoryCredentialStore`] when its backend does not
/// answer. Probes the backend once, so call it once per controller.
pub fn platform_store() -> Box<dyn CredentialStore> {
    platform_backend().unwrap_or_else(|| Box::new(MemoryCredentialStore::default()))
}

#[cfg(windows)]
fn platform_backend() -> Option<Box<dyn CredentialStore>> {
    windows::open()
}

#[cfg(target_os = "macos")]
fn platform_backend() -> Option<Box<dyn CredentialStore>> {
    macos::open()
}

#[cfg(target_os = "linux")]
fn platform_backend() -> Option<Box<dyn CredentialStore>> {
    linux::open()
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn platform_backend() -> Option<Box<dyn CredentialStore>> {
    None
}

/// The in-memory store: secrets live in a map for this run only. It never refuses a delete,
/// so it owes none, unless a test wraps it in a store that does.
#[derive(Debug, Default)]
pub struct MemoryCredentialStore {
    secrets: BTreeMap<CredentialKey, String>,
    pending: PendingRemovals,
}

impl CredentialStore for MemoryCredentialStore {
    fn durable(&self) -> bool {
        false
    }

    fn read(&self, key: &CredentialKey) -> Result<Option<String>, String> {
        Ok(self.secrets.get(key).cloned())
    }

    fn write(&mut self, key: &CredentialKey, password: &str) -> CredentialState {
        self.secrets.insert(key.clone(), password.to_owned());
        CredentialState::SessionOnly
    }

    fn remove(&mut self, key: &CredentialKey) -> CredentialState {
        self.secrets.remove(key);
        CredentialState::Missing
    }

    fn pending(&self) -> &PendingRemovals {
        &self.pending
    }

    fn pending_mut(&mut self) -> &mut PendingRemovals {
        &mut self.pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(network: &str, purpose: &'static str) -> CredentialKey {
        CredentialKey {
            network_id: network.to_owned(),
            username: "alice".to_owned(),
            host: "irc.example".to_owned(),
            purpose,
        }
    }

    fn named(network: &str, username: &str, host: &str, purpose: &'static str) -> CredentialKey {
        CredentialKey {
            network_id: network.to_owned(),
            username: username.to_owned(),
            host: host.to_owned(),
            purpose,
        }
    }

    #[test]
    fn key_names_match_the_go_vectors() {
        // tui/internal/storage/credentialstore_test.go, TestCredentialKeyName.
        let cases = [
            (
                named("freenode", "alice", "chat.freenode.net", ""),
                "omairc/v1/8:freenode/5:alice/17:chat.freenode.net",
            ),
            (
                named("freenode", "alice", "chat.freenode.net", "sasl"),
                "omairc/v1/8:freenode/5:alice/17:chat.freenode.net/4:sasl",
            ),
            (named("", "", "", ""), "omairc/v1/0:/0:/0:"),
            (named("é", "ü", "host", ""), "omairc/v1/2:é/2:ü/4:host"),
            (
                named("net", "u", "h", "a/b"),
                "omairc/v1/3:net/1:u/1:h/3:a/b",
            ),
        ];
        for (key, want) in cases {
            assert_eq!(key.key_name(), want, "{key:?}");
        }
    }

    #[test]
    fn key_names_match_the_qt_vectors_for_each_slot() {
        // tests/storage/tst_secretservice.cpp: the same entry names, for both slots.
        let password = named("nid", "user", "irc.example", PURPOSE_PASSWORD);
        assert_eq!(
            password.key_name(),
            "omairc/v1/3:nid/4:user/11:irc.example"
        );
        let legacy_password = named("nid", "user", "irc.example", LEGACY_PURPOSE_PASSWORD);
        assert_eq!(
            legacy_password.key_name(),
            "omairc/v1/3:nid/4:user/11:irc.example/8:password"
        );
        let nickserv = named("nid", "user", "irc.example", PURPOSE_NICKSERV);
        assert_eq!(
            nickserv.key_name(),
            "omairc/v1/3:nid/4:user/11:irc.example/8:nickserv"
        );
        let split_host = named("id", "a", "b/c", "");
        let split_user = named("id", "a/b", "c", "");
        assert_eq!(split_host.key_name(), "omairc/v1/2:id/1:a/3:b/c");
        assert_eq!(split_user.key_name(), "omairc/v1/2:id/3:a/b/1:c");
        assert_ne!(split_host.key_name(), split_user.key_name());
    }

    #[test]
    fn a_profile_key_uses_the_nick_when_no_username_is_set() {
        let mut profile = Profile {
            host: "irc.example".to_owned(),
            nick: "fred".to_owned(),
            ..Profile::new("nid")
        };
        assert_eq!(
            CredentialKey::for_profile(&profile, PURPOSE_PASSWORD).username,
            "fred"
        );
        profile.username = "sasl-fred".to_owned();
        assert_eq!(
            CredentialKey::for_profile(&profile, PURPOSE_NICKSERV).key_name(),
            "omairc/v1/3:nid/9:sasl-fred/11:irc.example/8:nickserv"
        );
    }

    #[test]
    fn the_memory_store_keeps_secrets_for_the_run_and_never_claims_durability() {
        let mut store = MemoryCredentialStore::default();
        assert!(!store.durable());
        assert_eq!(store.read(&key("a", PURPOSE_PASSWORD)), Ok(None));
        assert_eq!(
            store.write(&key("a", PURPOSE_PASSWORD), "hunter2"),
            CredentialState::SessionOnly
        );
        assert_eq!(
            store.read(&key("a", PURPOSE_PASSWORD)),
            Ok(Some("hunter2".to_owned()))
        );
        assert_eq!(
            store.read(&key("a", PURPOSE_NICKSERV)),
            Ok(None),
            "slots are separate"
        );
    }

    #[test]
    fn remove_forgets_one_slot() {
        let mut store = MemoryCredentialStore::default();
        store.write(&key("a", PURPOSE_PASSWORD), "one");
        store.write(&key("a", PURPOSE_NICKSERV), "two");
        assert_eq!(
            store.remove(&key("a", PURPOSE_PASSWORD)),
            CredentialState::Missing
        );
        assert_eq!(store.read(&key("a", PURPOSE_PASSWORD)), Ok(None));
        assert_eq!(
            store.read(&key("a", PURPOSE_NICKSERV)),
            Ok(Some("two".to_owned()))
        );
    }

    #[test]
    fn a_legacy_server_password_is_read_and_migrated_on_write() {
        let mut store = MemoryCredentialStore::default();
        let profile = Profile {
            host: "irc.example".to_owned(),
            nick: "fred".to_owned(),
            ..Profile::new("nid")
        };
        let legacy = CredentialKey::for_profile(&profile, LEGACY_PURPOSE_PASSWORD);
        store.write(&legacy, "hunter2");
        assert_eq!(
            read_slot_sync(&store, &profile, PURPOSE_PASSWORD),
            Ok(Some("hunter2".to_owned()))
        );
        assert_eq!(
            write_slot(&mut store, &profile, PURPOSE_PASSWORD, "hunter2"),
            CredentialState::SessionOnly
        );
        assert_eq!(store.read(&legacy), Ok(None));
        assert_eq!(
            store.read(&CredentialKey::for_profile(&profile, PURPOSE_PASSWORD)),
            Ok(Some("hunter2".to_owned()))
        );
    }

    #[test]
    fn the_memory_store_owes_no_delete_until_a_caller_records_one() {
        let mut store = MemoryCredentialStore::default();
        assert!(store.pending().network("a").is_empty());
        store.pending_mut().add(key("a", PURPOSE_PASSWORD));
        assert_eq!(store.pending().network("a"), [key("a", PURPOSE_PASSWORD)]);
    }
}
