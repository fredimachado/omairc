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
    PURPOSE_NICKSERV, PURPOSE_PASSWORD, ReadSlot,
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

/// Keeps secrets in memory but refuses every delete, like a locked keychain.
struct FailingRemove(MemoryCredentialStore);

impl CredentialStore for FailingRemove {
    fn durable(&self) -> bool {
        true
    }

    fn read(&self, key: &CredentialKey) -> Result<Option<String>, String> {
        self.0.read(key)
    }

    fn write(&mut self, key: &CredentialKey, password: &str) -> CredentialState {
        self.0.write(key, password);
        CredentialState::Available
    }

    fn remove(&mut self, _key: &CredentialKey) -> CredentialState {
        CredentialState::Error("remove failed".to_owned())
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
    assert_eq!(
        controller.pending_startup_connect_ids(),
        vec![NET.to_owned()],
        "startup connect waits on the keychain"
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while controller.session(NET).is_none() && Instant::now() < deadline {
        controller.poll_credential_reads();
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        read_slot(controller.credentials.as_ref(), &profile, PURPOSE_PASSWORD),
        ReadSlot::Ready(Ok(Some("hunter2".to_owned())))
    );
    assert!(
        controller.session(NET).is_some(),
        "poll resumes startup connect after the keychain settles"
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
    assert!(controller.pending_startup_connect_ids().is_empty());
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
    assert!(controller.pending_startup_connect_ids().is_empty());
}

#[test]
fn poll_credential_reads_clears_a_failed_startup_connect() {
    let loopbacks = Loopbacks::default();
    let profile = complete_profile();
    let attempts = Arc::new(Mutex::new(0_u32));
    let store = DeferringCredentialStore::new(
        Box::new(MemoryCredentialStore::default()),
        {
            let attempts = Arc::clone(&attempts);
            move |_key| {
                thread::sleep(Duration::from_millis(400));
                *attempts.lock().expect("lock") += 1;
                Err("timed out".to_owned())
            }
        },
    );
    let mut controller =
        Controller::with_backing(clock(), Box::new(loopbacks), Backing::Ephemeral);
    controller.credentials = store;
    controller.profiles.insert(NET.to_owned(), profile);
    controller
        .pending_session_registrations
        .insert(NET.to_owned());
    assert_eq!(controller.activate_startup(), 0, "read still in flight");
    assert!(
        controller
            .pending_session_registrations
            .contains(NET),
        "load_stored leaves the network pending until credentials settle"
    );
    assert_eq!(
        controller.pending_startup_connect_ids(),
        vec![NET.to_owned()],
        "startup connect waits on the keychain"
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while !controller.pending_startup_connect_ids().is_empty()
        && Instant::now() < deadline
    {
        controller.poll_credential_reads();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(controller.pending_startup_connect_ids().is_empty());
    assert!(controller.session(NET).is_none());
    assert_eq!(
        controller.last_error_for(NET),
        Some("The saved password could not be read from secure storage.")
    );
    assert_eq!(*attempts.lock().expect("lock"), 1);
}

#[test]
fn poll_credential_reads_starts_a_startup_connect_after_a_deferred_read() {
    let loopbacks = Loopbacks::default();
    let profile = complete_profile();
    let key = CredentialKey::for_profile(&profile, PURPOSE_PASSWORD);
    let secrets = Arc::new(Mutex::new(MemoryCredentialStore::default()));
    secrets
        .lock()
        .expect("lock")
        .write(&key, "hunter2");
    let attempts = Arc::new(Mutex::new(0_u32));
    let store = DeferringCredentialStore::new(
        Box::new(SlowMemoryStore {
            secrets: Arc::clone(&secrets),
            delay: Duration::from_millis(400),
            pending: PendingRemovals::default(),
        }),
        {
            let secrets = Arc::clone(&secrets);
            let attempts = Arc::clone(&attempts);
            move |key| {
                thread::sleep(Duration::from_millis(400));
                *attempts.lock().expect("lock") += 1;
                secrets.lock().expect("lock").read(key)
            }
        },
    );
    let mut controller =
        Controller::with_backing(clock(), Box::new(loopbacks.clone()), Backing::Ephemeral);
    controller.credentials = store;
    controller.profiles.insert(NET.to_owned(), profile.clone());
    controller
        .pending_session_registrations
        .insert(NET.to_owned());
    assert_eq!(controller.activate_startup(), 0, "read still in flight");
    assert_eq!(
        controller.pending_startup_connect_ids(),
        vec![NET.to_owned()]
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while controller.session(NET).is_none() && Instant::now() < deadline {
        controller.poll_credential_reads();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        controller.pending_startup_connect_ids().is_empty(),
        "startup connect leaves the pending set once settled"
    );
    assert!(
        controller.session(NET).is_some(),
        "poll resumes startup connect after the keychain settles"
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
    assert_eq!(*attempts.lock().expect("lock"), 1);
}

#[test]
fn save_profile_reports_loading_instead_of_refusing_apply() {
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
    let loopbacks = Loopbacks::default();
    let mut controller =
        Controller::with_backing(clock(), Box::new(loopbacks), Backing::Ephemeral);
    controller.credentials = store;
    controller.profiles.insert(NET.to_owned(), profile.clone());
    let outcome = controller.save_profile(profile.clone(), SecretEdits::default());
    assert_eq!(outcome.password, CredentialState::Loading);
    assert!(controller.connect_allowed_after_save(&profile, &outcome));
    let started = Instant::now();
    assert!(
        !controller.start(NET),
        "start must not open a session while the keychain read is in flight"
    );
    assert!(
        started.elapsed() < Duration::from_millis(50),
        "start blocked on the keyring"
    );
    assert!(controller.session(NET).is_none());
    assert_eq!(controller.last_error_for(NET), None);
    let deadline = Instant::now() + Duration::from_secs(3);
    while controller.session(NET).is_none() && Instant::now() < deadline {
        controller.poll_credential_reads();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        controller.session(NET).is_some(),
        "poll opens the session after the deferred read settles"
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
fn a_deferred_read_error_is_not_cached_forever() {
    let profile = complete_profile();
    let key = CredentialKey::for_profile(&profile, PURPOSE_PASSWORD);
    let secrets = Arc::new(Mutex::new(MemoryCredentialStore::default()));
    secrets
        .lock()
        .expect("lock")
        .write(&key, "hunter2");
    let attempts = Arc::new(Mutex::new(0_u32));
    let store = DeferringCredentialStore::new(
        Box::new(MemoryCredentialStore::default()),
        {
            let secrets = Arc::clone(&secrets);
            let attempts = Arc::clone(&attempts);
            move |key| {
                let mut count = attempts.lock().expect("lock");
                *count += 1;
                if *count == 1 {
                    Err("timed out".to_owned())
                } else {
                    secrets.lock().expect("lock").read(key)
                }
            }
        },
    );
    assert_eq!(
        read_slot(store.as_ref(), &profile, PURPOSE_PASSWORD),
        ReadSlot::Loading
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut first_answer = ReadSlot::Loading;
    while Instant::now() < deadline {
        first_answer = read_slot(store.as_ref(), &profile, PURPOSE_PASSWORD);
        if matches!(first_answer, ReadSlot::Ready(Err(_))) {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        first_answer,
        ReadSlot::Ready(Err("timed out".to_owned()))
    );
    for _ in 0..5 {
        assert_eq!(
            read_slot(store.as_ref(), &profile, PURPOSE_PASSWORD),
            ReadSlot::Ready(Err("timed out".to_owned())),
            "a settled Err stays until retry_read"
        );
    }
    assert_eq!(*attempts.lock().expect("lock"), 1);
    store.retry_read(&key);
    // Inflight is the race-free stand-in for Loading: read_slot may settle a
    // finished worker before this thread observes Loading.
    assert!(
        store.deferred_read_inflight(&key),
        "retry_read starts one new worker before settle"
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut answer = ReadSlot::Loading;
    while Instant::now() < deadline {
        answer = read_slot(store.as_ref(), &profile, PURPOSE_PASSWORD);
        if !matches!(answer, ReadSlot::Loading) {
            break;
        }
        store.poll_reads();
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        answer,
        ReadSlot::Ready(Ok(Some("hunter2".to_owned())))
    );
    assert_eq!(*attempts.lock().expect("lock"), 2);
    assert_eq!(
        read_slot(store.as_ref(), &profile, PURPOSE_PASSWORD),
        ReadSlot::Ready(Ok(Some("hunter2".to_owned()))),
        "a second read_slot alone must not be the retry"
    );
    assert_eq!(*attempts.lock().expect("lock"), 2);
}

#[test]
fn poll_credential_reads_refuses_two_failed_startup_connects() {
    let loopbacks = Loopbacks::default();
    let net_a = "net-a";
    let net_b = "net-b";
    let profile_a = Profile {
        host: "irc.a.example".to_owned(),
        nick: "fred".to_owned(),
        secret_saved: true,
        connect_on_startup: true,
        ..Profile::new(net_a)
    };
    let profile_b = Profile {
        host: "irc.b.example".to_owned(),
        nick: "fred".to_owned(),
        secret_saved: true,
        connect_on_startup: true,
        ..Profile::new(net_b)
    };
    let key_a = CredentialKey::for_profile(&profile_a, PURPOSE_PASSWORD);
    let key_b = CredentialKey::for_profile(&profile_b, PURPOSE_PASSWORD);
    let attempts = Arc::new(Mutex::new(BTreeMap::<String, u32>::new()));
    let store = DeferringCredentialStore::new(
        Box::new(MemoryCredentialStore::default()),
        {
            let attempts = Arc::clone(&attempts);
            move |key| {
                *attempts
                    .lock()
                    .expect("lock")
                    .entry(key.network_id.clone())
                    .or_insert(0) += 1;
                thread::sleep(Duration::from_millis(400));
                Err("timed out".to_owned())
            }
        },
    );
    let mut controller =
        Controller::with_backing(clock(), Box::new(loopbacks), Backing::Ephemeral);
    controller.credentials = store;
    controller.profiles.insert(net_a.to_owned(), profile_a);
    controller.profiles.insert(net_b.to_owned(), profile_b);
    controller
        .pending_session_registrations
        .insert(net_a.to_owned());
    controller
        .pending_session_registrations
        .insert(net_b.to_owned());
    assert_eq!(controller.activate_startup(), 0, "reads still in flight");
    let mut pending = controller.pending_startup_connect_ids();
    pending.sort();
    assert_eq!(
        pending,
        vec![net_a.to_owned(), net_b.to_owned()],
        "startup connect waits on both keychains"
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while !controller.pending_startup_connect_ids().is_empty() && Instant::now() < deadline {
        controller.poll_credential_reads();
        thread::sleep(Duration::from_millis(5));
    }
    assert!(controller.pending_startup_connect_ids().is_empty());
    assert!(controller.session(net_a).is_none());
    assert!(controller.session(net_b).is_none());
    assert_eq!(
        controller.last_error_for(net_a),
        Some("The saved password could not be read from secure storage.")
    );
    assert_eq!(
        controller.last_error_for(net_b),
        Some("The saved password could not be read from secure storage.")
    );
    let attempt_counts = attempts.lock().expect("lock").clone();
    assert_eq!(attempt_counts.get(net_a), Some(&1));
    assert_eq!(attempt_counts.get(net_b), Some(&1));
    controller.poll_credential_reads();
    assert!(
        !controller.credentials.deferred_read_inflight(&key_a),
        "settled Err must not leave a password read in flight for net-a"
    );
    assert!(
        !controller.credentials.deferred_read_inflight(&key_b),
        "settled Err must not leave a password read in flight for net-b"
    );
    let attempt_counts = attempts.lock().expect("lock").clone();
    assert_eq!(attempt_counts.get(net_a), Some(&1));
    assert_eq!(attempt_counts.get(net_b), Some(&1));
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

#[test]
fn forget_secret_deletes_the_keychain_entry_immediately() {
    use crate::model::connect::{ConnectField, ConnectState, Secret};
    use crate::storage::profilestore::Profile as StoredProfile;

    let mut controller =
        Controller::with_backing(clock(), Box::new(NoTransports), Backing::Ephemeral);
    controller.credentials = Box::new(MemoryCredentialStore::default());
    let profile = complete_profile();
    controller.save_profile(
        profile.clone(),
        SecretEdits {
            password: Some("hunter2".to_owned().into()),
            nickserv: Some("nickpw".to_owned().into()),
        },
    );
    // MemoryCredentialStore is session-only, so pin the flags a durable keychain would leave.
    {
        let mut pinned = controller.profile(NET).expect("profile").clone();
        pinned.secret_saved = true;
        pinned.nickserv_saved = true;
        controller.profiles.insert(NET.to_owned(), pinned);
    }
    assert_eq!(
        read_slot_sync(controller.credentials.as_ref(), &profile, PURPOSE_PASSWORD),
        Ok(Some("hunter2".to_owned()))
    );
    assert_eq!(
        read_slot_sync(controller.credentials.as_ref(), &profile, PURPOSE_NICKSERV),
        Ok(Some("nickpw".to_owned()))
    );

    let mut stored = StoredProfile {
        host: profile.host.clone(),
        nick: profile.nick.clone(),
        secret_saved: true,
        nickserv_saved: true,
        ..StoredProfile::new(NET)
    };
    stored.port = profile.port;
    let mut state = ConnectState::from_stored(&[stored]);
    assert!(state.can_forget(Secret::Password));
    assert_eq!(
        state.credential_status(),
        "password saved securely; NickServ saved securely"
    );

    let network_id = state.forget(Secret::Password).expect("saved network");
    let settled = controller.forget_secret(&network_id, PURPOSE_PASSWORD);
    assert_eq!(settled, CredentialState::Missing);
    state.apply_forget_result(Secret::Password, settled);
    assert_eq!(
        read_slot_sync(controller.credentials.as_ref(), &profile, PURPOSE_PASSWORD),
        Ok(None),
        "server password is gone from the store"
    );
    assert_eq!(
        read_slot_sync(controller.credentials.as_ref(), &profile, PURPOSE_NICKSERV),
        Ok(Some("nickpw".to_owned())),
        "NickServ is untouched"
    );
    assert!(!controller.profile(NET).expect("profile").secret_saved);
    assert!(controller.profile(NET).expect("profile").nickserv_saved);
    assert_eq!(state.credential_status(), "NickServ saved securely");
    assert!(!state.can_forget(Secret::Password));

    let network_id = state.forget(Secret::NickServ).expect("saved network");
    let settled = controller.forget_secret(&network_id, PURPOSE_NICKSERV);
    assert_eq!(settled, CredentialState::Missing);
    state.apply_forget_result(Secret::NickServ, settled);
    assert_eq!(
        read_slot_sync(controller.credentials.as_ref(), &profile, PURPOSE_NICKSERV),
        Ok(None)
    );
    assert_eq!(state.credential_status(), "");
    assert!(!controller.profile(NET).expect("profile").nickserv_saved);

    state.set_text(ConnectField::ServerPassword, "again");
    controller.credentials.write(
        &CredentialKey::for_profile(&profile, PURPOSE_PASSWORD),
        "again",
    );
    {
        let mut pinned = controller.profile(NET).expect("profile").clone();
        pinned.secret_saved = true;
        controller.profiles.insert(NET.to_owned(), pinned);
    }
    state.apply_forget_result(Secret::Password, CredentialState::Available);
    let network_id = state.forget(Secret::Password).expect("held");
    let settled = controller.forget_secret(&network_id, PURPOSE_PASSWORD);
    assert_eq!(settled, CredentialState::Missing);
    state.apply_forget_result(Secret::Password, settled);
    assert_eq!(
        read_slot_sync(controller.credentials.as_ref(), &profile, PURPOSE_PASSWORD),
        Ok(None)
    );
    assert_eq!(state.text(ConnectField::ServerPassword), "");
}

#[test]
fn forget_secret_reports_error_when_the_keychain_refuses_the_delete() {
    use crate::model::connect::{ConnectState, Secret};
    use crate::storage::profilestore::Profile as StoredProfile;

    let mut controller =
        Controller::with_backing(clock(), Box::new(NoTransports), Backing::Ephemeral);
    controller.credentials = Box::new(FailingRemove(MemoryCredentialStore::default()));
    let profile = complete_profile();
    controller.save_profile(
        profile.clone(),
        SecretEdits {
            password: Some("hunter2".to_owned().into()),
            nickserv: None,
        },
    );
    {
        let mut pinned = controller.profile(NET).expect("profile").clone();
        pinned.secret_saved = true;
        controller.profiles.insert(NET.to_owned(), pinned);
    }

    let mut stored = StoredProfile {
        host: profile.host.clone(),
        nick: profile.nick.clone(),
        secret_saved: true,
        ..StoredProfile::new(NET)
    };
    stored.port = profile.port;
    let mut state = ConnectState::from_stored(&[stored]);
    assert!(state.can_forget(Secret::Password));

    let network_id = state.forget(Secret::Password).expect("saved network");
    let settled = controller.forget_secret(&network_id, PURPOSE_PASSWORD);
    assert_eq!(
        settled,
        CredentialState::Error("remove failed".to_owned())
    );
    state.apply_forget_result(Secret::Password, settled);
    assert!(
        state.can_forget(Secret::Password),
        "link stays forgettable after a refused delete"
    );
    assert_eq!(
        state.credential_status(),
        "secure storage error; password is not saved"
    );
    assert!(
        controller.profile(NET).expect("profile").secret_saved,
        "saved flag stays when the key is still there"
    );
    assert_eq!(
        read_slot_sync(controller.credentials.as_ref(), &profile, PURPOSE_PASSWORD),
        Ok(Some("hunter2".to_owned())),
        "store still holds the secret"
    );
}
