//! Port of `tui/internal/session/typing_test.go`: the outbound typing gate, its
//! per-target pace, and the session's `SendTyping`.

use std::time::Duration;

use chrono::{DateTime, TimeDelta, TimeZone, Utc};

use super::batch::history_config;
use super::fixture::Fixture;
use crate::session::typing::{
    typing_tagmsg, TypingPhase, TypingPublisher, TYPING_SEND_INTERVAL_MS,
};

fn now() -> DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000, 0).single().unwrap()
}

fn plus_ms(ms: i64) -> DateTime<Utc> {
    now() + TimeDelta::milliseconds(ms)
}

fn publisher_was_publishing(publisher: &TypingPublisher, target: &str) -> bool {
    publisher
        .targets
        .get(target)
        .is_some_and(|clock| clock.last_phase == TypingPhase::Active)
}

fn with_typing_registration(fx: &Fixture) {
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :message-tags\r\n\
         :server CAP omairc ACK :message-tags\r\n\
         :server 001 omairc :Welcome\r\n",
    );
}

#[test]
fn typing_publisher_throttles_per_target() {
    let mut publisher = TypingPublisher::default();
    assert!(
        publisher.should_send("#omarchy", TypingPhase::Active, now()),
        "the first active hint must be allowed"
    );
    publisher.record_sent("#omarchy", TypingPhase::Active, now());
    assert!(publisher_was_publishing(&publisher, "#omarchy"));
    assert!(
        !publisher.should_send("#omarchy", TypingPhase::Active, plus_ms(2999)),
        "a hint inside the interval must be throttled"
    );
    assert!(
        publisher.should_send("#omarchy", TypingPhase::Active, plus_ms(3000)),
        "a hint at the interval must be allowed"
    );
    assert!(
        publisher.should_send("#desktop", TypingPhase::Active, plus_ms(1000)),
        "a fresh target must not be throttled"
    );
    assert!(
        !publisher.should_send("#omarchy", TypingPhase::Done, plus_ms(1000)),
        "done inside the interval must be throttled"
    );
    assert!(
        publisher.should_send("#omarchy", TypingPhase::Done, plus_ms(3000)),
        "done at the interval must be allowed"
    );
}

#[test]
fn typing_publisher_suppresses_done_after_chat() {
    let mut publisher = TypingPublisher::default();
    publisher.record_sent("#omarchy", TypingPhase::Active, now());
    publisher.note_message_sent("#omarchy");
    assert!(
        !publisher.should_send("#omarchy", TypingPhase::Done, plus_ms(4000)),
        "done must be suppressed after a chat message"
    );
    assert!(publisher_was_publishing(&publisher, "#omarchy"));
    assert!(
        publisher.should_send("#omarchy", TypingPhase::Active, plus_ms(4000)),
        "an active hint must still be allowed"
    );
    publisher.record_sent("#omarchy", TypingPhase::Active, plus_ms(4000));
    assert!(
        publisher.should_send("#omarchy", TypingPhase::Done, plus_ms(7000)),
        "done must be allowed after a fresh active hint"
    );
    publisher.reset();
    assert!(
        !publisher_was_publishing(&publisher, "#omarchy"),
        "reset must clear the target"
    );
    assert!(
        publisher.should_send("#omarchy", TypingPhase::Active, now()),
        "a reset target must accept an active hint"
    );
}

#[test]
fn typing_publisher_refuses_paused_and_unknown_targets() {
    let publisher = TypingPublisher::default();
    assert!(
        !publisher.should_send("#omarchy", TypingPhase::Paused, now()),
        "paused must never be sent"
    );
    assert!(
        !publisher.should_send("#omarchy", TypingPhase::Done, now()),
        "done for an unknown target must not be sent"
    );
    assert_eq!(
        typing_tagmsg("#omarchy", TypingPhase::Active).as_deref(),
        Some("@+typing=active TAGMSG #omarchy\r\n")
    );
    assert_eq!(typing_tagmsg("bad target", TypingPhase::Active), None);
    assert_eq!(typing_tagmsg("", TypingPhase::Done), None);
}

#[test]
fn session_send_typing_requires_registered_and_message_tags() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    assert!(
        !fx.session.send_typing("#omarchy", TypingPhase::Active),
        "typing must be refused before registration"
    );
    fx.inject(":server CAP omairc LS :multi-prefix\r\n:server 001 omairc :Welcome\r\n");
    assert!(
        !fx.session.send_typing("#omarchy", TypingPhase::Active),
        "typing must be refused without message-tags"
    );
    assert!(!fx.frames().iter().any(|frame| frame.contains("TAGMSG")));
}

#[test]
fn session_send_typing_writes_tagmsg_when_registered() {
    let fx = Fixture::new(history_config());
    with_typing_registration(&fx);
    assert!(fx.session.send_typing("#omarchy", TypingPhase::Active));
    assert_eq!(fx.last_frame(), "@+typing=active TAGMSG #omarchy\r\n");
}

#[test]
fn session_send_typing_throttles_per_target() {
    let fx = Fixture::new(history_config());
    with_typing_registration(&fx);
    assert!(
        fx.session.send_typing("#omarchy", TypingPhase::Active),
        "the first active hint must be sent"
    );
    assert!(
        !fx.session.send_typing("#omarchy", TypingPhase::Paused),
        "paused must never be sent"
    );
    assert!(
        !fx.session.send_typing("#omarchy", TypingPhase::Active),
        "a hint inside the interval must be throttled"
    );
    assert!(
        fx.session.send_typing("#desktop", TypingPhase::Active),
        "a fresh target must not be throttled"
    );
    fx.clock
        .advance(Duration::from_millis(TYPING_SEND_INTERVAL_MS as u64));
    assert!(
        fx.session.send_typing("#omarchy", TypingPhase::Done),
        "done at the interval must be sent"
    );
}

#[test]
fn session_send_typing_suppresses_done_after_chat() {
    let fx = Fixture::new(history_config());
    with_typing_registration(&fx);
    assert!(
        fx.session.send_typing("#omarchy", TypingPhase::Active),
        "the active hint must be sent"
    );
    assert!(
        fx.session.send_privmsg("#omarchy", "hi"),
        "the message must be sent"
    );
    assert!(
        !fx.session.send_typing("#omarchy", TypingPhase::Done),
        "done must be suppressed right after a chat message"
    );
    fx.clock
        .advance(Duration::from_millis(TYPING_SEND_INTERVAL_MS as u64));
    assert!(
        !fx.session.send_typing("#omarchy", TypingPhase::Done),
        "done must stay suppressed until a fresh active hint"
    );
    assert!(
        fx.session.send_typing("#omarchy", TypingPhase::Active),
        "an active hint must clear the suppression"
    );
    fx.clock
        .advance(Duration::from_millis(TYPING_SEND_INTERVAL_MS as u64));
    assert!(
        fx.session.send_typing("#omarchy", TypingPhase::Done),
        "done must be sent after a fresh active hint"
    );
}

#[test]
fn session_send_typing_refuses_invalid_targets() {
    let fx = Fixture::new(history_config());
    with_typing_registration(&fx);
    let before = fx.frames().len();
    assert!(
        !fx.session.send_typing("", TypingPhase::Active),
        "an empty target must be refused"
    );
    assert!(
        !fx.session.send_typing("bad target", TypingPhase::Active),
        "a target with a space must be refused"
    );
    assert_eq!(
        fx.frames().len(),
        before,
        "invalid typing targets must not write frames"
    );
}
