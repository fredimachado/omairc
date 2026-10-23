//! Read-marker publishing and MARKREAD get on direct messages. Ports
//! `tui/internal/controller/readmarker_test.go`.

use chrono::{TimeDelta, TimeZone, Utc};

use crate::irc::event::IrcConversationCause;

use super::fixture::{Fixture, OMARCHY};

const NET: &str = OMARCHY;

fn when(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32, ms: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, s)
        .unwrap()
        + TimeDelta::milliseconds(ms as i64)
}

fn enable_read_marker(fixture: &mut Fixture) {
    if let Some(session) = fixture.controller.session(NET) {
        session.locked(|core| {
            core.read_marker_token = Some("draft/read-marker");
        });
    }
}

fn deliver_privmsg(
    fixture: &mut Fixture,
    target: &str,
    nick: &str,
    body: &str,
    at: chrono::DateTime<Utc>,
) {
    let stamp = at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    fixture.deliver(
        NET,
        &format!("@time={stamp} :{nick}!u@h PRIVMSG {target} :{body}"),
    );
}

fn frames_contain(frames: &[String], needle: &str) -> bool {
    frames.iter().any(|frame| frame.contains(needle))
}

fn count_containing(frames: &[String], needle: &str) -> usize {
    frames.iter().filter(|frame| frame.contains(needle)).count()
}

fn ensure_room(fixture: &mut Fixture, target: &str) {
    let key = fixture.controller.reducer.conversation_key(NET, target);
    fixture
        .controller
        .reducer
        .ensure_conversation(&key, target, IrcConversationCause::ChannelState);
}

#[test]
fn server_marker_clears_unread_on_join() {
    let mut fixture = Fixture::seeded();
    let _other = fixture.controller.reducer.conversation_key(NET, "#other");
    ensure_room(&mut fixture, "#other");
    fixture.controller.select_conversation(NET, "#other");
    let marker_time = when(2024, 6, 1, 12, 0, 0, 0);
    let room = fixture.controller.reducer.conversation_key(NET, "#room");
    ensure_room(&mut fixture, "#room");
    deliver_privmsg(&mut fixture, "#room", "peer", "old", marker_time - TimeDelta::minutes(1));
    deliver_privmsg(&mut fixture, "#room", "peer", "new", marker_time + TimeDelta::minutes(1));
    assert_eq!(
        fixture.controller.reducer.find(&room).expect("room").unread,
        2,
        "expected unread before marker"
    );
    fixture
        .controller
        .read_marker_received(NET, "#room", Some(marker_time));
    assert_eq!(
        fixture.controller.reducer.find(&room).expect("room").unread,
        1,
        "unread after marker"
    );
}

#[test]
fn star_leaves_unread() {
    let mut fixture = Fixture::seeded();
    ensure_room(&mut fixture, "#other");
    fixture.controller.select_conversation(NET, "#other");
    let room = fixture.controller.reducer.conversation_key(NET, "#room");
    ensure_room(&mut fixture, "#room");
    let at = Utc::now();
    deliver_privmsg(&mut fixture, "#room", "peer", "hi", at);
    fixture.controller.read_marker_received(NET, "#room", None);
    assert_eq!(
        fixture.controller.reducer.find(&room).expect("room").unread,
        1,
        "* must not clear unread"
    );
}

#[test]
fn older_marker_does_not_move_badge_back() {
    let mut fixture = Fixture::seeded();
    let room = fixture.controller.reducer.conversation_key(NET, "#room");
    ensure_room(&mut fixture, "#room");
    let newer = when(2024, 6, 2, 12, 0, 0, 0);
    let older = newer - TimeDelta::hours(1);
    deliver_privmsg(&mut fixture, "#room", "peer", "line", newer);
    fixture.controller.read_marker_received(NET, "#room", Some(newer));
    fixture.controller.read_marker_received(NET, "#room", Some(older));
    let conv = fixture.controller.reducer.find(&room).expect("room");
    assert!(
        conv.read_marker == Some(newer),
        "older marker must not replace newer stored marker"
    );
}

