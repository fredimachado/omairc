//! The STS cases of `tui/internal/session/session_lifecycle_test.go` (Go), ported to the Rust
//! session. The Go tests read the in-memory store on the session; here the fixture's `sts` plays
//! that role. The last two tests cross the file: a policy learned on one run applies after a
//! restart, and networks on one manager share one cache.

use std::rc::Rc;
use std::time::Duration;

use chrono::{TimeZone, Utc};

use super::fixture::{plaintext_config, test_config, Fixture};
use crate::session::clock::{Clock, FakeClock};
use crate::session::config::SessionConfig;
use crate::session::events::{SessionEvent, SessionState};
use crate::session::loopback::LoopbackTransport;
use crate::session::session::Session;
use crate::session::sessionmanager::SessionManager;
use crate::session::sts::{MemoryStsStore, StsStore};
use crate::storage::stspolicies::IniStsStore;
use crate::util::testdir::TempDir;

const HOST: &str = "irc.example";

/// The TLS test config without autojoin, as the Go STS tests use.
fn tls_config() -> SessionConfig {
    let mut config = test_config();
    config.autojoin_channels.clear();
    config
}

/// How many `CAP LS` lines were written.
fn cap_ls_count(frames: &[String]) -> usize {
    frames
        .iter()
        .filter(|frame| frame.starts_with("CAP LS"))
        .count()
}

