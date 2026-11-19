//! Credential invariants: Qt-compatible key names, unreadable secrets, removal ordering
//! and live-session reconciliation after Apply.

use std::cell::{Cell, RefCell};
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
    read_slot_sync, CredentialKey, CredentialState, CredentialStore, LEGACY_PURPOSE_PASSWORD,
    MemoryCredentialStore, PURPOSE_NICKSERV, PURPOSE_PASSWORD,
};
use crate::storage::profilestore::Profile;
use crate::storage::settings::Status;
use crate::storage::Backing;

pub(super) const NET: &str = "net";

#[derive(Clone, Default)]
pub(super) struct Loopbacks(
    Rc<RefCell<BTreeMap<String, Rc<LoopbackTransport>>>>,
    Rc<Cell<usize>>,
);

impl TransportFactory for Loopbacks {
    fn open(&self, network_id: &str) -> Result<Rc<dyn Transport>, String> {
        let transport = Rc::new(LoopbackTransport::new());
        self.1.set(self.1.get() + 1);
        self.0
            .borrow_mut()
            .insert(network_id.to_owned(), transport.clone());
        Ok(transport)
    }
}

impl Loopbacks {
    /// How many transports the factory has opened, over all networks.
    pub(super) fn opened(&self) -> usize {
        self.1.get()
    }

    pub(super) fn get(&self, network_id: &str) -> Rc<LoopbackTransport> {
        self.0.borrow()[network_id].clone()
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

pub(super) fn clock() -> Rc<dyn Clock> {
    Rc::new(FakeClock::new(
        Utc.with_ymd_and_hms(2026, 10, 24, 9, 0, 0).unwrap(),
    ))
}

pub(super) fn complete_profile() -> Profile {
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

/// A controller on loopbacks with `complete_profile` saved (password "first") and not set to
/// connect at launch.
fn applied_controller() -> (Controller, Loopbacks, Profile) {
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
    (controller, loopbacks, profile)
}

#[test]
fn apply_on_an_unchanged_profile_reuses_the_session() {
    let (mut controller, loopbacks, profile) = applied_controller();
    assert!(controller.start(NET));
    let first = loopbacks.get(NET);
    controller.save_profile(profile, SecretEdits::default());
    assert!(controller.start(NET));
    assert!(
        Rc::ptr_eq(&first, &loopbacks.get(NET)),
        "an unchanged profile and secret keep the registered session"
    );
}

#[test]
fn apply_rebuilds_the_session_when_the_profile_or_the_secret_changes() {
    let (mut controller, loopbacks, profile) = applied_controller();
    assert!(controller.start(NET));
    let first = loopbacks.get(NET);

    let moved = Profile {
        host: "other.example".to_owned(),
        ..profile.clone()
    };
    controller.save_profile(moved.clone(), SecretEdits::default());
    assert!(controller.start(NET));
    let second = loopbacks.get(NET);
    assert!(!Rc::ptr_eq(&first, &second), "a changed profile rebuilds");

    controller.save_profile(
        moved,
        SecretEdits {
            password: Some("second".to_owned().into()),
            nickserv: None,
        },
    );
    assert!(controller.start(NET));
    assert!(
        !Rc::ptr_eq(&second, &loopbacks.get(NET)),
        "a changed secret rebuilds"
    );
}

#[test]
fn disconnect_and_remove_retire_the_session() {
    let (mut controller, _loopbacks, _profile) = applied_controller();
    assert!(controller.start(NET));
    assert!(controller.can_disconnect(NET));
    assert!(controller.disconnect(NET));
    assert!(!controller.can_disconnect(NET), "disconnect stops it");

    assert!(controller.start(NET));
    assert_eq!(controller.forget_network(NET), Status::Written);
    assert!(controller.session(NET).is_none(), "remove retires it");
    assert!(controller.applied_sessions.get(NET).is_none());
}
