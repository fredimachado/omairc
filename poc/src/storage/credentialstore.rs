//! Port of `tui/internal/storage/credentialstore.go` (Go) and
//! `src/storage/secretservicecredentialstore.cpp` (C++), behind a trait.
//!
//! A network's server and NickServ passwords live here, not in the profile file. The
//! OS keychain is Phase 11. Until then the only store is [`MemoryCredentialStore`],
//! which keeps secrets for the life of the process and says so: its writes report
//! `SessionOnly`, the state the Connect sheet words as "secure storage unavailable".

use std::collections::BTreeMap;

/// Where a secret stands after a write or a delete (`CredentialStore::State`), as the
/// Connect sheet words it. The durable state (`Available`) and the backend failures
/// arrive with the keychain store in Phase 11.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CredentialState {
    /// No secret is stored for the key.
    #[default]
    Missing,
    /// The secret is kept only in memory, for this run, because no secure storage
    /// backend exists yet.
    SessionOnly,
}

/// The slot a secret belongs to, under one network (`CredentialKey::purpose`).
pub const PURPOSE_PASSWORD: &str = "password";
/// The NickServ slot of a network.
pub const PURPOSE_NICKSERV: &str = "nickserv";

/// One secret: the network it belongs to, and which slot of it (`CredentialKey`). The
/// keychain attributes Go adds (username, host) arrive with Phase 11. Until then a
/// network's secrets follow it through a rename, since the id never changes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CredentialKey {
    pub network_id: String,
    pub purpose: &'static str,
}

impl CredentialKey {
    pub fn new(network_id: &str, purpose: &'static str) -> Self {
        Self {
            network_id: network_id.to_owned(),
            purpose,
        }
    }
}

/// Reads, writes and removes network secrets. The controller owns one, and the Connect
/// sheet never touches it directly.
pub trait CredentialStore {
    /// Whether a secret written here outlives the process. A store that is not durable
    /// must not be reported as "saved".
    fn durable(&self) -> bool;

    /// The stored secret, or `None` when there is none.
    fn read(&self, key: &CredentialKey) -> Option<String>;

    /// Stores a secret and reports where it now lives.
    fn write(&mut self, key: &CredentialKey, password: &str) -> CredentialState;

    /// Deletes a secret and reports the state after the delete.
    fn remove(&mut self, key: &CredentialKey) -> CredentialState;
}

/// The in-memory store: secrets live in a map for this run only.
#[derive(Debug, Default)]
pub struct MemoryCredentialStore {
    secrets: BTreeMap<CredentialKey, String>,
}

impl CredentialStore for MemoryCredentialStore {
    fn durable(&self) -> bool {
        false
    }

    fn read(&self, key: &CredentialKey) -> Option<String> {
        self.secrets.get(key).cloned()
    }

    fn write(&mut self, key: &CredentialKey, password: &str) -> CredentialState {
        self.secrets.insert(key.clone(), password.to_owned());
        CredentialState::SessionOnly
    }

    fn remove(&mut self, key: &CredentialKey) -> CredentialState {
        self.secrets.remove(key);
        CredentialState::Missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(network: &str, purpose: &'static str) -> CredentialKey {
        CredentialKey::new(network, purpose)
    }

    #[test]
    fn the_memory_store_keeps_secrets_for_the_run_and_never_claims_durability() {
        let mut store = MemoryCredentialStore::default();
        assert!(!store.durable());
        assert_eq!(store.read(&key("a", PURPOSE_PASSWORD)), None);
        assert_eq!(
            store.write(&key("a", PURPOSE_PASSWORD), "hunter2"),
            CredentialState::SessionOnly
        );
        assert_eq!(store.read(&key("a", PURPOSE_PASSWORD)).as_deref(), Some("hunter2"));
        assert_eq!(store.read(&key("a", PURPOSE_NICKSERV)), None, "slots are separate");
    }

    #[test]
    fn remove_forgets_one_slot() {
        let mut store = MemoryCredentialStore::default();
        store.write(&key("a", PURPOSE_PASSWORD), "one");
        store.write(&key("a", PURPOSE_NICKSERV), "two");
        assert_eq!(store.remove(&key("a", PURPOSE_PASSWORD)), CredentialState::Missing);
        assert_eq!(store.read(&key("a", PURPOSE_PASSWORD)), None);
        assert_eq!(store.read(&key("a", PURPOSE_NICKSERV)).as_deref(), Some("two"));
    }
}
