//! Forget must not drop profile or secrets when the combined settings write is
//! blocked. Shared with the TUI `RemoveSelected` Sync-failure contract.

use std::rc::Rc;

use chrono::{TimeZone, Utc};

use crate::controller::controller::Controller;
use crate::controller::persistence::SecretEdits;
use crate::controller::transports::NoTransports;
use crate::model::connect::{ConnectState, RemoveOutcome};
use crate::session::clock::{Clock, FakeClock};
use crate::storage::credentialstore::{read_slot_sync, MemoryCredentialStore, PURPOSE_PASSWORD};
use crate::storage::profilestore::Profile;
use crate::storage::settings::Status;
use crate::storage::Backing;
use crate::util::testdir::TempDir;

const NET: &str = "net";

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

    assert_eq!(controller.forget_network(NET), Status::FormatError);
    assert_eq!(
        read_slot_sync(controller.credentials.as_ref(), &profile, PURPOSE_PASSWORD),
        Ok(Some("hunter2".to_owned()))
    );
    assert!(controller.profile(NET).is_some());
}

/// Preflight can succeed while the combined forget Sync still fails (for example the
/// parent directory refuses a new temp file). Memory and secrets must stay until that
/// write reports success — otherwise conf still holds `networks/<id>` with no keys.
#[cfg(unix)]
#[test]
fn a_failed_forget_sync_leaves_profile_and_credentials_in_place() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new("forget-sync-creds");
    let conf = dir.path().join("omairc.conf");
    let backing = Backing::Disk(conf.clone());
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
    assert!(conf.is_file(), "preflight needs a readable conf after save");
    // Directory not writable → atomic rename fails; the conf file itself still
    // probes as writable so preflight_remove returns Written.
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))
        .expect("chmod dir");

    let status = controller.forget_network(NET);

    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
        .expect("restore dir mode");

    assert_eq!(
        status,
        Status::AccessError,
        "forget must return AccessError when Sync fails after preflight"
    );
    assert_eq!(
        read_slot_sync(controller.credentials.as_ref(), &profile, PURPOSE_PASSWORD),
        Ok(Some("hunter2".to_owned())),
        "secrets must stay when the combined forget write fails"
    );
    assert!(
        controller.profile(NET).is_some(),
        "in-memory profile must stay when Sync fails"
    );
    let text = std::fs::read_to_string(&conf).expect("conf still readable");
    assert!(
        text.contains("net\\host=irc.example"),
        "conf must still hold the profile group after a failed Sync:\n{text}"
    );
}

/// The Connect sheet's Remove path goes through `remove_with_forget`, which calls
/// forget then `complete_remove` with that Status. Hard-coding Written in the
/// helper (or calling `complete_remove` with Written despite a failed forget)
/// drops the row and fails this test.
#[cfg(unix)]
#[test]
fn connect_remove_keeps_the_row_when_forget_sync_fails() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new("forget-sync-sheet");
    let conf = dir.path().join("omairc.conf");
    let backing = Backing::Disk(conf.clone());
    let mut controller = Controller::with_backing(clock(), Box::new(NoTransports), backing);
    controller.credentials = Box::new(MemoryCredentialStore::default());
    let keep = Profile {
        host: "irc.keep.example".to_owned(),
        nick: "oak".to_owned(),
        secret_saved: true,
        connect_on_startup: true,
        ..Profile::new("keep")
    };
    controller.save_profile(
        keep.clone(),
        SecretEdits {
            password: None,
            nickserv: None,
        },
    );
    let profile = complete_profile();
    controller.save_profile(
        profile.clone(),
        SecretEdits {
            password: Some("hunter2".to_owned().into()),
            nickserv: None,
        },
    );

    let stored: Vec<_> = ["keep", NET]
        .into_iter()
        .map(|id| controller.profile(id).expect("profile").clone())
        .collect();
    let mut state = ConnectState::from_stored(&stored);
    assert!(state.select(NET));
    let populated = vec!["keep".to_owned(), NET.to_owned()];
    assert_eq!(
        state.remove_with_forget(&populated, |_| Status::Written),
        RemoveOutcome::Armed
    );

    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))
        .expect("chmod dir");
    let outcome = state.remove_with_forget(&populated, |id| controller.forget_network(id));
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
        .expect("restore dir mode");

    assert_eq!(outcome, RemoveOutcome::Refused);
    assert!(
        state.is_saved(NET),
        "sheet row must stay when forget_network returns AccessError"
    );
    assert_eq!(state.selected(), NET);
    assert_eq!(
        state.persistence_status(),
        "The settings file could not be written."
    );
    assert!(controller.profile(NET).is_some());
}

/// A malformed conf is FormatError: Remove must show Apply's read sentence.
#[test]
fn connect_remove_shows_format_error_when_conf_is_malformed() {
    let dir = TempDir::new("forget-format-sheet");
    let conf = dir.path().join("omairc.conf");
    let backing = Backing::Disk(conf.clone());
    let mut controller = Controller::with_backing(clock(), Box::new(NoTransports), backing);
    controller.credentials = Box::new(MemoryCredentialStore::default());
    let keep = Profile {
        host: "irc.keep.example".to_owned(),
        nick: "oak".to_owned(),
        secret_saved: true,
        connect_on_startup: true,
        ..Profile::new("keep")
    };
    controller.save_profile(
        keep,
        SecretEdits {
            password: None,
            nickserv: None,
        },
    );
    let profile = complete_profile();
    controller.save_profile(
        profile,
        SecretEdits {
            password: Some("hunter2".to_owned().into()),
            nickserv: None,
        },
    );
    std::fs::write(&conf, "not ini").expect("broken file");

    let stored: Vec<_> = ["keep", NET]
        .into_iter()
        .map(|id| controller.profile(id).expect("profile").clone())
        .collect();
    let mut state = ConnectState::from_stored(&stored);
    assert!(state.select(NET));
    let populated = vec!["keep".to_owned(), NET.to_owned()];
    assert_eq!(
        state.remove_with_forget(&populated, |_| Status::Written),
        RemoveOutcome::Armed
    );
    assert_eq!(
        state.remove_with_forget(&populated, |id| controller.forget_network(id)),
        RemoveOutcome::Refused
    );
    assert!(state.is_saved(NET));
    assert_eq!(
        state.persistence_status(),
        "The settings file could not be read."
    );
    assert!(controller.profile(NET).is_some());
}
