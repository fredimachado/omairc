//! The Secret Service (`org.freedesktop.secrets`) on Linux, through `oo7`. Port of
//! `tui/internal/storage/secretservice.go`: items in the default collection, carrying the
//! attributes `service=omairc` and `username=<key name>` that QtKeychain writes. Each call is
//! synchronous: `futures::executor::block_on` drives the oo7 future on a worker thread, and the
//! caller waits at most `CALL_TIMEOUT`, as the Go client bounds its D-Bus calls.

use std::collections::HashMap;
use std::future::Future;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use futures::executor::block_on;
use oo7::{Keyring, Secret};

use super::{CredentialKey, CredentialState, CredentialStore};

/// The attribute names and the service value QtKeychain and the TUI search by.
const ATTRIBUTE_SERVICE: &str = "service";
const ATTRIBUTE_USERNAME: &str = "username";
const SERVICE: &str = "omairc";

/// How long one Secret Service call may take, as Go's `secretServiceCallTimeout`. A call that
/// overruns is abandoned on its worker thread, so a hung service cannot block the app.
const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// The Secret Service store. Each call connects to the bus, so the struct has no state.
pub struct SecretServiceStore;

/// The store, when a Secret Service answers on the session bus within `CALL_TIMEOUT`. Without
/// one the caller falls back to memory.
pub fn open() -> Option<Box<dyn CredentialStore>> {
    let answered = bounded(async { Keyring::new().await.is_ok() }) == Some(true);
    answered.then(|| Box::new(SecretServiceStore) as Box<dyn CredentialStore>)
}

/// Runs `call` to completion on a worker thread and waits at most `CALL_TIMEOUT` for it.
/// `None` when the call overran.
fn bounded<T: Send + 'static>(call: impl Future<Output = T> + Send + 'static) -> Option<T> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        // The receiver is gone when the caller gave up, so a late result is dropped.
        let _ = sender.send(block_on(call));
    });
    receiver.recv_timeout(CALL_TIMEOUT).ok()
}

/// The attributes that find one entry: the service, and the key name as the username.
fn attributes(key_name: &str) -> HashMap<&str, &str> {
    HashMap::from([(ATTRIBUTE_SERVICE, SERVICE), (ATTRIBUTE_USERNAME, key_name)])
}

/// The default collection, unlocked. Every operation starts here.
async fn unlocked_keyring() -> oo7::Result<Keyring> {
    let keyring = Keyring::new().await?;
    keyring.unlock().await?;
    Ok(keyring)
}

/// The first non-empty secret among the entries for `key_name`.
async fn read_password(key_name: String) -> Option<String> {
    let keyring = unlocked_keyring().await.ok()?;
    let items = keyring.search_items(&attributes(&key_name)).await.ok()?;
    for item in items {
        item.unlock().await.ok()?;
        let secret = item.secret().await.ok()?;
        let text = String::from_utf8_lossy(secret.as_bytes()).into_owned();
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

/// Stores `password` under `key_name`, replacing the entry that is there.
async fn store_password(key_name: String, password: String) -> oo7::Result<()> {
    let keyring = unlocked_keyring().await?;
    keyring
        .create_item(
            &key_name,
            &attributes(&key_name),
            Secret::text(password),
            true,
        )
        .await
}

/// Deletes every entry for `key_name`. Deleting nothing succeeds.
async fn delete_password(key_name: String) -> oo7::Result<()> {
    let keyring = unlocked_keyring().await?;
    keyring.delete(&attributes(&key_name)).await
}

/// The state a call that did not answer in time reports.
fn timed_out() -> CredentialState {
    CredentialState::Error("secret service did not answer in time".to_owned())
}

impl CredentialStore for SecretServiceStore {
    fn durable(&self) -> bool {
        true
    }

    fn read(&self, key: &CredentialKey) -> Option<String> {
        bounded(read_password(key.key_name())).flatten()
    }

    fn write(&mut self, key: &CredentialKey, password: &str) -> CredentialState {
        match bounded(store_password(key.key_name(), password.to_owned())) {
            Some(Ok(())) => CredentialState::Available,
            Some(Err(error)) => CredentialState::Error(error.to_string()),
            None => timed_out(),
        }
    }

    fn remove(&mut self, key: &CredentialKey) -> CredentialState {
        match bounded(delete_password(key.key_name())) {
            Some(Ok(())) => CredentialState::Missing,
            Some(Err(error)) => CredentialState::Error(error.to_string()),
            None => timed_out(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::credentialstore::PURPOSE_PASSWORD;

    /// Writes, reads and removes a real Secret Service entry. Run with `cargo test -- --ignored`
    /// on a session with a running keyring (GNOME Keyring or KWallet).
    #[test]
    #[ignore = "needs a Secret Service on the session bus"]
    fn the_secret_service_round_trips_a_secret() {
        let key = CredentialKey {
            network_id: format!("omairc-test-{}", std::process::id()),
            username: "test".to_owned(),
            host: "irc.example".to_owned(),
            purpose: PURPOSE_PASSWORD,
        };
        let mut store = open().expect("a Secret Service answers on the session bus");
        assert_eq!(store.read(&key), None);
        assert_eq!(store.write(&key, "hunter2"), CredentialState::Available);
        assert_eq!(store.read(&key).as_deref(), Some("hunter2"));
        assert_eq!(
            store.write(&key, "swordfish"),
            CredentialState::Available,
            "an update"
        );
        assert_eq!(store.read(&key).as_deref(), Some("swordfish"));
        assert_eq!(store.remove(&key), CredentialState::Missing);
        assert_eq!(store.read(&key), None);
        assert_eq!(
            store.remove(&key),
            CredentialState::Missing,
            "removing nothing is fine"
        );
    }
}
