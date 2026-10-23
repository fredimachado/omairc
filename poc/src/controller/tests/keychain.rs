//! The keychain half of an Apply that changes a saved network's host, username or nick. The
//! secret lives under a key built from those fields, so a change moves it: the old key is
//! deleted once the new one holds the secret, and a key the store refuses to delete stays owed
//! until a later save or the removal of the network. The Qt side is `IrcConnection::applySecret`
//! and `flushObsoleteKeys`; the Go client removes the old key inline, and has no owed set.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::{TimeZone, Utc};

use crate::controller::controller::Controller;
use crate::controller::persistence::SecretEdits;
use crate::controller::transports::NoTransports;
use crate::desktop::notifier::NullNotifier;
use crate::session::clock::{Clock, FakeClock};
use crate::storage::credentialstore::{
    CredentialKey, CredentialState, CredentialStore, MemoryCredentialStore, PendingRemovals,
    PURPOSE_NICKSERV, PURPOSE_PASSWORD,
};
use crate::storage::profilestore::Profile;
use crate::storage::Backing;

const LIBERA: &str = "libera";

/// What a test store was asked to do, shared with the test. The lists name the keys the store
/// refuses for that operation, as a keychain that is locked or out of reach does.
#[derive(Clone, Default)]
struct Ledger {
    writes: Rc<RefCell<Vec<CredentialKey>>>,
    refused_writes: Rc<RefCell<Vec<CredentialKey>>>,
    refused_deletes: Rc<RefCell<Vec<CredentialKey>>>,
    unreadable: Rc<RefCell<Vec<CredentialKey>>>,
    /// Whether the store keeps a write past the process (a real keychain), or only for the run.
    durable: Rc<Cell<bool>>,
}

/// A memory store that answers through a [`Ledger`].
#[derive(Default)]
struct ScriptedStore {
    memory: MemoryCredentialStore,
    ledger: Ledger,
}

impl CredentialStore for ScriptedStore {
    fn durable(&self) -> bool {
        self.ledger.durable.get()
    }

    fn read(&self, key: &CredentialKey) -> Result<Option<String>, String> {
        if self.ledger.unreadable.borrow().contains(key) {
            return Err("timed out waiting for the secret service".to_owned());
        }
        self.memory.read(key)
    }

    fn write(&mut self, key: &CredentialKey, password: &str) -> CredentialState {
        self.ledger.writes.borrow_mut().push(key.clone());
        if self.ledger.refused_writes.borrow().contains(key) {
            return CredentialState::Error("the keychain refused the write".to_owned());
        }
        self.memory.write(key, password);
        if self.durable() {
            CredentialState::Available
        } else {
            CredentialState::SessionOnly
        }
    }

    fn remove(&mut self, key: &CredentialKey) -> CredentialState {
        if self.ledger.refused_deletes.borrow().contains(key) {
            return CredentialState::Error("the keychain refused the delete".to_owned());
        }
        self.memory.remove(key)
    }

    fn pending(&self) -> &PendingRemovals {
        self.memory.pending()
    }

    fn pending_mut(&mut self) -> &mut PendingRemovals {
        self.memory.pending_mut()
    }
}

fn clock() -> Rc<dyn Clock> {
    Rc::new(FakeClock::new(
        Utc.with_ymd_and_hms(2026, 10, 23, 9, 0, 0).unwrap(),
    ))
}

/// An ephemeral controller whose secrets go to a [`ScriptedStore`] over `ledger`.
fn controller(ledger: &Ledger) -> Controller {
    let mut controller = Controller::with_backing(
        clock(),
        Box::new(NullNotifier),
        Box::new(NoTransports),
        Backing::Ephemeral,
    );
    controller.credentials = Box::new(ScriptedStore {
        memory: MemoryCredentialStore::default(),
        ledger: ledger.clone(),
    });
    controller
}

/// A complete profile of the libera network, on `host`, with `nick` and `username`.
fn profile(host: &str, nick: &str, username: &str) -> Profile {
    Profile {
        host: host.to_owned(),
        nick: nick.to_owned(),
        username: username.to_owned(),
        ..Profile::new(LIBERA)
    }
}

fn typed(text: &str) -> Option<crate::util::Secret> {
    Some(text.to_owned().into())
}

fn key(profile: &Profile, purpose: &'static str) -> CredentialKey {
    CredentialKey::for_profile(profile, purpose)
}

fn read(controller: &Controller, key: &CredentialKey) -> Option<String> {
    controller
        .credentials
        .read(key)
        .expect("the test store answers")
}

