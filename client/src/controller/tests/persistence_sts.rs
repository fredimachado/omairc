//! Quit refreshes the STS policy a live connection holds, so a relaunch keeps it.

use std::rc::Rc;
use std::time::Duration;

use chrono::{TimeZone, Utc};

use crate::controller::controller::Controller;
use crate::controller::tests::persistence::{disk, feed, libera, save, Loopbacks, LIBERA};
use crate::session::clock::{Clock, FakeClock};
use crate::storage::credentialstore::MemoryCredentialStore;
use crate::util::testdir::TempDir;

#[test]
fn shutting_down_refreshes_the_sts_expiry_a_live_connection_holds() {
    let dir = TempDir::new("quit-sts-refresh");
    let backing = disk(&dir);
    let loopbacks = Loopbacks::default();
    let clock = Rc::new(FakeClock::new(
        Utc.with_ymd_and_hms(2026, 10, 21, 9, 0, 0).unwrap(),
    ));
    let mut controller =
        Controller::with_backing(clock.clone(), Box::new(loopbacks.clone()), backing.clone());
    controller.credentials = Box::new(MemoryCredentialStore::default());
    controller.load_stored();
    save(&mut controller, libera(LIBERA));
    assert!(controller.start(LIBERA));
    feed(
        &mut controller,
        &loopbacks,
        LIBERA,
        ":irc.libera.chat CAP fred LS :sts=duration=60",
    );
    let learned = backing
        .sts_store()
        .lookup("irc.libera.chat", clock.now())
        .expect("the TLS connection learns the policy");
    assert_eq!(learned.expiry_epoch_seconds, clock.now().timestamp() + 60);

    clock.advance(Duration::from_secs(30));
    controller.shutdown();

    let refreshed = backing
        .sts_store()
        .lookup("irc.libera.chat", clock.now())
        .expect("the policy is still valid at quit");
    assert_eq!(
        refreshed.expiry_epoch_seconds,
        clock.now().timestamp() + 60,
        "quit moves the expiry from connect time to quit time"
    );
}
