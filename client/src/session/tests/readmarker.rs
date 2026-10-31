//! the outbound MARKREAD and
//! READ queue, the in-flight rule, and the inbound echo, star, and FAIL paths.

use chrono::{DateTime, TimeDelta, TimeZone, Utc};

use super::fixture::Fixture;
use crate::session::events::{SessionEvent, SessionState};

use super::fixture::test_config;

/// 2024-06-01T12:00:00Z plus `seconds`.
fn at(seconds: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).single().unwrap() + TimeDelta::seconds(seconds)
}

/// Registers with `draft/read-marker` (or the given tokens) acknowledged.
fn register_with_read_marker(fx: &Fixture, tokens: &str) {
    fx.connect_tls();
    fx.inject(&format!(":server CAP omairc LS :{tokens}\r\n"));
    fx.inject(&format!(":server CAP omairc ACK :{tokens}\r\n"));
    fx.inject(":server 001 omairc :Welcome\r\n");
}

/// The `ReadMarkerReceived` events so far, as (target, marker).
fn read_markers(fx: &Fixture) -> Vec<(String, Option<DateTime<Utc>>)> {
    fx.events()
        .into_iter()
        .filter_map(|event| match event {
            SessionEvent::ReadMarkerReceived { target, marker, .. } => Some((target, marker)),
            _ => None,
        })
        .collect()
}

fn count_containing(frames: &[String], needle: &str) -> usize {
    frames.iter().filter(|frame| frame.contains(needle)).count()
}

fn fresh() -> Fixture {
    Fixture::new(test_config())
}

#[test]
fn read_marker_cap_absent_sends_nothing() {
    let fx = fresh();
    register_with_read_marker(&fx, "multi-prefix");
    let before = fx.frames().len();
    fx.session
        .queue_read_marker_set("#omarchy", Utc.timestamp_opt(0, 0).single().unwrap());
    assert_eq!(
        fx.frames().len(),
        before,
        "wrote frames without the read-marker cap"
    );
}

#[test]
fn read_marker_caught_up_sends_markread_once() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker multi-prefix");
    let when =
        Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).single().unwrap() + TimeDelta::milliseconds(123);
    fx.session.queue_read_marker_set("#omarchy", when);
    let want = "MARKREAD #omarchy timestamp=2024-06-01T12:00:00.123Z\r\n";
    assert!(fx.wrote(want), "frames = {:?}", fx.frames());
    fx.session.queue_read_marker_set("#omarchy", when);
    assert_eq!(
        count_containing(&fx.frames(), want),
        1,
        "duplicate MARKREAD sent"
    );
}

#[test]
fn read_marker_soju_uses_read_command() {
    let fx = fresh();
    register_with_read_marker(&fx, "soju.im/read multi-prefix");
    fx.session.queue_read_marker_set("alice", at(0));
    assert!(
        fx.wrote("READ alice timestamp=2024-06-01T12:00:00.000Z\r\n"),
        "frames = {:?}",
        fx.frames()
    );
}

#[test]
fn read_marker_older_set_not_queued() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    fx.session.queue_read_marker_set("#omarchy", at(60));
    fx.session.queue_read_marker_set("#omarchy", at(0));
    assert_eq!(
        count_containing(&fx.frames(), "MARKREAD"),
        1,
        "an older timestamp must not send again"
    );
}

#[test]
fn read_marker_newer_timestamp_sends_again_after_fail() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    fx.session.queue_read_marker_set("#omarchy", at(0));
    fx.inject(":server FAIL MARKREAD RATE_LIMITED #omarchy :slow down\r\n");
    fx.session.queue_read_marker_set("#omarchy", at(60));
    assert_eq!(
        count_containing(&fx.frames(), "MARKREAD"),
        2,
        "two MARKREAD frames after FAIL"
    );
}

#[test]
fn read_marker_fail_does_not_disconnect() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    fx.session.queue_read_marker_set("#omarchy", at(0));
    fx.inject(":server FAIL MARKREAD INVALID_TARGET #omarchy :nope\r\n");
    assert_eq!(fx.session.state(), SessionState::Registered);
}