#[test]
fn a_host_change_without_a_retyped_password_moves_the_secret() {
    let ledger = Ledger::default();
    let mut controller = controller(&ledger);
    let before = profile("old.example", "fred", "fred");
    controller.save_profile(
        before.clone(),
        SecretEdits {
            password: typed("hunter2"),
            nickserv: None,
        },
    );
    let after = profile("new.example", "fred", "fred");

    let outcome = controller.save_profile(after.clone(), SecretEdits::default());

    assert_eq!(outcome.password, CredentialState::SessionOnly);
    assert_eq!(
        read(&controller, &key(&after, PURPOSE_PASSWORD)).as_deref(),
        Some("hunter2")
    );
    assert_eq!(
        read(&controller, &key(&before, PURPOSE_PASSWORD)),
        None,
        "the old entry is gone"
    );
    assert!(controller.credentials.pending().network(LIBERA).is_empty());
}

#[test]
fn a_nick_change_moves_the_secret_when_the_username_is_blank() {
    // With no username the key is built from the nick, so a nick change is a key change.
    let ledger = Ledger::default();
    let mut controller = controller(&ledger);
    let before = profile("irc.example", "fred", "");
    controller.save_profile(
        before.clone(),
        SecretEdits {
            password: typed("hunter2"),
            nickserv: None,
        },
    );
    let after = profile("irc.example", "freda", "");

    let outcome = controller.save_profile(after.clone(), SecretEdits::default());

    assert_eq!(outcome.password, CredentialState::SessionOnly);
    assert_eq!(
        read(&controller, &key(&after, PURPOSE_PASSWORD)).as_deref(),
        Some("hunter2")
    );
    assert_eq!(read(&controller, &key(&before, PURPOSE_PASSWORD)), None);
}

#[test]
fn a_retyped_password_on_a_key_change_writes_only_the_new_key() {
    let ledger = Ledger::default();
    let mut controller = controller(&ledger);
    let before = profile("old.example", "fred", "fred");
    controller.save_profile(
        before.clone(),
        SecretEdits {
            password: typed("old"),
            nickserv: None,
        },
    );
    ledger.writes.borrow_mut().clear();
    let after = profile("new.example", "fred", "fred");

    let outcome = controller.save_profile(
        after.clone(),
        SecretEdits {
            password: typed("fresh"),
            nickserv: None,
        },
    );

    assert_eq!(outcome.password, CredentialState::SessionOnly);
    assert_eq!(
        *ledger.writes.borrow(),
        [key(&after, PURPOSE_PASSWORD)],
        "only the new key is written"
    );
    assert_eq!(
        read(&controller, &key(&after, PURPOSE_PASSWORD)).as_deref(),
        Some("fresh")
    );
    assert_eq!(read(&controller, &key(&before, PURPOSE_PASSWORD)), None);
}

#[test]
fn removing_a_network_deletes_its_current_and_its_obsolete_keys() {
    let ledger = Ledger::default();
    let mut controller = controller(&ledger);
    let before = profile("old.example", "fred", "fred");
    controller.save_profile(
        before.clone(),
        SecretEdits {
            password: typed("hunter2"),
            nickserv: typed("nickserv-pass"),
        },
    );
    let after = profile("new.example", "fred", "fred");
    // The old entries cannot be deleted during the change, so both are owed afterwards.
    ledger.refused_deletes.borrow_mut().extend([
        key(&before, PURPOSE_PASSWORD),
        key(&before, PURPOSE_NICKSERV),
    ]);
    controller.save_profile(after.clone(), SecretEdits::default());
    assert_eq!(controller.credentials.pending().network(LIBERA).len(), 2);
    assert_eq!(
        read(&controller, &key(&after, PURPOSE_NICKSERV)).as_deref(),
        Some("nickserv-pass")
    );

    // The keychain takes deletes again, and the network is removed.
    ledger.refused_deletes.borrow_mut().clear();
    assert!(controller.forget_network(LIBERA));

    for stale in [
        key(&before, PURPOSE_PASSWORD),
        key(&before, PURPOSE_NICKSERV),
        key(&after, PURPOSE_PASSWORD),
        key(&after, PURPOSE_NICKSERV),
    ] {
        assert_eq!(read(&controller, &stale), None, "{stale:?} is deleted");
    }
    assert!(controller.credentials.pending().network(LIBERA).is_empty());
}

