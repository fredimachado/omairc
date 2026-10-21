//! Port of the reconnect, quit, manager and malformed-input cases of
//! `tui/internal/session/session_lifecycle_test.go`.

use std::rc::Rc;

use chrono::TimeZone;
use std::time::Duration;

use super::fixture::{test_config, Fixture};
use crate::session::clock::FakeClock;
use crate::session::events::{ErrorKind, SessionState};
use crate::session::loopback::LoopbackTransport;
use crate::session::sessionmanager::{SessionManager, SessionManagerError};
use crate::session::transport::ConnectionState;
use crate::session::transport::Transport;

fn delays(fx: &Fixture) -> Vec<u64> {
    fx.reconnects()
        .into_iter()
        .map(|(delay, _)| delay)
        .collect()
}

fn clock_at_epoch() -> Rc<FakeClock> {
    Rc::new(FakeClock::new(
        chrono::Utc.timestamp_opt(0, 0).single().unwrap(),
    ))
}

#[test]
fn connection_timeout_schedules_reconnect() {
    let fx = Fixture::new(test_config());
    fx.session.start();
    fx.transport.timeout_connect();
    assert_eq!(fx.session.state(), SessionState::Reconnecting);
    assert_eq!(fx.reconnects(), vec![(250, 1)]);
    assert_eq!(fx.errors()[0].0, ErrorKind::Network);
}

#[test]
fn remote_close_schedules_reconnect() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.transport.remote_close();
    assert_eq!(fx.session.state(), SessionState::Reconnecting);
    assert_eq!(fx.reconnects(), vec![(250, 1)]);
}

/// The reachability signal of Go is `notify_reachable` here: the scheduled
/// reconnect starts without waiting out the backoff.
#[test]
fn reachability_starts_reconnect_without_waiting() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.transport.remote_close();
    assert_eq!(fx.session.state(), SessionState::Reconnecting);
    assert_eq!(fx.reconnects(), vec![(250, 1)]);
    assert_eq!(fx.session.reconnect_attempt(), 1);

    fx.session.notify_reachable();
    assert_eq!(fx.session.state(), SessionState::Connecting);
    assert_eq!(fx.transport.state(), ConnectionState::Connecting);
    assert_eq!(fx.reconnects().len(), 1);
    assert_eq!(fx.session.reconnect_attempt(), 1);
}

#[test]
fn reachability_ignored_unless_reconnecting() {
    let fx = Fixture::new(test_config());
    fx.register_with_welcome();
    assert_eq!(fx.session.state(), SessionState::Registered);
    fx.session.notify_reachable();
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert_eq!(fx.transport.state(), ConnectionState::Encrypted);

    fx.transport.remote_close();
    fx.session.stop();
    fx.session.notify_reachable();
    assert_eq!(fx.session.state(), SessionState::Idle);
    assert_eq!(fx.transport.state(), ConnectionState::Disconnected);
}

#[test]
fn reconnect_can_be_cancelled() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.transport.remote_close();
    assert!(
        fx.clock.pending() > 0,
        "the reconnect timer must be pending"
    );
    fx.session.stop();
    assert_eq!(fx.session.state(), SessionState::Idle);
    assert_eq!(
        fx.clock.pending(),
        0,
        "the reconnect timer must be cancelled"
    );
    let written = fx.frames().len();
    fx.clock.advance(Duration::from_millis(250));
    assert_eq!(
        fx.frames().len(),
        written,
        "a cancelled reconnect must not write frames"
    );
    assert_eq!(fx.transport.state(), ConnectionState::Disconnected);
}

