//! The deletes a store still owes: keys that hold a network's secret and that a rename or a
//! removal could not delete yet (Qt's `IrcDraftSecret::obsoleteKeys`). Each store keeps its
//! own set, so the controller reaches them through [`CredentialStore::pending`] and the
//! set outlives one save.
//! [`CredentialStore::pending`]: super::CredentialStore::pending

use std::collections::BTreeSet;

use super::{purpose_matches_slot, CredentialKey};

/// The keys whose delete is still owed. A key is here from the moment it stops being the
/// network's current key, until a delete of it succeeds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingRemovals {
    keys: BTreeSet<CredentialKey>,
}

impl PendingRemovals {
    /// Owes a delete of `key`. Owing it twice is the same debt.
    pub fn add(&mut self, key: CredentialKey) {
        self.keys.insert(key);
    }

    /// The delete of `key` is done, or the key no longer needs one.
    pub fn forget(&mut self, key: &CredentialKey) {
        self.keys.remove(key);
    }

    /// The owed keys of one secret slot of a network, in key order.
    pub fn slot(&self, network_id: &str, purpose: &str) -> Vec<CredentialKey> {
        self.keys
            .iter()
            .filter(|key| key.network_id == network_id && purpose_matches_slot(purpose, key))
            .cloned()
            .collect()
    }

    /// The owed keys of every secret slot of a network, in key order.
    pub fn network(&self, network_id: &str) -> Vec<CredentialKey> {
        self.keys
            .iter()
            .filter(|key| key.network_id == network_id)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::credentialstore::{PURPOSE_NICKSERV, PURPOSE_PASSWORD};

    fn key(network: &str, host: &str, purpose: &'static str) -> CredentialKey {
        CredentialKey {
            network_id: network.to_owned(),
            username: "fred".to_owned(),
            host: host.to_owned(),
            purpose,
        }
    }

    #[test]
    fn a_key_is_owed_once_until_it_is_forgotten() {
        let mut owed = PendingRemovals::default();
        let old = key("libera", "old.example", PURPOSE_PASSWORD);
        owed.add(old.clone());
        owed.add(old.clone());
        assert_eq!(owed.network("libera"), [old.clone()]);
        owed.forget(&old);
        assert!(owed.network("libera").is_empty());
    }

    #[test]
    fn slots_and_networks_filter_what_they_name() {
        let mut owed = PendingRemovals::default();
        owed.add(key("libera", "a.example", PURPOSE_PASSWORD));
        owed.add(key("libera", "a.example", PURPOSE_NICKSERV));
        owed.add(key("oftc", "a.example", PURPOSE_PASSWORD));
        assert_eq!(owed.slot("libera", PURPOSE_PASSWORD).len(), 1);
        assert_eq!(owed.slot("libera", PURPOSE_NICKSERV).len(), 1);
        assert!(owed.slot("oftc", PURPOSE_NICKSERV).is_empty());
        assert_eq!(owed.network("libera").len(), 2);
        assert_eq!(owed.network("oftc").len(), 1);
    }
}
