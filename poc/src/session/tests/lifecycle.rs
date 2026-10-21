//! Port of the ping, registration-refusal and nick-collision cases of
//! `tui/internal/session/session_lifecycle_test.go`.
//!
//! The Go tests also assert on Status text (`textFor`, `anyFieldContains`). Status
//! is Phase 3's recorder, so those assertions are left out here; each test keeps
//! its state, error, reconnect and frame checks.

use std::time::Duration;

use super::fixture::{assert_frames, test_config, Fixture};
use crate::session::events::{ErrorKind, SessionEvent, SessionState};
use crate::session::transport::Transport;

fn advance_ms(fx: &Fixture, ms: u64) {
    fx.clock.advance(Duration::from_millis(ms));
}

#[test]
fn answers_ping_immediately() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject("PING :server-token\r\n");
    assert_eq!(fx.last_frame(), "PONG :server-token\r\n");
    assert_eq!(
        fx.clock.pending(),
        0,
        "no watchdog is armed before registration"
    );
}

#[test]
fn silent_socket_after_welcome_sends_client_ping() {
    let fx = Fixture::new(test_config());
    fx.register_with_welcome();
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert!(fx.clock.pending() > 0, "the ping watchdog must be armed");

    fx.inject(":bob!u@h PRIVMSG #omarchy :hi\r\n");
    let before = fx.frames().len();
    advance_ms(&fx, 60000);
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert_frames(&fx.written_since(before), &["PING :omairc-watchdog\r\n"]);
    assert!(fx.clock.pending() > 0, "the probe must re-arm the watchdog");
}

#[test]
fn unanswered_client_ping_reconnects() {
    let fx = Fixture::new(test_config());
    fx.register_with_welcome();
    advance_ms(&fx, 60000);
    assert_eq!(fx.last_frame(), "PING :omairc-watchdog\r\n");

    advance_ms(&fx, 60000);
    let (kind, message) = fx.errors().last().cloned().expect("an error");
    assert_eq!(kind, ErrorKind::Network);
    assert_eq!(message, "Ping timeout");
    assert_eq!(fx.session.state(), SessionState::Reconnecting);
    assert_eq!(fx.reconnects().len(), 1);
}

#[test]
fn matching_pong_keeps_session_registered() {
    let fx = Fixture::new(test_config());
    fx.register_with_welcome();
    advance_ms(&fx, 60000);
    fx.inject(":server PONG irc.example :omairc-watchdog\r\n");
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert!(fx.errors().is_empty());
    assert!(fx.clock.pending() > 0, "the watchdog must be re-armed");

    let before = fx.frames().len();
    advance_ms(&fx, 60000);
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert_frames(&fx.written_since(before), &["PING :omairc-watchdog\r\n"]);
    assert!(fx.errors().is_empty());
}

#[test]
fn answers_server_ping_after_welcome() {
    let fx = Fixture::new(test_config());
    fx.register_with_welcome();
    fx.inject("PING :server-token\r\n");
    assert_eq!(fx.last_frame(), "PONG :server-token\r\n");
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert!(
        !fx.wrote("PING :omairc-watchdog\r\n"),
        "a server ping must not trigger the watchdog"
    );
}

#[test]
fn registration_refusal_fails_visibly() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server 432 * omairc :Erroneous nickname\r\n");
    assert_eq!(fx.session.state(), SessionState::Failed);
    let errors = fx.errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].0, ErrorKind::Registration);
    assert_eq!(errors[0].1, "Erroneous nickname");
    assert!(fx.reconnects().is_empty());
}

#[test]
fn registration_refusal_without_sentence_keeps_numeric() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server 432 * omairc :\r\n");
    assert_eq!(fx.session.state(), SessionState::Failed);
    assert_eq!(
        fx.error_messages(),
        vec!["IRC registration was refused (432)"]
    );
}