#[test]
fn caught_up_selection_sends_read_marker_once() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    ensure_room(&mut fixture, "#room");
    fixture.controller.select_conversation(NET, "#room");
    fixture.clear_frames(NET);
    let at = when(2024, 6, 1, 12, 0, 0, 123);
    deliver_privmsg(&mut fixture, "#room", "peer", "hi", at);
    let want = "MARKREAD #room timestamp=2024-06-01T12:00:00.123Z\r\n";
    assert!(
        frames_contain(&fixture.frames(NET), want),
        "frames = {:?}, want {:?}",
        fixture.frames(NET),
        want
    );
    let before = fixture.frames(NET).len();
    deliver_privmsg(&mut fixture, "#room", "peer", "again", at);
    assert_eq!(
        fixture.frames(NET).len(),
        before,
        "equal timestamp must not send another MARKREAD"
    );
}

#[test]
fn status_open_does_not_publish_read_marker() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    ensure_room(&mut fixture, "#room");
    fixture.controller.select_conversation(NET, "#room");
    fixture.clear_frames(NET);
    let first = when(2024, 6, 1, 12, 0, 0, 123);
    deliver_privmsg(&mut fixture, "#room", "peer", "one", first);
    assert!(
        frames_contain(&fixture.frames(NET), "MARKREAD #room timestamp="),
        "want MARKREAD when caught up"
    );
    let before = count_containing(&fixture.frames(NET), "MARKREAD");
    fixture.controller.open_status(NET);
    let second = first + TimeDelta::minutes(1);
    deliver_privmsg(&mut fixture, "#room", "peer", "two", second);
    assert_eq!(
        count_containing(&fixture.frames(NET), "MARKREAD"),
        before,
        "Status must not publish read markers"
    );
    fixture.controller.note_transcript_viewport(true, true);
    assert_eq!(
        count_containing(&fixture.frames(NET), "MARKREAD"),
        before,
        "viewport sync on Status must not publish"
    );
}

#[test]
fn scrolled_up_does_not_send() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, false);
    ensure_room(&mut fixture, "#room");
    fixture.controller.select_conversation(NET, "#room");
    fixture.clear_frames(NET);
    let at = when(2024, 6, 1, 12, 0, 0, 0);
    deliver_privmsg(&mut fixture, "#room", "peer", "hi", at);
    assert!(
        !frames_contain(&fixture.frames(NET), "MARKREAD"),
        "scrolled up must not publish a read marker"
    );
}

#[test]
fn later_timestamp_sends_once_more() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    ensure_room(&mut fixture, "#room");
    fixture.controller.select_conversation(NET, "#room");
    fixture.clear_frames(NET);
    let first = when(2024, 6, 1, 12, 0, 0, 0);
    let second = first + TimeDelta::minutes(1);
    deliver_privmsg(&mut fixture, "#room", "peer", "one", first);
    fixture.deliver(NET, ":server FAIL MARKREAD RATE_LIMITED #room :slow\r\n");
    deliver_privmsg(&mut fixture, "#room", "peer", "two", second);
    assert_eq!(
        count_containing(&fixture.frames(NET), "MARKREAD"),
        2,
        "expected two MARKREAD frames"
    );
}

#[test]
fn select_existing_direct_requests_read_marker_get() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    ensure_room(&mut fixture, "#room");
    fixture.controller.select_conversation(NET, "#room");
    fixture.clear_frames(NET);
    let alice = fixture.controller.reducer.conversation_key(NET, "alice");
    fixture
        .controller
        .reducer
        .ensure_conversation(&alice, "alice", IrcConversationCause::InboundOther);
    fixture.controller.select_conversation(NET, "alice");
    assert!(
        frames_contain(&fixture.frames(NET), "MARKREAD alice\r\n"),
        "selecting existing DM must send MARKREAD get"
    );
}