#[test]
fn reconnect_delay_is_bounded_exponential() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.transport.remote_close();
    assert_eq!(delays(&fx), vec![250]);

    fx.fire_reconnect();
    assert_eq!(fx.transport.state(), ConnectionState::Connecting);
    fx.transport.complete_connect();
    fx.transport.remote_close();
    assert_eq!(delays(&fx), vec![250, 500]);

    fx.fire_reconnect();
    fx.transport.complete_connect();
    fx.transport.remote_close();
    assert_eq!(delays(&fx), vec![250, 500, 1000]);

    // The third reconnect is the cap of the test config. Stop, as the handler of
    // the Go test does after its reconnect limit.
    fx.session.stop();
    assert_eq!(fx.session.state(), SessionState::Idle);
    assert_eq!(fx.clock.pending(), 0, "the session must have stopped");
}

#[test]
fn reconnect_keeps_retrying_until_stop() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.transport.remote_close();
    for _ in 0..3 {
        fx.fire_reconnect();
        fx.transport.complete_connect();
        fx.transport.remote_close();
    }
    assert_eq!(fx.session.state(), SessionState::Reconnecting);
    let all = delays(&fx);
    assert!(all.len() > 3, "delays = {all:?}, want more than 3");
    assert_eq!(*all.last().unwrap(), 1000);
    fx.session.stop();
    assert_eq!(fx.session.state(), SessionState::Idle);
    assert_eq!(fx.clock.pending(), 0);
}

#[test]
fn quit_stops_reconnect_wait() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.transport.remote_close();
    assert_eq!(fx.session.state(), SessionState::Reconnecting);
    assert!(fx.clock.pending() > 0);
    assert!(fx.session.quit(""), "quit must succeed");
    assert_eq!(fx.session.state(), SessionState::Idle);
    assert_eq!(fx.clock.pending(), 0);
    let written = fx.frames().len();
    fx.clock.advance(Duration::from_millis(250));
    assert_eq!(fx.session.state(), SessionState::Idle);
    assert_eq!(fx.frames().len(), written);
    assert_eq!(fx.transport.state(), ConnectionState::Disconnected);
}

#[test]
fn retryable_network_error_is_emitted_once_until_welcome() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.transport.remote_close();
    for _ in 0..2 {
        fx.fire_reconnect();
        fx.transport.complete_connect();
        fx.transport.remote_close();
    }
    let errors = fx.errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].0, ErrorKind::Network);

    fx.fire_reconnect();
    fx.transport.complete_connect();
    fx.inject(":server CAP omairc LS :multi-prefix\r\n:server 001 omairc :Welcome\r\n");
    assert_eq!(fx.session.state(), SessionState::Registered);

    fx.transport.remote_close();
    let errors = fx.errors();
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert_eq!(errors[1].0, ErrorKind::Network);
}

#[test]
fn retryable_errors_stay_deduped_across_kinds_until_welcome() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.transport.remote_close();
    let errors = fx.errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].0, ErrorKind::Network);

    fx.fire_reconnect();
    fx.transport.fail_connect("TLS handshake failed");
    let errors = fx.errors();
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[1].0, ErrorKind::Tls);
    assert_eq!(fx.session.state(), SessionState::Reconnecting);

    fx.fire_reconnect();
    fx.transport.complete_connect();
    fx.transport.remote_close();
    let errors = fx.errors();
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert_eq!(
        (errors[0].0, errors[1].0),
        (ErrorKind::Network, ErrorKind::Tls)
    );
}

#[test]
fn quit_rejects_invalid_reason_while_registered() {
    let fx = Fixture::new(test_config());
    fx.register_with_welcome();
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert!(
        !fx.session.quit("bad\nreason"),
        "quit with an invalid reason must fail"
    );
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert!(!fx.wrote("QUIT :bad\nreason\r\n"));

    fx.transport.remote_close();
    assert_eq!(fx.session.state(), SessionState::Reconnecting);
    assert!(fx.session.quit(""), "quit while reconnecting must succeed");
    assert_eq!(fx.session.state(), SessionState::Idle);
}