#[test]
fn registration_refusal_without_colon_keeps_numeric() {
    for line in [
        ":server 432 * omairc\r\n",
        ":server 432 * omairc :\r\n",
        ":server 432 * omairc :IDENTIFY hunter2\r\n",
    ] {
        let fx = Fixture::new(test_config());
        fx.connect_tls();
        fx.inject(line);
        assert_eq!(fx.session.state(), SessionState::Failed, "{line:?}");
        let messages = fx.error_messages();
        assert_eq!(
            messages,
            vec!["IRC registration was refused (432)"],
            "{line:?}"
        );
        assert!(!messages[0].contains("hunter2"), "{line:?}");
    }
}

#[test]
fn registration_refusal_keeps_one_word_sentence() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server 432 * omairc :Unavailable\r\n");
    assert_eq!(fx.session.state(), SessionState::Failed);
    assert_eq!(fx.error_messages(), vec!["Unavailable"]);
}

#[test]
fn registration_refusal_drops_secret_sentence() {
    for sentence in [
        "IDENTIFY hunter2",
        "Closing Link: PASS hunter2",
        "PAS hunter2",
        "PASS:hunter2",
    ] {
        let fx = Fixture::new(test_config());
        fx.connect_tls();
        fx.inject(&format!(":server 432 * omairc :{sentence}\r\n"));
        assert_eq!(fx.session.state(), SessionState::Failed, "{sentence}");
        assert_eq!(
            fx.error_messages(),
            vec!["IRC registration was refused (432)"],
            "{sentence}"
        );
        assert!(fx.reconnects().is_empty(), "{sentence}");
    }
}

#[test]
fn nick_in_use_before_welcome_retries_then_registers() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :multi-prefix\r\n");
    assert_eq!(fx.session.state(), SessionState::Registering);
    assert!(fx.wrote("NICK omairc\r\n"));

    fx.inject(":server 433 * omairc :Nickname in use\r\n");
    assert_eq!(fx.session.state(), SessionState::Registering);
    assert!(fx.errors().is_empty());
    assert_eq!(fx.last_frame(), "NICK omairc_\r\n");
    assert_eq!(fx.session.nick(), "omairc_");

    fx.inject(":server 433 * omairc_ :Nickname in use\r\n");
    assert_eq!(fx.session.state(), SessionState::Registering);
    assert_eq!(fx.last_frame(), "NICK omairc2\r\n");
    assert_eq!(fx.session.nick(), "omairc2");

    fx.inject(":server 001 omairc2 :Welcome\r\n");
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert_eq!(fx.session.nick(), "omairc2");
    assert!(fx.errors().is_empty());
}

#[test]
fn nick_in_use_fallbacks_exhausted_fails() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :multi-prefix\r\n");
    fx.inject(
        ":server 433 * omairc :Nickname in use\r\n\
         :server 433 * omairc_ :Nickname in use\r\n\
         :server 433 * omairc2 :Nickname in use\r\n",
    );
    assert_eq!(fx.session.state(), SessionState::Failed);
    let errors = fx.errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].0, ErrorKind::Registration);
    assert_eq!(errors[0].1, "Nickname in use");
    assert_eq!(fx.last_frame(), "NICK omairc2\r\n");
    assert!(fx.reconnects().is_empty());
}

#[test]
fn nick_in_use_after_welcome_keeps_session() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :multi-prefix\r\n:server 001 omairc :Welcome\r\n");
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert!(fx.errors().is_empty());

    fx.inject(":server 433 omairc othernick :Nickname is already in use\r\n");
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert_eq!(
        fx.transport.state(),
        crate::session::transport::ConnectionState::Encrypted
    );
    assert!(fx.errors().is_empty());
    let messages: Vec<String> = fx.messages().iter().map(|m| m.command.clone()).collect();
    assert_eq!(messages.last().map(String::as_str), Some("433"));
}