#[test]
fn open_existing_direct_requests_read_marker_get() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    ensure_room(&mut fixture, "#room");
    fixture.controller.select_conversation(NET, "#room");
    fixture.clear_frames(NET);
    let alice = fixture.controller.reducer.conversation_key(NET, "alice");
    fixture
        .controller
        .reducer
        .ensure_conversation(&alice, "alice", IrcConversationCause::InboundOther);
    assert!(fixture.controller.open_direct("alice"));
    assert_eq!(
        count_containing(&fixture.frames(NET), "MARKREAD alice\r\n"),
        1,
        "opening existing DM must send one MARKREAD get"
    );
}

#[test]
fn direct_markread_get_waits_for_read_marker_cap() {
    let mut fixture = Fixture::seeded();
    let alice = fixture.controller.reducer.conversation_key(NET, "alice");
    fixture
        .controller
        .reducer
        .ensure_conversation(&alice, "alice", IrcConversationCause::InboundOther);
    fixture.controller.select_conversation(NET, "alice");
    assert!(
        !frames_contain(&fixture.frames(NET), "MARKREAD alice\r\n"),
        "must not mark get sent before the cap is available"
    );
    enable_read_marker(&mut fixture);
    fixture.controller.select_conversation(NET, "#omarchy");
    fixture.controller.select_conversation(NET, "alice");
    assert!(
        frames_contain(&fixture.frames(NET), "MARKREAD alice\r\n"),
        "must retry MARKREAD get once the cap is available"
    );
}

#[test]
fn gaining_focus_holds_read_marker_until_viewport_reports_caught_up() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    ensure_room(&mut fixture, "#room");
    fixture.controller.select_conversation(NET, "#room");
    fixture.clear_frames(NET);
    fixture.controller.set_window_active(false);
    fixture.controller.set_window_active(true);
    let at = when(2024, 6, 1, 12, 0, 0, 123);
    deliver_privmsg(&mut fixture, "#room", "peer", "after focus", at);
    assert!(
        !frames_contain(&fixture.frames(NET), "MARKREAD #room timestamp="),
        "focus gain must not publish until the shell reports follow-end"
    );
    fixture.controller.note_transcript_viewport(true, true);
    assert!(
        frames_contain(&fixture.frames(NET), "MARKREAD #room timestamp=2024-06-01T12:00:00.123Z\r\n"),
        "caught-up viewport must publish the read marker"
    );
}

#[test]
fn first_conversation_claim_requests_read_marker_get_for_direct() {
    use std::rc::Rc;

    use crate::controller::controller::Controller;
    use crate::controller::transports::NoTransports;
    use crate::session::clock::FakeClock;
    use crate::session::config::SessionConfig;
    use crate::session::loopback::LoopbackTransport;

    const LAB: &str = "lab";
    let clock = Rc::new(FakeClock::new(Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap()));
    let mut controller = Controller::with_parts(clock.clone(), Box::new(NoTransports));
    let transport = Rc::new(LoopbackTransport::new());
    let config = SessionConfig::with_defaults(LAB, "", "irc.example", "omairc");
    let session = controller
        .manager
        .create(config, transport.clone(), clock)
        .expect("session");
    session.start();
    transport.complete_connect();
    controller.pump();
    session.locked(|core| {
        core.read_marker_token = Some("draft/read-marker");
    });
    transport.inject_bytes(
        b":server CAP omairc ACK :draft/read-marker multi-prefix\r\n\
          :server 001 omairc :Welcome\r\n\
          :server 376 omairc :End\r\n",
    );
    controller.pump();
    transport.clear_written_frames();
    let at = when(2024, 6, 1, 12, 0, 0, 0);
    let stamp = at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    transport.inject_bytes(
        format!("@time={stamp} :alice!u@h PRIVMSG omairc :hi\r\n").as_bytes(),
    );
    controller.pump();
    assert_eq!(controller.selected_target(), "alice");
    let frames = transport
        .written_frames()
        .into_iter()
        .map(|frame| String::from_utf8_lossy(&frame).into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        count_containing(&frames, "MARKREAD alice\r\n"),
        1,
        "first conversation claim must send one MARKREAD get"
    );
}
