//! The Secret Service (`org.freedesktop.secrets`) on Linux, through `oo7`. Port of
//! `tui/internal/storage/secretservice.go`: items in the default collection, carrying the
//! attributes `service=omairc` and `username=<key name>` that QtKeychain writes. Each call is
//! synchronous: `futures::executor::block_on` drives the oo7 future on a worker thread, and the
//! caller waits at most a bounded time for it.
//!
//! Two bounds apply. The startup probe only connects to the bus, so it waits `PROBE_TIMEOUT`,
//! and a service that does not answer in that time falls back to the in-memory store for the
//! run. Every read, write and delete unlocks the keyring, which can wait on the user at the
//! keyring's prompt, so it waits `CALL_TIMEOUT`. An overrun is an `Error`, never a missing
//! secret.

use std::collections::HashMap;
use std::future::Future;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use futures::executor::block_on;
use oo7::{Keyring, Secret};

use super::{CredentialKey, CredentialState, CredentialStore, PendingRemovals};

/// The attribute names and the service value QtKeychain and the TUI search by.
const ATTRIBUTE_SERVICE: &str = "service";
const ATTRIBUTE_USERNAME: &str = "username";
const SERVICE: &str = "omairc";

/// How long one call that unlocks the keyring may wait, the unlock prompt included. A call
/// that overruns is abandoned on its worker thread, so a hung service cannot block the app
/// forever. Thirty seconds is long enough for a person to answer the prompt.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the startup probe waits for the bus to answer. The probe never unlocks, so a
/// service that is not answering quickly is treated as absent for this run.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// The Secret Service store. Each call connects to the bus; the struct keeps only the deletes
/// it owes.
pub struct SecretServiceStore {
    pending: PendingRemovals,
}

/// The store, when a Secret Service answers on the session bus within `PROBE_TIMEOUT`. Without
/// one the caller falls back to memory.
pub fn open() -> Option<Box<dyn CredentialStore>> {
    let answered = bounded(PROBE_TIMEOUT, async { Keyring::new().await.is_ok() }) == Some(true);
    answered.then(|| {
        Box::new(SecretServiceStore {
            pending: PendingRemovals::default(),
        }) as Box<dyn CredentialStore>
    })
}

/// Runs `call` to completion on a worker thread and waits at most `timeout` for it. `None` when
/// the call overran; the worker is left to finish on its own, and a late result is dropped.
fn bounded<T: Send + 'static>(
    timeout: Duration,
    call: impl Future<Output = T> + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        // The receiver is gone when the caller gave up, so a late result is dropped.
        let _ = sender.send(block_on(call));
    });
    receiver.recv_timeout(timeout).ok()
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

/// The first non-empty secret among the entries for `key_name`. `Ok(None)` when there is none;
/// an error the service reports is `Err`, and is never read as a missing secret.
async fn read_password(key_name: String) -> oo7::Result<Option<String>> {
    let keyring = unlocked_keyring().await?;
    for item in keyring.search_items(&attributes(&key_name)).await? {
        item.unlock().await?;
        let secret = item.secret().await?;
        let text = String::from_utf8_lossy(secret.as_bytes()).into_owned();
        if !text.is_empty() {
            return Ok(Some(text));
        }
    }
    Ok(None)
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

/// The reason a call that did not answer in time reports.
fn timed_out_reason() -> String {
    "timed out waiting for the secret service".to_owned()
}

impl CredentialStore for SecretServiceStore {
    fn durable(&self) -> bool {
        true
    }

    fn read(&self, key: &CredentialKey) -> Result<Option<String>, String> {
        match bounded(CALL_TIMEOUT, read_password(key.key_name())) {
            Some(answer) => answer.map_err(|error| error.to_string()),
            None => Err(timed_out_reason()),
        }
    }

    fn write(&mut self, key: &CredentialKey, password: &str) -> CredentialState {
        match bounded(
            CALL_TIMEOUT,
            store_password(key.key_name(), password.to_owned()),
        ) {
            Some(Ok(())) => CredentialState::Available,
            Some(Err(error)) => CredentialState::Error(error.to_string()),
            None => CredentialState::Error(timed_out_reason()),
        }
    }

    fn remove(&mut self, key: &CredentialKey) -> CredentialState {
        match bounded(CALL_TIMEOUT, delete_password(key.key_name())) {
            Some(Ok(())) => CredentialState::Missing,
            Some(Err(error)) => CredentialState::Error(error.to_string()),
            None => CredentialState::Error(timed_out_reason()),
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
    use crate::storage::credentialstore::PURPOSE_PASSWORD;

    #[test]
    fn a_call_that_overruns_its_timeout_yields_no_answer() {
        // A call that never answers is a timeout, not a result: `read` maps it to `Err`, and
        // a write or delete maps it to `Error`, so it never reads as a missing secret.
        assert_eq!(
            bounded(Duration::from_millis(20), std::future::pending::<u8>()),
            None
        );
        assert_eq!(bounded(Duration::from_secs(5), async { 7u8 }), Some(7));
        assert!(timed_out_reason().starts_with("timed out"));
    }

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