#[test]
fn nick_in_use_fallback_drops_secret_status_sentence() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :multi-prefix\r\n:server 433 * omairc :IDENTIFY hunter2\r\n");
    assert_eq!(fx.session.state(), SessionState::Registering);
    assert!(fx.errors().is_empty());
    assert_eq!(fx.last_frame(), "NICK omairc_\r\n");
}

#[test]
fn nick_in_use_after_welcome_drops_secret_status_sentence() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :multi-prefix\r\n\
         :server 001 omairc :Welcome\r\n\
         :server 433 omairc othernick :Closing Link: PASS hunter2\r\n",
    );
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert!(fx.errors().is_empty());
}

#[test]
fn unavailable_resource_after_welcome_keeps_session() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :multi-prefix\r\n:server 001 omairc :Welcome\r\n");
    assert_eq!(fx.session.state(), SessionState::Registered);
    fx.inject(":server 437 omairc othernick :Nick/channel is temporarily unavailable\r\n");
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert_eq!(
        fx.transport.state(),
        crate::session::transport::ConnectionState::Encrypted
    );
    assert!(fx.errors().is_empty());
}

#[test]
fn unavailable_resource_before_welcome_fails() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server 437 * omairc :Nick/channel is temporarily unavailable\r\n");
    assert_eq!(fx.session.state(), SessionState::Failed);
    let errors = fx.errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].0, ErrorKind::Registration);
    assert_eq!(errors[0].1, "Nick/channel is temporarily unavailable");
    assert!(fx.reconnects().is_empty());
}

#[test]
fn registration_error_before_welcome_shows_reason_and_stops() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    let written = fx.frames().len();
    fx.inject(":server ERROR :Closing Link: 127.0.0.1 (Invalid nickname)\r\n");
    assert_eq!(fx.session.state(), SessionState::Failed);
    let errors = fx.errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].0, ErrorKind::Registration);
    assert_eq!(errors[0].1, "Closing Link: 127.0.0.1 (Invalid nickname)");
    assert!(fx.reconnects().is_empty());
    assert_eq!(fx.frames().len(), written, "no further nick");
}

#[test]
fn registration_error_before_welcome_drops_secret_sentence() {
    for sentence in [
        "IDENTIFY hunter2",
        "Closing Link: PASS hunter2",
        "PAS hunter2",
        "PASS:hunter2",
    ] {
        let fx = Fixture::new(test_config());
        fx.connect_tls();
        fx.inject(&format!(":server ERROR :{sentence}\r\n"));
        assert_eq!(fx.session.state(), SessionState::Failed, "{sentence}");
        let errors = fx.errors();
        assert_eq!(errors.len(), 1, "{sentence}");
        assert_eq!(errors[0].0, ErrorKind::Registration, "{sentence}");
        assert_eq!(errors[0].1, "IRC server reported an error", "{sentence}");
        assert!(fx.reconnects().is_empty(), "{sentence}");
    }
}

#[test]
fn registration_error_after_welcome_reconnects_with_reason() {
    let fx = Fixture::new(test_config());
    fx.register_with_welcome();
    fx.inject(":server ERROR :Closing Link: 127.0.0.1 (Quit)\r\n");
    assert_eq!(fx.session.state(), SessionState::Reconnecting);
    let errors = fx.errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].0, ErrorKind::Network);
    assert_eq!(errors[0].1, "Closing Link: 127.0.0.1 (Quit)");
    assert_eq!(fx.reconnects().len(), 1);
}

#[test]
fn ping_reconnect_event_carries_delay_and_attempt() {
    let fx = Fixture::new(test_config());
    fx.register_with_welcome();
    fx.inject(":server ERROR :bye\r\n");
    let scheduled = fx
        .events()
        .into_iter()
        .find(|event| matches!(event, SessionEvent::ReconnectScheduled { .. }));
    assert!(matches!(
        scheduled,
        Some(SessionEvent::ReconnectScheduled {
            delay_ms: 250,
            attempt: 1,
            ..
        })
    ));
}
