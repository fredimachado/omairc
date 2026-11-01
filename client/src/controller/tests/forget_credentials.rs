//! Forget must not drop profile or secrets when the combined settings write is
//! blocked. Shared with the TUI `RemoveSelected` Sync-failure contract.

use std::rc::Rc;

use chrono::{TimeZone, Utc};

use crate::controller::controller::Controller;
use crate::controller::persistence::SecretEdits;
use crate::controller::transports::NoTransports;
use crate::session::clock::{Clock, FakeClock};
use crate::storage::credentialstore::{
    read_slot_sync, MemoryCredentialStore, PURPOSE_PASSWORD,
};
use crate::storage::profilestore::Profile;
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

    assert!(!controller.forget_network(NET));
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
    assert!(
        conf.is_file(),
        "preflight needs a readable conf after save"
    );
    // Directory not writable → atomic rename fails; the conf file itself still
    // probes as writable so preflight_remove returns Written.
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))
        .expect("chmod dir");

    let forgot = controller.forget_network(NET);

    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
        .expect("restore dir mode");

    assert!(!forgot, "forget must return false when Sync fails after preflight");
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