#[test]
fn a_failed_move_keeps_the_old_secret_until_a_typed_one_replaces_it() {
    let ledger = Ledger::default();
    let mut controller = controller(&ledger);
    let before = profile("old.example", "fred", "fred");
    controller.save_profile(
        before.clone(),
        SecretEdits {
            password: typed("hunter2"),
            nickserv: None,
        },
    );
    let after = profile("new.example", "fred", "fred");
    ledger
        .refused_writes
        .borrow_mut()
        .push(key(&after, PURPOSE_PASSWORD));

    let outcome = controller.save_profile(after.clone(), SecretEdits::default());

    assert!(
        matches!(outcome.password, CredentialState::Error(_)),
        "{:?}",
        outcome.password
    );
    assert_eq!(
        read(&controller, &key(&before, PURPOSE_PASSWORD)).as_deref(),
        Some("hunter2"),
        "the only copy is kept"
    );
    assert_eq!(
        controller
            .credentials
            .pending()
            .slot(LIBERA, PURPOSE_PASSWORD),
        [key(&before, PURPOSE_PASSWORD)]
    );

    ledger.refused_writes.borrow_mut().clear();
    let outcome = controller.save_profile(
        after.clone(),
        SecretEdits {
            password: typed("fresh"),
            nickserv: None,
        },
    );

    assert_eq!(outcome.password, CredentialState::SessionOnly);
    assert_eq!(
        read(&controller, &key(&after, PURPOSE_PASSWORD)).as_deref(),
        Some("fresh")
    );
    assert_eq!(
        read(&controller, &key(&before, PURPOSE_PASSWORD)),
        None,
        "the old entry goes once the new one holds"
    );
    assert!(controller.credentials.pending().network(LIBERA).is_empty());
}

#[test]
fn a_secret_the_store_cannot_read_is_an_error_and_its_key_is_kept() {
    let ledger = Ledger::default();
    let mut controller = controller(&ledger);
    let before = profile("old.example", "fred", "fred");
    controller.save_profile(
        before.clone(),
        SecretEdits {
            password: typed("hunter2"),
            nickserv: None,
        },
    );
    let after = profile("new.example", "fred", "fred");
    ledger.writes.borrow_mut().clear();
    ledger
        .unreadable
        .borrow_mut()
        .push(key(&before, PURPOSE_PASSWORD));

    let outcome = controller.save_profile(after.clone(), SecretEdits::default());

    assert!(
        matches!(&outcome.password, CredentialState::Error(message) if message.starts_with("timed out")),
        "a timeout is not a missing secret: {:?}",
        outcome.password
    );
    assert!(
        ledger.writes.borrow().is_empty(),
        "nothing is written from an unread key"
    );
    ledger.unreadable.borrow_mut().clear();
    assert_eq!(
        read(&controller, &key(&before, PURPOSE_PASSWORD)).as_deref(),
        Some("hunter2")
    );
    assert_eq!(
        controller.credentials.pending().network(LIBERA),
        [key(&before, PURPOSE_PASSWORD)]
    );
}

#[test]
fn an_unanswered_read_keeps_the_saved_flag_and_a_missing_secret_clears_it() {
    let ledger = Ledger::default();
    ledger.durable.set(true);
    let mut controller = controller(&ledger);
    let saved = profile("irc.example", "fred", "fred");
    let outcome = controller.save_profile(
        saved.clone(),
        SecretEdits {
            password: typed("hunter2"),
            nickserv: None,
        },
    );
    assert_eq!(outcome.password, CredentialState::Available);
    assert!(controller
        .profile(LIBERA)
        .is_some_and(|stored| stored.secret_saved));

    // The keychain times out on the read Apply makes, and the secret is still there.
    ledger
        .unreadable
        .borrow_mut()
        .push(key(&saved, PURPOSE_PASSWORD));
    let outcome = controller.save_profile(saved.clone(), SecretEdits::default());
    assert!(matches!(outcome.password, CredentialState::Error(_)));
    assert!(
        controller
            .profile(LIBERA)
            .is_some_and(|stored| stored.secret_saved),
        "a refused read says nothing, so the flag stays"
    );

    // The keychain answers and holds nothing, so the flag says so.
    ledger.unreadable.borrow_mut().clear();
    controller
        .credentials
        .remove(&key(&saved, PURPOSE_PASSWORD));
    let outcome = controller.save_profile(saved.clone(), SecretEdits::default());
    assert_eq!(outcome.password, CredentialState::Missing);
    assert!(!controller
        .profile(LIBERA)
        .is_some_and(|stored| stored.secret_saved));
}
