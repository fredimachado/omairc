//! MARKREAD get-hold release, FAIL retry, and reconnect ordering.

use chrono::{TimeDelta, TimeZone, Utc};

use crate::irc::event::{Event, IrcConversationCause, MessageEvent, MsgId, WelcomeEvent};
use crate::irc::serverfeatures::IrcServerFeatures;

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

fn frames_contain(frames: &[String], needle: &str) -> bool {
    frames.iter().any(|frame| frame.contains(needle))
}

fn count_containing(frames: &[String], needle: &str) -> usize {
    frames.iter().filter(|frame| frame.contains(needle)).count()
}

fn apply_direct_message(
    fixture: &mut Fixture,
    key: &crate::irc::event::ConversationKey,
    author: &str,
    body: &str,
    at: chrono::DateTime<Utc>,
) {
    fixture.controller.apply(Event::Message(MessageEvent {
        conversation: key.clone(),
        author: author.to_owned(),
        body: body.to_owned(),
        timestamp: at,
        server_time: Some(at),
        target: author.to_owned(),
        msgid: MsgId::default(),
        only_visible_to_you: false,
    }));
}

fn set_ascii_casemapping(fixture: &mut Fixture) {
    let mut features = IrcServerFeatures::new();
    features.apply_token("CASEMAPPING=ascii");
    fixture
        .controller
        .reducer
        .set_server_features(NET, features);
}

#[test]
fn reconnect_selected_direct_does_not_set_older_than_get_reply() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    let alice = fixture.controller.reducer.conversation_key(NET, "Alice");
    fixture
        .controller
        .reducer
        .ensure_conversation(&alice, "Alice", IrcConversationCause::InboundOther);
    let older = when(2024, 6, 1, 12, 0, 0, 0);
    apply_direct_message(&mut fixture, &alice, "Alice", "hi", older);
    fixture.controller.select_conversation(NET, "Alice");
    fixture.clear_frames(NET);
    fixture.controller.apply(Event::Welcome(WelcomeEvent {
        network_id: NET.to_string(),
        current_nick: "omairc".to_string(),
    }));
    assert_eq!(
        count_containing(&fixture.frames(NET), "MARKREAD Alice\r\n"),
        1,
        "welcome must send one get, frames={:?}",
        fixture.frames(NET)
    );
    assert!(
        !frames_contain(&fixture.frames(NET), "timestamp="),
        "welcome must not publish a set before the get reply, frames={:?}",
        fixture.frames(NET)
    );
    let newer = older + TimeDelta::hours(1);
    fixture.deliver(
        NET,
        &format!(
            ":server MARKREAD Alice :timestamp={}\r\n",
            newer.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        ),
    );
    assert!(
        !frames_contain(&fixture.frames(NET), "timestamp=2024-06-01T12:00:00.000Z"),
        "must not set an older marker after a newer get reply, frames={:?}",
        fixture.frames(NET)
    );
}

#[test]
fn star_get_reply_releases_hold_and_sends_one_set() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    let alice = fixture.controller.reducer.conversation_key(NET, "Alice");
    fixture
        .controller
        .reducer
        .ensure_conversation(&alice, "Alice", IrcConversationCause::InboundOther);
    let at = when(2024, 6, 1, 12, 0, 0, 123);
    apply_direct_message(&mut fixture, &alice, "Alice", "hi", at);
    fixture.clear_frames(NET);
    fixture.controller.select_conversation(NET, "Alice");
    assert!(
        !frames_contain(&fixture.frames(NET), "timestamp="),
        "must hold the set until the get reply, frames={:?}",
        fixture.frames(NET)
    );
    fixture.deliver(NET, ":server MARKREAD Alice :*\r\n");
    let want = "MARKREAD Alice timestamp=2024-06-01T12:00:00.123Z\r\n";
    assert_eq!(
        count_containing(&fixture.frames(NET), want),
        1,
        "star reply must release the hold and send exactly one set, frames={:?}",
        fixture.frames(NET)
    );
}

#[test]
fn older_get_reply_releases_hold_and_sends_one_newer_set() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    let alice = fixture.controller.reducer.conversation_key(NET, "Alice");
    fixture
        .controller
        .reducer
        .ensure_conversation(&alice, "Alice", IrcConversationCause::InboundOther);
    let newer = when(2024, 6, 1, 13, 0, 0, 0);
    apply_direct_message(&mut fixture, &alice, "Alice", "hi", newer);
    fixture.clear_frames(NET);
    fixture.controller.select_conversation(NET, "Alice");
    fixture.deliver(NET, ":server MARKREAD Alice :timestamp=2024-06-01T12:00:00.000Z\r\n");
    let want = "MARKREAD Alice timestamp=2024-06-01T13:00:00.000Z\r\n";
    assert_eq!(
        count_containing(&fixture.frames(NET), want),
        1,
        "older get reply must release the hold and send the newer local stamp once, frames={:?}",
        fixture.frames(NET)
    );
}