#[test]
fn read_marker_get_on_direct() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    fx.session.request_read_marker_get("alice");
    assert!(fx.wrote("MARKREAD alice\r\n"), "frames = {:?}", fx.frames());
}

#[test]
fn read_marker_echo_does_not_resend_equal_pending() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    let when =
        Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).single().unwrap() + TimeDelta::milliseconds(123);
    fx.session.queue_read_marker_set("#omarchy", when);
    let want = "MARKREAD #omarchy timestamp=2024-06-01T12:00:00.123Z\r\n";
    assert!(fx.wrote(want));
    fx.session.queue_read_marker_set("#omarchy", when);
    fx.inject(":server MARKREAD #omarchy :timestamp=2024-06-01T12:00:00.123Z\r\n");
    assert_eq!(
        count_containing(&fx.frames(), want),
        1,
        "echo must not resend an equal timestamp"
    );
}

#[test]
fn read_marker_star_does_not_clear_in_flight() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    fx.session.queue_read_marker_set("#omarchy", at(0));
    fx.inject(":server MARKREAD #omarchy :*\r\n");
    let markers = read_markers(&fx);
    assert_eq!(markers.len(), 1);
    assert!(
        markers[0].1.is_none(),
        "a star reply must deliver a nil marker"
    );
    fx.session.queue_read_marker_set("#omarchy", at(60));
    assert_eq!(
        count_containing(&fx.frames(), "timestamp="),
        1,
        "star must not flush a newer pending set"
    );
}

#[test]
fn read_marker_older_get_reply_leaves_in_flight_set() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    fx.session.queue_read_marker_set("alice", at(0));
    fx.inject(":server MARKREAD alice :timestamp=2024-06-01T11:00:00.000Z\r\n");
    fx.session.queue_read_marker_set("alice", at(60));
    assert_eq!(
        count_containing(&fx.frames(), "timestamp="),
        1,
        "an older get reply must not clear the newer in-flight set"
    );
}

#[test]
fn read_marker_bad_echo_flushes_pending() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    fx.session.queue_read_marker_set("#omarchy", at(0));
    fx.inject(":server MARKREAD #omarchy :timestamp=not-a-time\r\n");
    fx.session.queue_read_marker_set("#omarchy", at(60));
    assert_eq!(
        count_containing(&fx.frames(), "timestamp=2024-06-01T12:01:00.000Z"),
        1,
        "a bad echo must clear in-flight and allow the pending flush"
    );
}

#[test]
fn read_marker_fail_named_target_only_clears_that_entry() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    fx.session.queue_read_marker_set("#omarchy", at(0));
    fx.session.queue_read_marker_set("alice", at(60));
    fx.inject(":server FAIL MARKREAD INTERNAL_ERROR the_given_target :failed\r\n");
    assert_eq!(
        count_containing(&fx.frames(), "MARKREAD #omarchy timestamp="),
        1,
        "#omarchy must stay in flight"
    );
    fx.session.queue_read_marker_set("#omarchy", at(120));
    assert_eq!(
        count_containing(&fx.frames(), "MARKREAD #omarchy timestamp="),
        1,
        "an untargeted-style clear must not resend #omarchy"
    );
}

#[test]
fn read_marker_fail_named_target_with_unusual_chars() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    fx.session.queue_read_marker_set("!channel", at(0));
    fx.session.queue_read_marker_set("#omarchy", at(60));
    fx.inject(":server FAIL MARKREAD INVALID_TARGET !channel :nope\r\n");
    fx.session.queue_read_marker_set("!channel", at(120));
    assert_eq!(
        count_containing(&fx.frames(), "MARKREAD !channel timestamp="),
        2,
        "a named FAIL clears only !channel"
    );
    assert_eq!(
        count_containing(&fx.frames(), "MARKREAD #omarchy timestamp="),
        1,
        "#omarchy must stay in flight"
    );
}

#[test]
fn read_marker_fail_without_target_clears_in_flight() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    fx.session.queue_read_marker_set("#omarchy", at(0));
    fx.inject(":server FAIL MARKREAD NEED_MORE_PARAMS :Missing parameters\r\n");
    fx.session.queue_read_marker_set("#omarchy", at(60));
    assert_eq!(
        count_containing(&fx.frames(), "MARKREAD"),
        2,
        "expected a resend after an untargeted FAIL"
    );
}