/// The text of every `sts` lifecycle entry, in order.
fn sts_texts(fx: &Fixture) -> Vec<String> {
    fx.events()
        .into_iter()
        .filter_map(|event| match event {
            SessionEvent::Lifecycle {
                kind: "sts", text, ..
            } => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn plaintext_sts_port_reconnects_with_tls_and_caches_on_secure_duration() {
    let fx = Fixture::new(plaintext_config());
    fx.session.start();
    fx.transport.complete_connect();
    assert!(
        !fx.transport.tls_requested(),
        "the first connect must be plaintext"
    );
    assert_eq!(fx.transport.connected_port(), 6667);

    fx.inject(
        ":server CAP omairc LS :sts=port=6697,duration=60 multi-prefix\r\n\
         :server 001 omairc :should-not-register-plaintext\r\n",
    );

    assert!(
        fx.transport.tls_requested(),
        "the STS upgrade must request TLS"
    );
    assert_eq!(fx.transport.connected_port(), 6697);
    assert_eq!(fx.session.port(), 6697);
    assert!(
        fx.session.tls_enabled(),
        "tls_enabled must be true after the upgrade"
    );
    assert!(fx.states().contains(&SessionState::StsUpgrading));
    assert!(
        !fx.wrote("CAP REQ :multi-prefix\r\n") && !fx.wrote("NICK omairc\r\n"),
        "the plaintext connection must not finish registering"
    );
    assert_eq!(
        sts_texts(&fx),
        vec!["Upgraded to TLS on port 6697".to_owned()],
        "the upgrade is logged once"
    );

    fx.transport.complete_connect();
    fx.inject(":server CAP omairc LS :sts=duration=60\r\n:server 001 omairc :Welcome\r\n");
    assert_eq!(
        cap_ls_count(&fx.frames()),
        2,
        "the secure connection negotiates again"
    );
    assert_eq!(fx.registered_count(), 1, "the secure connection registers");
    let cached = fx
        .sts
        .lookup(HOST, fx.clock.now())
        .expect("the STS policy must be cached after the secure connect");
    assert_eq!(cached.port, 6697);
}

#[test]
fn plaintext_sts_without_port_stays_plaintext() {
    let fx = Fixture::new(plaintext_config());
    fx.session.start();
    fx.transport.complete_connect();
    fx.inject(":server CAP omairc LS :sts=duration=60\r\n:server 001 omairc :Welcome\r\n");
    assert!(
        !fx.transport.tls_requested(),
        "an STS advertisement without a port must not upgrade"
    );
    assert_eq!(fx.transport.connected_port(), 6667);
    assert!(
        fx.wrote("NICK omairc\r\n"),
        "the session must register in plaintext"
    );
}

#[test]
fn plaintext_cap_new_sts_port_reconnects_with_tls() {
    let fx = Fixture::new(plaintext_config());
    fx.session.start();
    fx.transport.complete_connect();
    fx.inject(":server CAP omairc LS :multi-prefix\r\n");
    assert!(
        fx.wrote("CAP REQ :multi-prefix\r\n"),
        "multi-prefix must be requested"
    );

    fx.inject(":server CAP omairc NEW :sts=port=6697\r\n");
    assert!(
        fx.transport.tls_requested(),
        "a CAP NEW STS port must upgrade"
    );
    assert_eq!(fx.transport.connected_port(), 6697);
}

#[test]
fn tls_sts_duration_refreshes_cache_without_reconnect() {
    let fx = Fixture::new(tls_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :sts=duration=86400\r\n");

    assert_eq!(
        cap_ls_count(&fx.frames()),
        1,
        "no reconnect, so no second CAP LS"
    );
    assert!(fx.transport.tls_requested(), "TLS stays requested");
    assert_eq!(fx.transport.connected_port(), 6697);
    let cached = fx
        .sts
        .lookup(HOST, fx.clock.now())
        .expect("the STS duration must refresh the cache");
    assert_eq!((cached.port, cached.duration_seconds), (6697, 86400));
}

#[test]
fn tls_sts_duration_zero_clears_cache() {
    let fx = Fixture::new(tls_config());
    fx.sts.store(HOST, 6697, 60, fx.clock.now());

    fx.connect_tls();
    fx.inject(":server CAP omairc LS :sts=duration=0\r\n:server 001 omairc :Welcome\r\n");
    assert_eq!(
        fx.sts.lookup(HOST, fx.clock.now()),
        None,
        "a zero duration must clear the cache"
    );
}

#[test]
fn cached_host_opens_tls_when_profile_disables_it() {
    let fx = Fixture::new(plaintext_config());
    fx.sts.store(HOST, 6697, 60, fx.clock.now());

    fx.session.start();
    assert!(
        fx.transport.tls_requested(),
        "a cached policy must force TLS"
    );
    assert_eq!(fx.transport.connected_port(), 6697);
    assert!(fx.session.tls_enabled());
    assert_eq!(fx.session.port(), 6697);
}

#[test]
fn cap_del_sts_does_not_clear_cache() {
    let fx = Fixture::new(tls_config());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :sts=duration=86400\r\n\
         :server 001 omairc :Welcome\r\n\
         :server CAP omairc DEL :sts\r\n",
    );
    let cached = fx
        .sts
        .lookup(HOST, fx.clock.now())
        .expect("CAP DEL must not clear the STS cache");
    assert_eq!(cached.port, 6697);
}

#[test]
fn an_expired_cached_policy_does_not_upgrade_on_start() {
    let fx = Fixture::new(plaintext_config());
    fx.sts.store(HOST, 6697, 60, fx.clock.now());
    fx.clock.advance(Duration::from_secs(61));

    fx.session.start();
    assert!(
        !fx.transport.tls_requested(),
        "an expired policy must not force TLS"
    );
    assert_eq!(fx.transport.connected_port(), 6667);
}

#[test]
fn a_dropped_secure_connection_refreshes_the_expiry() {
    let fx = Fixture::new(tls_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :sts=duration=60\r\n");
    fx.clock.advance(Duration::from_secs(30));

    fx.transport.remote_close();

    let cached = fx
        .sts
        .lookup(HOST, fx.clock.now())
        .expect("the policy is still valid");
    assert_eq!(
        cached.expiry_epoch_seconds, 90,
        "30s in plus 60s of duration"
    );
}

#[test]
fn stopping_the_session_refreshes_the_expiry() {
    let fx = Fixture::new(tls_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :sts=duration=60\r\n");
    fx.clock.advance(Duration::from_secs(30));

    fx.session.stop();

    let cached = fx
        .sts
        .lookup(HOST, fx.clock.now())
        .expect("the policy is still valid");
    assert_eq!(cached.expiry_epoch_seconds, 90);
}

#[test]
fn networks_on_one_manager_share_the_policy_cache() {
    let manager = SessionManager::new(Rc::new(MemoryStsStore::default()));
    let clock = Rc::new(FakeClock::new(Utc.timestamp_opt(0, 0).single().unwrap()));
    let learner = Rc::new(LoopbackTransport::new());
    let learner_session = manager
        .create(
            SessionConfig {
                network_id: "learner".to_owned(),
                ..tls_config()
            },
            learner.clone(),
            clock.clone(),
        )
        .expect("create the learning network");
    learner_session.start();
    learner.complete_connect();
    learner.inject_bytes(b":server CAP omairc LS :sts=duration=60\r\n");

    let follower = Rc::new(LoopbackTransport::new());
    let follower_session = manager
        .create(
            SessionConfig {
                network_id: "follower".to_owned(),
                ..plaintext_config()
            },
            follower.clone(),
            clock,
        )
        .expect("create the plaintext network");
    follower_session.start();

    assert!(
        follower.tls_requested(),
        "the other network reuses the cached policy"
    );
    assert_eq!(follower.connected_port(), 6697);
}

#[test]
fn a_policy_learned_before_a_restart_applies_after_it() {
    let dir = TempDir::new("sts-restart");
    let path = dir.path().join("sts");
    let clock = Rc::new(FakeClock::new(Utc.timestamp_opt(0, 0).single().unwrap()));

    let first_transport = Rc::new(LoopbackTransport::new());
    let first = Session::new(
        tls_config(),
        first_transport.clone(),
        clock.clone(),
        Rc::new(IniStsStore::new(path.clone())),
    );
    first.start();
    first_transport.complete_connect();
    first_transport.inject_bytes(b":server CAP omairc LS :sts=duration=60\r\n");
    drop(first);

    let second_transport = Rc::new(LoopbackTransport::new());
    let second = Session::new(
        plaintext_config(),
        second_transport.clone(),
        clock,
        Rc::new(IniStsStore::new(path)),
    );
    second.start();
    assert!(
        second_transport.tls_requested(),
        "the saved policy forces TLS after a restart"
    );
    assert_eq!(second_transport.connected_port(), 6697);
}

#[test]
fn the_sts_upgrading_state_names_itself_like_the_cpp_enum() {
    assert_eq!(SessionState::StsUpgrading.name(), "StsUpgrading");
}