#[test]
fn fail_get_reply_releases_hold_and_sends_set() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    let alice = fixture.controller.reducer.conversation_key(NET, "Alice");
    fixture
        .controller
        .reducer
        .ensure_conversation(&alice, "Alice", IrcConversationCause::InboundOther);
    let at = when(2024, 6, 1, 12, 0, 0, 0);
    apply_direct_message(&mut fixture, &alice, "Alice", "hi", at);
    fixture.clear_frames(NET);
    fixture.controller.select_conversation(NET, "Alice");
    assert!(
        !frames_contain(&fixture.frames(NET), "timestamp="),
        "must hold until FAIL releases, frames={:?}",
        fixture.frames(NET)
    );
    fixture.deliver(NET, ":server FAIL MARKREAD INTERNAL_ERROR Alice :oops\r\n");
    let want = "MARKREAD Alice timestamp=2024-06-01T12:00:00.000Z\r\n";
    assert_eq!(
        count_containing(&fixture.frames(NET), want),
        1,
        "FAIL must release the hold and allow one set, frames={:?}",
        fixture.frames(NET)
    );
}

#[test]
fn reconnect_ascii_dm_holds_until_get_reply_after_feature_reset() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    set_ascii_casemapping(&mut fixture);
    let peer = "foo[bar]";
    let key = fixture.controller.reducer.conversation_key(NET, peer);
    assert_eq!(
        key.normalized_target, "foo[bar]",
        "ascii must keep brackets distinct from rfc1459 braces"
    );
    fixture
        .controller
        .reducer
        .ensure_conversation(&key, peer, IrcConversationCause::InboundOther);
    let marker = when(2024, 6, 1, 12, 0, 0, 0);
    let local = when(2024, 6, 1, 12, 30, 0, 0);
    fixture
        .controller
        .read_marker_received(NET, peer, Some(marker));
    apply_direct_message(&mut fixture, &key, peer, "hi", local);
    fixture.controller.select_conversation(NET, peer);
    fixture.clear_frames(NET);
    // Registered resets features to rfc1459 before Welcome; CASEMAPPING returns with 005.
    fixture
        .controller
        .reducer
        .set_server_features(NET, IrcServerFeatures::new());
    fixture.controller.apply(Event::Welcome(WelcomeEvent {
        network_id: NET.to_string(),
        current_nick: "fred".to_string(),
    }));
    assert_eq!(
        count_containing(&fixture.frames(NET), &format!("MARKREAD {peer}\r\n")),
        1,
        "welcome must send one get under the selected key, frames={:?}",
        fixture.frames(NET)
    );
    assert!(
        !frames_contain(&fixture.frames(NET), "timestamp="),
        "hold must match after the rfc1459 feature reset, frames={:?}",
        fixture.frames(NET)
    );
    set_ascii_casemapping(&mut fixture);
    fixture.deliver(
        NET,
        &format!(
            ":server MARKREAD {peer} :timestamp={}\r\n",
            marker.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        ),
    );
    let want = format!("MARKREAD {peer} timestamp=2024-06-01T12:30:00.000Z\r\n");
    assert_eq!(
        count_containing(&fixture.frames(NET), &want),
        1,
        "after 005 and the get reply, exactly one newer set must go out, frames={:?}",
        fixture.frames(NET)
    );
}

#[test]
fn unparseable_get_reply_releases_hold_and_sends_set() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    let alice = fixture.controller.reducer.conversation_key(NET, "Alice");
    fixture
        .controller
        .reducer
        .ensure_conversation(&alice, "Alice", IrcConversationCause::InboundOther);
    let at = when(2024, 6, 1, 12, 0, 0, 0);
    apply_direct_message(&mut fixture, &alice, "Alice", "hi", at);
    fixture.clear_frames(NET);
    fixture.controller.select_conversation(NET, "Alice");
    assert!(
        !frames_contain(&fixture.frames(NET), "timestamp="),
        "must hold until the unparseable reply, frames={:?}",
        fixture.frames(NET)
    );
    fixture.deliver(NET, ":server MARKREAD Alice :timestamp=not-a-time\r\n");
    let want = "MARKREAD Alice timestamp=2024-06-01T12:00:00.000Z\r\n";
    assert_eq!(
        count_containing(&fixture.frames(NET), want),
        1,
        "unparseable reply must release the hold and allow one set, frames={:?}",
        fixture.frames(NET)
    );
}