#[test]
fn destruction_while_connecting_is_safe() {
    let transport = Rc::new(LoopbackTransport::new());
    let session =
        crate::session::session::Session::new(test_config(), transport.clone(), clock_at_epoch());
    session.start();
    assert_eq!(transport.state(), ConnectionState::Connecting);
    drop(session);
    // Injecting into the dropped session's transport must not reach freed state.
    transport.complete_connect();
    transport.inject_bytes(b"PING :x\r\n");
}

#[test]
fn manager_starts_two_live_networks() {
    let manager = SessionManager::new();
    let first_transport = Rc::new(LoopbackTransport::new());
    let second_transport = Rc::new(LoopbackTransport::new());
    let first = manager
        .create(
            crate::session::config::SessionConfig {
                network_id: "network-a".to_owned(),
                ..test_config()
            },
            first_transport.clone(),
            clock_at_epoch(),
        )
        .expect("create network-a");
    let second = manager
        .create(
            crate::session::config::SessionConfig {
                network_id: "network-b".to_owned(),
                ..test_config()
            },
            second_transport.clone(),
            clock_at_epoch(),
        )
        .expect("create network-b");
    assert!(manager.find("network-a").is_some());
    assert!(manager.activate("network-a"));
    assert!(manager.activate("network-b"));
    assert_eq!(first.state(), SessionState::Connecting);
    assert_eq!(second.state(), SessionState::Connecting);
    assert_eq!(first_transport.state(), ConnectionState::Connecting);
    assert_eq!(second_transport.state(), ConnectionState::Connecting);

    manager.stop("network-a");
    assert_eq!(second.state(), SessionState::Connecting);
}

#[test]
fn manager_create_stays_add_only() {
    let manager = SessionManager::new();
    let config = || crate::session::config::SessionConfig {
        network_id: "network-a".to_owned(),
        ..test_config()
    };
    let first = manager
        .create(
            config(),
            Rc::new(LoopbackTransport::new()),
            clock_at_epoch(),
        )
        .expect("create");
    let duplicate = manager.create(
        config(),
        Rc::new(LoopbackTransport::new()),
        clock_at_epoch(),
    );
    assert_eq!(
        duplicate.err(),
        Some(SessionManagerError::DuplicateNetworkId(
            "network-a".to_owned()
        ))
    );
    assert_eq!(
        manager.find("network-a").unwrap().network_id(),
        first.network_id()
    );
}

#[test]
fn manager_discard_unregisters_immediately() {
    let manager = SessionManager::new();
    let config = || crate::session::config::SessionConfig {
        network_id: "network-a".to_owned(),
        ..test_config()
    };
    let session = manager
        .create(
            config(),
            Rc::new(LoopbackTransport::new()),
            clock_at_epoch(),
        )
        .expect("create");
    assert!(manager.activate("network-a"));
    manager.discard("network-a");
    assert!(
        manager.find("network-a").is_none(),
        "discard must unregister immediately"
    );
    manager.discard("network-a");
    let replacement = manager
        .create(
            config(),
            Rc::new(LoopbackTransport::new()),
            clock_at_epoch(),
        )
        .expect("replacement");
    assert!(manager.find("network-a").is_some());
    assert_eq!(session.state(), SessionState::Idle);
    assert_eq!(replacement.state(), SessionState::Idle);
}