#[test]
fn read_marker_is_consumed_and_not_a_protocol_error() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    fx.inject(":server MARKREAD #omarchy :timestamp=2024-06-01T12:00:00.000Z\r\n");
    assert!(fx.errors().is_empty());
    assert!(fx
        .messages()
        .iter()
        .all(|message| message.command != "MARKREAD"));
}

#[test]
fn read_marker_echo_then_equal_set_does_not_resend() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    let when =
        Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).single().unwrap() + TimeDelta::milliseconds(123);
    fx.session.queue_read_marker_set("#omarchy", when);
    let want = "MARKREAD #omarchy timestamp=2024-06-01T12:00:00.123Z\r\n";
    assert!(fx.wrote(want));
    fx.inject(":server MARKREAD #omarchy :timestamp=2024-06-01T12:00:00.123Z\r\n");
    fx.session.queue_read_marker_set("#omarchy", when);
    assert_eq!(
        count_containing(&fx.frames(), want),
        1,
        "after echo, equal timestamp must not send again: {:?}",
        fx.frames()
    );
}

#[test]
fn read_marker_received_newer_blocks_older_set() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    fx.inject(":server MARKREAD alice :timestamp=2024-06-01T13:00:00.000Z\r\n");
    fx.session.queue_read_marker_set("Alice", at(0));
    assert_eq!(
        count_containing(&fx.frames(), "timestamp="),
        0,
        "older set after newer received marker must not send: {:?}",
        fx.frames()
    );
}

#[test]
fn read_marker_fail_allows_same_timestamp_retry() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    let want = "MARKREAD #omarchy timestamp=2024-06-01T12:00:00.000Z\r\n";
    fx.session.queue_read_marker_set("#omarchy", at(0));
    assert_eq!(count_containing(&fx.frames(), want), 1);
    fx.inject(":server FAIL MARKREAD RATE_LIMITED #omarchy :slow\r\n");
    fx.session.queue_read_marker_set("#omarchy", at(0));
    assert_eq!(
        count_containing(&fx.frames(), want),
        2,
        "FAIL must clear in-flight so the same stamp can be retried: {:?}",
        fx.frames()
    );
}

#[test]
fn read_marker_set_before_registered_does_not_block_same_stamp() {
    let fx = fresh();
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :draft/read-marker multi-prefix\r\n");
    fx.inject(":server CAP omairc ACK :draft/read-marker multi-prefix\r\n");
    let want = "MARKREAD #omarchy timestamp=2024-06-01T12:00:00.000Z\r\n";
    fx.session.queue_read_marker_set("#omarchy", at(0));
    assert_eq!(
        count_containing(&fx.frames(), want),
        0,
        "must not send a set before 001, frames={:?}",
        fx.frames()
    );
    fx.inject(":server 001 omairc :Welcome\r\n");
    fx.session.queue_read_marker_set("#omarchy", at(0));
    assert_eq!(
        count_containing(&fx.frames(), want),
        1,
        "a refused send must not leave in_flight/last_known blocking the same stamp: {:?}",
        fx.frames()
    );
}

#[test]
fn read_marker_reconnect_clears_in_flight_so_same_stamp_retries() {
    let fx = fresh();
    register_with_read_marker(&fx, "draft/read-marker");
    let want = "MARKREAD #omarchy timestamp=2024-06-01T12:00:00.000Z\r\n";
    fx.session.queue_read_marker_set("#omarchy", at(0));
    assert_eq!(count_containing(&fx.frames(), want), 1);
    fx.transport.remote_close();
    fx.fire_reconnect();
    fx.transport.complete_connect();
    register_with_read_marker(&fx, "draft/read-marker");
    fx.session.queue_read_marker_set("#omarchy", at(0));
    assert_eq!(
        count_containing(&fx.frames(), want),
        2,
        "connection reset must clear in_flight so the same stamp can be sent again: {:?}",
        fx.frames()
    );
}
