//! Credential invariants: Qt-compatible key names, unreadable secrets, removal ordering,
//! and live-session reconciliation after Apply.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use chrono::{TimeZone, Utc};

use crate::controller::controller::Controller;
use crate::controller::persistence::{SaveOutcome, SecretEdits};
use crate::controller::transports::{TransportFactory, NoTransports};
use crate::session::clock::{Clock, FakeClock};
use crate::session::loopback::LoopbackTransport;
use crate::session::transport::Transport;
use crate::storage::credentialstore::{
    read_slot, read_slot_sync, CredentialKey, CredentialState, CredentialStore,
    DeferringCredentialStore, LEGACY_PURPOSE_PASSWORD, MemoryCredentialStore, PendingRemovals,
    PURPOSE_PASSWORD, ReadSlot,
};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use crate::storage::profilestore::Profile;
use crate::storage::settings::Status;
use crate::storage::Backing;
use crate::util::testdir::TempDir;

const NET: &str = "net";

#[derive(Clone, Default)]
struct Loopbacks(Rc<RefCell<BTreeMap<String, Rc<LoopbackTransport>>>>);

impl TransportFactory for Loopbacks {
    fn open(&self, network_id: &str) -> Result<Rc<dyn Transport>, String> {
        let transport = Rc::new(LoopbackTransport::new());
        self.0
            .borrow_mut()
            .insert(network_id.to_owned(), transport.clone());
        Ok(transport)
    }
}

impl Loopbacks {
    fn get(&self, network_id: &str) -> Rc<LoopbackTransport> {
        self.0.borrow()[network_id].clone()
    }
}

struct FailingRead(MemoryCredentialStore);

impl CredentialStore for FailingRead {
    fn durable(&self) -> bool {
        true
    }

    fn read(&self, _key: &CredentialKey) -> Result<Option<String>, String> {
        Err("timed out".to_owned())
    }

    fn write(&mut self, key: &CredentialKey, password: &str) -> CredentialState {
        self.0.write(key, password)
    }

    fn remove(&mut self, key: &CredentialKey) -> CredentialState {
        self.0.remove(key)
    }

    fn pending(&self) -> &crate::storage::credentialstore::PendingRemovals {
        self.0.pending()
    }

    fn pending_mut(&mut self) -> &mut crate::storage::credentialstore::PendingRemovals {
        self.0.pending_mut()
    }
}

fn clock() -> Rc<dyn Clock> {
    Rc::new(FakeClock::new(
        Utc.with_ymd_and_hms(2026, 10, 24, 9, 0, 0).unwrap(),
    ))
}

fn complete_profile() -> Profile {
    Profile {
        host: "irc.example".to_owned(),
        nick: "fred".to_owned(),
        secret_saved: true,
        connect_on_startup: true,
        ..Profile::new(NET)
    }
}

#[test]
fn qt_server_password_keys_are_read_through_the_legacy_suffix() {
    let mut store = MemoryCredentialStore::default();
    let profile = complete_profile();
    let legacy = CredentialKey::for_profile(&profile, LEGACY_PURPOSE_PASSWORD);
    store.write(&legacy, "hunter2");
    assert_eq!(
        read_slot_sync(&store, &profile, PURPOSE_PASSWORD),
        Ok(Some("hunter2".to_owned()))
    );
}

struct SlowMemoryStore {
    secrets: Arc<Mutex<MemoryCredentialStore>>,
    delay: Duration,
    pending: PendingRemovals,
}

impl CredentialStore for SlowMemoryStore {
    fn durable(&self) -> bool {
        true
    }

    fn read(&self, key: &CredentialKey) -> Result<Option<String>, String> {
        thread::sleep(self.delay);
        self.secrets.lock().expect("lock").read(key)
    }

    fn write(&mut self, key: &CredentialKey, password: &str) -> CredentialState {
        self.secrets.lock().expect("lock").write(key, password)
    }

    fn remove(&mut self, key: &CredentialKey) -> CredentialState {
        self.secrets.lock().expect("lock").remove(key)
    }

    fn pending(&self) -> &PendingRemovals {
        &self.pending
    }

    fn pending_mut(&mut self) -> &mut PendingRemovals {
        &mut self.pending
    }
}