#[test]
fn fail_without_target_releases_hold_and_sends_set() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    let alice = fixture.controller.reducer.conversation_key(NET, "Alice");
    fixture
        .controller
        .reducer
        .ensure_conversation(&alice, "Alice", IrcConversationCause::InboundOther);
    let at = when(2024, 6, 1, 12, 0, 0, 0);
    apply_direct_message(&mut fixture, &alice, "Alice", "hi", at);
    fixture.clear_frames(NET);
    fixture.controller.select_conversation(NET, "Alice");
    assert!(
        !frames_contain(&fixture.frames(NET), "timestamp="),
        "must hold until the untargeted FAIL, frames={:?}",
        fixture.frames(NET)
    );
    fixture.deliver(NET, ":server FAIL MARKREAD NEED_MORE_PARAMS :Missing parameters\r\n");
    let want = "MARKREAD Alice timestamp=2024-06-01T12:00:00.000Z\r\n";
    assert_eq!(
        count_containing(&fixture.frames(NET), want),
        1,
        "FAIL without a target must release the hold and allow one set, frames={:?}",
        fixture.frames(NET)
    );
}

#[test]
fn cap_drop_releases_hold_and_allows_set_when_cap_returns() {
    let mut fixture = Fixture::seeded();
    fixture.controller.note_transcript_viewport(true, true);
    // Negotiate the cap for real so CAP DEL withdraws it from the enabled set
    // and the controller sees the drop.
    fixture.deliver(NET, ":server CAP * NEW :draft/read-marker\r\n");
    fixture.deliver(NET, ":server CAP * ACK :draft/read-marker\r\n");
    let alice = fixture.controller.reducer.conversation_key(NET, "Alice");
    fixture
        .controller
        .reducer
        .ensure_conversation(&alice, "Alice", IrcConversationCause::InboundOther);
    let at = when(2024, 6, 1, 12, 0, 0, 0);
    apply_direct_message(&mut fixture, &alice, "Alice", "hi", at);
    fixture.clear_frames(NET);
    fixture.controller.select_conversation(NET, "Alice");
    assert!(
        frames_contain(&fixture.frames(NET), "MARKREAD Alice\r\n"),
        "select must send a get once the cap is acknowledged, frames={:?}",
        fixture.frames(NET)
    );
    assert!(
        !frames_contain(&fixture.frames(NET), "timestamp="),
        "must hold before the cap drop, frames={:?}",
        fixture.frames(NET)
    );
    fixture.deliver(NET, ":server CAP * DEL :draft/read-marker\r\n");
    enable_read_marker(&mut fixture);
    // Cap drop already released the hold; an unchanged viewport report is not a
    // retry trigger, so sync explicitly once the cap is back.
    fixture.controller.sync_read_marker_for_selection();
    let want = "MARKREAD Alice timestamp=2024-06-01T12:00:00.000Z\r\n";
    assert_eq!(
        count_containing(&fixture.frames(NET), want),
        1,
        "dropping the read-marker cap must release the hold, frames={:?}",
        fixture.frames(NET)
    );
}

#[test]
fn receive_fallback_releases_hold_when_reply_key_mismatches_selected() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    set_ascii_casemapping(&mut fixture);
    let peer = "foo[bar]";
    let key = fixture.controller.reducer.conversation_key(NET, peer);
    fixture
        .controller
        .reducer
        .ensure_conversation(&key, peer, IrcConversationCause::InboundOther);
    let at = when(2024, 6, 1, 12, 30, 0, 0);
    apply_direct_message(&mut fixture, &key, peer, "hi", at);
    fixture.controller.select_conversation(NET, peer);
    fixture.clear_frames(NET);
    fixture
        .controller
        .reducer
        .set_server_features(NET, IrcServerFeatures::new());
    fixture.controller.apply(Event::Welcome(WelcomeEvent {
        network_id: NET.to_string(),
        current_nick: "fred".to_string(),
    }));
    assert!(
        !frames_contain(&fixture.frames(NET), "timestamp="),
        "must hold after welcome under the selected ascii key, frames={:?}",
        fixture.frames(NET)
    );
    // Reply while still on rfc1459: conversation_key folds to foo{bar}, so the
    // primary clear misses and the selected-key fallback must release the hold.
    fixture.deliver(NET, &format!(":server MARKREAD {peer} :*\r\n"));
    let want = format!("MARKREAD {peer} timestamp=2024-06-01T12:30:00.000Z\r\n");
    assert_eq!(
        count_containing(&fixture.frames(NET), &want),
        1,
        "receive fallback must release the hold before 005, frames={:?}",
        fixture.frames(NET)
    );
}

