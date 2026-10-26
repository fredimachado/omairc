//! The macOS login keychain, through `security-framework`'s generic passwords.
//! `tui/internal/storage/secretservice_darwin.go`, which shells out to the `security` tool: the
//! service is `omairc` and the account is the key name, the entry QtKeychain writes.

use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

use super::{CredentialKey, CredentialState, CredentialStore, PendingRemovals};

/// QtKeychain's service name.
const SERVICE: &str = "omairc";
/// `errSecItemNotFound`: the keychain holds no entry for the account.
const ITEM_NOT_FOUND: i32 = -25300;
/// An account no client writes, for the availability probe.
const PROBE_ACCOUNT: &str = "omairc/probe";

/// The login keychain store. The keychain holds the entries; the struct keeps only the deletes
/// it owes.
pub struct KeychainCredentialStore {
    pending: PendingRemovals,
}

/// The store, when the keychain answers. A read of an absent probe entry is an answer; any
/// other failure means the keychain is not usable.
pub fn open() -> Option<Box<dyn CredentialStore>> {
    let answered = match get_generic_password(SERVICE, PROBE_ACCOUNT) {
        Ok(_) => true,
        Err(error) => error.code() == ITEM_NOT_FOUND,
    };
    answered.then(|| {
        Box::new(KeychainCredentialStore {
            pending: PendingRemovals::default(),
        }) as Box<dyn CredentialStore>
    })
}

impl CredentialStore for KeychainCredentialStore {
    fn durable(&self) -> bool {
        true
    }

    fn read(&self, key: &CredentialKey) -> Result<Option<String>, String> {
        match get_generic_password(SERVICE, &key.key_name()) {
            Ok(password) => {
                let text = String::from_utf8_lossy(&password).into_owned();
                Ok((!text.is_empty()).then_some(text))
            }
            Err(error) if error.code() == ITEM_NOT_FOUND => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn write(&mut self, key: &CredentialKey, password: &str) -> CredentialState {
        // The framework adds the entry, or updates it when one already exists.
        match set_generic_password(SERVICE, &key.key_name(), password.as_bytes()) {
            Ok(()) => CredentialState::Available,
            Err(error) => CredentialState::Error(error.to_string()),
        }
    }

    fn remove(&mut self, key: &CredentialKey) -> CredentialState {
        match delete_generic_password(SERVICE, &key.key_name()) {
            Ok(()) => CredentialState::Missing,
            Err(error) if error.code() == ITEM_NOT_FOUND => CredentialState::Missing,
            Err(error) => CredentialState::Error(error.to_string()),
        }
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
    use crate::storage::credentialstore::PURPOSE_NICKSERV;

    /// Writes, reads and removes a real login keychain entry. Run with
    /// `cargo test -- --ignored` on macOS; it may ask once for keychain access.
    #[test]
    #[ignore = "writes to the login keychain"]
    fn the_login_keychain_round_trips_a_secret() {
        let key = CredentialKey {
            network_id: format!("omairc-test-{}", std::process::id()),
            username: "test".to_owned(),
            host: "irc.example".to_owned(),
            purpose: PURPOSE_NICKSERV,
        };
        let mut store = open().expect("the login keychain answers");
        assert_eq!(store.read(&key), Ok(None));
        assert_eq!(store.write(&key, "hunter2"), CredentialState::Available);
        assert_eq!(store.read(&key), Ok(Some("hunter2".to_owned())));
        assert_eq!(
            store.write(&key, "swordfish"),
            CredentialState::Available,
            "an update"
        );
        assert_eq!(store.read(&key), Ok(Some("swordfish".to_owned())));
        assert_eq!(store.remove(&key), CredentialState::Missing);
        assert_eq!(store.read(&key), Ok(None));
        assert_eq!(
            store.remove(&key),
            CredentialState::Missing,
            "removing nothing is fine"
        );
    }
}