#[test]
fn activate_startup_waits_for_a_deferred_keychain_read() {
    let loopbacks = Loopbacks::default();
    let profile = complete_profile();
    let key = CredentialKey::for_profile(&profile, PURPOSE_PASSWORD);
    let secrets = Arc::new(Mutex::new(MemoryCredentialStore::default()));
    secrets
        .lock()
        .expect("lock")
        .write(&key, "hunter2");
    let store = DeferringCredentialStore::new(
        Box::new(SlowMemoryStore {
            secrets: Arc::clone(&secrets),
            delay: Duration::from_millis(400),
            pending: PendingRemovals::default(),
        }),
        {
            let secrets = Arc::clone(&secrets);
            move |key| {
                thread::sleep(Duration::from_millis(400));
                secrets.lock().expect("lock").read(key)
            }
        },
    );
    let mut controller =
        Controller::with_backing(clock(), Box::new(loopbacks.clone()), Backing::Ephemeral);
    controller.credentials = store;
    controller.profiles.insert(NET.to_owned(), profile.clone());
    let started = Instant::now();
    assert_eq!(controller.activate_startup(), 0, "read still in flight");
    assert!(
        started.elapsed() < Duration::from_millis(50),
        "activate_startup blocked on the keyring"
    );
    assert!(controller.session(NET).is_none());
    let deadline = Instant::now() + Duration::from_secs(3);
    while matches!(
        read_slot(controller.credentials.as_ref(), &profile, PURPOSE_PASSWORD),
        ReadSlot::Loading
    ) && Instant::now() < deadline
    {
        controller.poll_credential_reads();
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        read_slot(controller.credentials.as_ref(), &profile, PURPOSE_PASSWORD),
        ReadSlot::Ready(Ok(Some("hunter2".to_owned())))
    );
    assert!(
        controller.start(NET),
        "connect once the keychain read has settled"
    );
    assert_eq!(
        controller
            .session(NET)
            .expect("session")
            .upload_credential()
            .1
            .expose(),
        "hunter2"
    );
}

#[test]
fn read_slot_returns_loading_without_waiting_on_the_keyring() {
    let profile = complete_profile();
    let key = CredentialKey::for_profile(&profile, PURPOSE_PASSWORD);
    let secrets = Arc::new(Mutex::new(MemoryCredentialStore::default()));
    secrets
        .lock()
        .expect("lock")
        .write(&key, "hunter2");
    let store = DeferringCredentialStore::new(
        Box::new(SlowMemoryStore {
            secrets: Arc::clone(&secrets),
            delay: Duration::from_secs(2),
            pending: PendingRemovals::default(),
        }),
        {
            let secrets = Arc::clone(&secrets);
            move |key| {
                thread::sleep(Duration::from_secs(2));
                secrets.lock().expect("lock").read(key)
            }
        },
    );
    let started = Instant::now();
    assert_eq!(
        read_slot(store.as_ref(), &profile, PURPOSE_PASSWORD),
        ReadSlot::Loading
    );
    assert!(started.elapsed() < Duration::from_millis(50));
}

#[test]
fn activate_startup_skips_a_profile_whose_saved_password_cannot_be_read() {
    let mut controller =
        Controller::with_backing(clock(), Box::new(NoTransports), Backing::Ephemeral);
    controller.credentials = Box::new(FailingRead(MemoryCredentialStore::default()));
    controller.profiles.insert(NET.to_owned(), complete_profile());
    assert_eq!(controller.activate_startup(), 0);
    assert!(controller.session(NET).is_none());
}

#[test]
fn a_failed_profile_removal_leaves_credentials_in_place() {
    let dir = TempDir::new("remove-creds");
    let backing = Backing::Disk(dir.path().join("omairc.conf"));
    let mut controller = Controller::with_backing(clock(), Box::new(NoTransports), backing);
    controller.credentials = Box::new(MemoryCredentialStore::default());
    let profile = complete_profile();
    controller.save_profile(
        profile.clone(),
        SecretEdits {
            password: Some("hunter2".to_owned().into()),
            nickserv: None,
        },
    );
    std::fs::write(dir.path().join("omairc.conf"), "not ini").expect("broken file");

    assert!(!controller.forget_network(NET));
    assert_eq!(
        read_slot_sync(controller.credentials.as_ref(), &profile, PURPOSE_PASSWORD),
        Ok(Some("hunter2".to_owned()))
    );
    assert!(controller.profile(NET).is_some());
}

#[test]
fn apply_rebuilds_a_live_session_when_only_the_password_changes() {
    let loopbacks = Loopbacks::default();
    let mut controller =
        Controller::with_backing(clock(), Box::new(loopbacks.clone()), Backing::Ephemeral);
    controller.credentials = Box::new(MemoryCredentialStore::default());
    let profile = Profile {
        connect_on_startup: false,
        ..complete_profile()
    };
    controller.save_profile(
        profile.clone(),
        SecretEdits {
            password: Some("first".to_owned().into()),
            nickserv: None,
        },
    );
    assert!(controller.start(NET));
    let first = controller
        .session(NET)
        .expect("session")
        .upload_credential()
        .1;

    controller.save_profile(
        profile,
        SecretEdits {
            password: Some("second".to_owned().into()),
            nickserv: None,
        },
    );
    assert!(controller.start(NET));
    let second = controller
        .session(NET)
        .expect("session")
        .upload_credential()
        .1;
    assert_eq!(first.expose(), "first");
    assert_eq!(second.expose(), "second");
}

#[test]
fn connect_allowed_after_save_refuses_a_credential_error() {
    let controller =
        Controller::with_backing(clock(), Box::new(NoTransports), Backing::Ephemeral);
    let profile = complete_profile();
    let outcome = SaveOutcome {
        status: Status::Written,
        password: CredentialState::Error("denied".to_owned()),
        nickserv: CredentialState::Missing,
    };
    assert!(!controller.connect_allowed_after_save(&profile, &outcome));
}