#[test]
fn malformed_input_surfaces_protocol_error() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject("BAD\x00FRAME\r\n");
    let errors = fx.errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].0, ErrorKind::Protocol);
    let message = errors[0].1.clone();
    assert!(
        message.contains("invalid character")
            && message.contains("Preview:")
            && message.contains("BAD"),
        "error = {message:?}"
    );
    assert_eq!(fx.session.state(), SessionState::CapLs);

    let check = |index: usize, raw: &[u8], want_substring: &str, want_redacted: &str| {
        fx.transport.inject_bytes(raw);
        let errors = fx.errors();
        assert_eq!(errors.len(), index, "errors after {raw:?}");
        let got = &errors[index - 1].1;
        assert!(
            got.contains(want_substring),
            "error = {got:?}, want {want_substring:?}"
        );
        if !want_redacted.is_empty() {
            assert!(
                got.contains(want_redacted),
                "error = {got:?}, want {want_redacted:?}"
            );
        }
        for secret in ["hunter2", "s3cret", "my_nick"] {
            assert!(!got.contains(secret), "error = {got:?} leaked {secret}");
        }
    };

    check(2, b"\x00PASS hunter2\r\n", "invalid character", "PASS ***");
    check(3, b"PASS\x00hunter2\r\n", "invalid character", "PASS ***");
    check(4, b"P\x00ASS hunter2\r\n", "invalid character", "PASS ***");
    check(5, b"\x01PASS hunter2\r\n", "invalid command", "PASS ***");
    check(6, b"PASS\x7F hunter2\r\n", "invalid command", "PASS ***");
    check(7, b"P@SS hunter2\r\n", "invalid command", "PASS ***");
    check(8, b"PA@SS hunter2\r\n", "invalid command", "PASS ***");
    check(
        9,
        b"32@4 omairc #omarchy +k s3cret\r\n",
        "invalid command",
        "+k ***",
    );
    check(
        10,
        b"PRIVMSG nickserv :identif\x00 my_nick s3cret\r\n",
        "invalid character",
        "IDENTIFY ***",
    );
    check(
        11,
        b"P\x00ASS\x00hunter2\r\n",
        "invalid character",
        "PASS ***",
    );
    check(
        12,
        b"@bad tag=foo PRIVMSG nickserv :identify my_nick s3cret\r\n",
        "invalid command",
        "IDENTIFY ***",
    );
}

#[test]
fn overlong_frame_logs_preview_without_secrets() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    let max = crate::irc::message::MAX_INBOUND_CLASSIC_FRAME_BYTES;

    let mut overlong = b"PING :".to_vec();
    overlong.extend(std::iter::repeat_n(b'z', max - overlong.len() - 1));
    overlong.extend_from_slice(b"\r\n");
    fx.transport.inject_bytes(&overlong);

    let errors = fx.errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    let message = &errors[0].1;
    assert!(message.contains("too many bytes"), "{message:?}");
    assert!(
        message.contains(&format!("{} bytes", max - 1)),
        "{message:?}"
    );
    assert!(message.contains("Preview: PING :"), "{message:?}");

    let check_overlong = |index: usize, prefix: &str, redacted: &str, secret: &str| {
        let mut frame = prefix.as_bytes().to_vec();
        frame.extend(std::iter::repeat_n(b'x', max - frame.len() - 1));
        frame.extend_from_slice(b"\r\nPING :ok\r\n");
        fx.transport.inject_bytes(&frame);
        let errors = fx.errors();
        assert_eq!(errors.len(), index, "{prefix:?}");
        let got = &errors[index - 1].1;
        assert!(got.contains("too many bytes"), "{got:?}");
        assert!(got.contains(redacted), "{got:?} must contain {redacted:?}");
        assert!(!got.contains(secret), "{got:?} leaked {secret:?}");
    };
    check_overlong(2, "PASS hunter2 ", "PASS ***", "hunter2");
    check_overlong(3, "JOIN #secret hunter2 ", "JOIN #secret ***", "hunter2");
    check_overlong(
        4,
        "PRIVMSG nickserv :identify my_nick s3cret ",
        "PRIVMSG nickserv :IDENTIFY ***",
        "s3cret",
    );
    check_overlong(
        5,
        "MODE #omarchy +k s3cret ",
        "MODE #omarchy +k ***",
        "s3cret",
    );
    check_overlong(6, "PASS\thunter2 ", "PASS ***", "hunter2");
    check_overlong(
        7,
        "PRIVMSG nickserv :\x01identify my_nick s3cret\x01 ",
        "PRIVMSG nickserv :IDENTIFY ***",
        "s3cret",
    );
}