#[test]
fn fail_set_does_not_immediately_resend_same_stamp() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    let alice = fixture.controller.reducer.conversation_key(NET, "Alice");
    fixture
        .controller
        .reducer
        .ensure_conversation(&alice, "Alice", IrcConversationCause::InboundOther);
    let at = when(2024, 6, 1, 12, 0, 0, 0);
    apply_direct_message(&mut fixture, &alice, "Alice", "hi", at);
    fixture.clear_frames(NET);
    fixture.controller.select_conversation(NET, "Alice");
    fixture.deliver(NET, ":server MARKREAD Alice :*\r\n");
    let want = "MARKREAD Alice timestamp=2024-06-01T12:00:00.000Z\r\n";
    assert_eq!(
        count_containing(&fixture.frames(NET), want),
        1,
        "want one set after star, frames={:?}",
        fixture.frames(NET)
    );
    fixture.deliver(NET, ":server FAIL MARKREAD RATE_LIMITED Alice :slow\r\n");
    // An unchanged viewport report must not immediately resend the same stamp.
    fixture.controller.note_transcript_viewport(true, true);
    assert_eq!(
        count_containing(&fixture.frames(NET), want),
        1,
        "FAIL must not immediately resend the same stamp, frames={:?}",
        fixture.frames(NET)
    );
}

#[test]
fn quick_reconnect_after_failed_set_get_before_stale_set() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    let alice = fixture.controller.reducer.conversation_key(NET, "Alice");
    fixture
        .controller
        .reducer
        .ensure_conversation(&alice, "Alice", IrcConversationCause::InboundOther);
    let failed_at = when(2024, 6, 1, 12, 0, 0, 0);
    apply_direct_message(&mut fixture, &alice, "Alice", "hi", failed_at);
    fixture.controller.select_conversation(NET, "Alice");
    fixture.deliver(NET, ":server MARKREAD Alice :*\r\n");
    let want_fail = "MARKREAD Alice timestamp=2024-06-01T12:00:00.000Z\r\n";
    assert!(
        frames_contain(&fixture.frames(NET), want_fail),
        "want initial set, frames={:?}",
        fixture.frames(NET)
    );
    fixture.deliver(NET, ":server FAIL MARKREAD RATE_LIMITED Alice :slow\r\n");
    // Leaving Registered arms the reconnect hold so a sync cannot publish
    // before Welcome's get.
    fixture.controller.arm_read_marker_reconnect_hold(NET);
    fixture.clear_frames(NET);
    fixture.controller.sync_read_marker_for_selection();
    assert!(
        !frames_contain(&fixture.frames(NET), "timestamp="),
        "reconnect hold must block sets before Welcome get, frames={:?}",
        fixture.frames(NET)
    );
    fixture.controller.apply(Event::Welcome(WelcomeEvent {
        network_id: NET.to_string(),
        current_nick: "omairc".to_string(),
    }));
    let frames = fixture.frames(NET);
    let get_idx = frames.iter().position(|frame| {
        frame.contains("MARKREAD Alice") && !frame.contains("timestamp=")
    });
    let set_idx = frames
        .iter()
        .position(|frame| frame.contains("MARKREAD Alice") && frame.contains("timestamp="));
    assert!(
        get_idx.is_some(),
        "welcome must send a get, frames={:?}",
        frames
    );
    if let (Some(get), Some(set)) = (get_idx, set_idx) {
        assert!(get < set, "get must go out before any set, frames={:?}", frames);
    }
    assert!(
        !frames_contain(&frames, "timestamp=2024-06-01T12:00:00.000Z"),
        "must not send the failed older stamp before the get reply, frames={:?}",
        frames
    );
    let server_now = failed_at + TimeDelta::seconds(2);
    fixture.deliver(
        NET,
        &format!(
            ":server MARKREAD Alice :timestamp={}\r\n",
            server_now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        ),
    );
    assert!(
        !frames_contain(&fixture.frames(NET), "timestamp=2024-06-01T12:00:00.000Z"),
        "nothing older than the server marker may go out, frames={:?}",
        fixture.frames(NET)
    );
}
