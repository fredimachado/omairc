//! The MONITOR coordinator over the real demo: the frames `/monitor`, `/unmonitor`
//! and the subscription write, the 730, 731 and 734 numerics that fold into presence
//! and the Status and inbox lines a change raises.
//! `tui/internal/controller/monitorcoordinator_test.go`. The demo answers MONITOR `+`
//! and `-` the way the server does: `anna` is online in the seed, `ghost` is not.

use crate::controller::monitorcoordinator::{MonitorOutcome, MonitorPresence};
use crate::irc::inbox::InboxKind;
use crate::irc::serverfeatures::IrcServerFeatures;

use super::fixture::{status_text, Fixture, OMARCHY};

/// The last frame the loopback wrote, as text.
fn last_frame(fixture: &Fixture) -> String {
    fixture
        .frames(OMARCHY)
        .last()
        .cloned()
        .expect("a frame was written")
}

#[test]
fn subscribe_sends_the_stored_list_once_per_registration() {
    let mut fixture = Fixture::seeded();
    fixture.controller.monitors.add(OMARCHY, "dax", Default::default());
    fixture.controller.monitors.add(OMARCHY, "ghost", Default::default());
    fixture.controller.monitor_forget_presence(OMARCHY);
    fixture.clear_frames(OMARCHY);

    fixture.controller.monitor_subscribe(OMARCHY);
    assert_eq!(fixture.frames(OMARCHY), ["MONITOR + dax,ghost\r\n"]);

    fixture.clear_frames(OMARCHY);
    fixture.controller.monitor_subscribe(OMARCHY);
    assert!(
        fixture.frames(OMARCHY).is_empty(),
        "a second subscribe in the same registration writes nothing"
    );
}

#[test]
fn subscribe_is_skipped_when_monitor_is_not_advertised() {
    let mut fixture = Fixture::seeded();
    fixture.controller.monitors.add(OMARCHY, "dax", Default::default());
    fixture.controller.monitor_forget_presence(OMARCHY);
    fixture
        .controller
        .reducer
        .set_server_features(OMARCHY, IrcServerFeatures::new());
    fixture.clear_frames(OMARCHY);

    fixture.controller.monitor_subscribe(OMARCHY);
    assert!(fixture.frames(OMARCHY).is_empty());
}

#[test]
fn subscribe_again_after_forgetting_presence_sends_the_list_again() {
    let mut fixture = Fixture::seeded();
    fixture.controller.monitors.add(OMARCHY, "dax", Default::default());
    fixture.controller.monitor_forget_presence(OMARCHY);
    fixture.controller.monitor_subscribe(OMARCHY);
    fixture.clear_frames(OMARCHY);

    fixture.controller.monitor_forget_presence(OMARCHY);
    fixture.controller.monitor_subscribe(OMARCHY);
    assert_eq!(fixture.frames(OMARCHY), ["MONITOR + dax\r\n"]);
}

#[test]
fn monitor_add_writes_the_frame_and_the_demo_answers_presence() {
    let mut fixture = Fixture::seeded();
    fixture.clear_frames(OMARCHY);

    assert_eq!(
        fixture.controller.monitor_add(OMARCHY, "anna"),
        MonitorOutcome::Sent
    );
    assert_eq!(last_frame(&fixture), "MONITOR + anna\r\n");
    assert_eq!(
        fixture.controller.monitor_presence(OMARCHY, "anna"),
        MonitorPresence::Online,
        "the demo answers 730 for a member"
    );
    assert!(fixture
        .controller
        .watched_nicks(OMARCHY)
        .contains(&"anna".to_owned()));
    assert_eq!(
        status_text(&fixture.controller, OMARCHY).last().map(String::as_str),
        Some("Watching anna")
    );
}

#[test]
fn a_presence_change_is_recorded_once_and_newly_online_raises_an_inbox_row() {
    let mut fixture = Fixture::seeded();
    fixture.controller.monitor_add(OMARCHY, "ghost");
    assert_eq!(
        fixture.controller.monitor_presence(OMARCHY, "ghost"),
        MonitorPresence::Offline,
        "the demo answers 731 for a nick it does not know"
    );
    let baseline = status_text(&fixture.controller, OMARCHY).len();

    // 730 from unknown-to-online is a change, but the first offline answer set the
    // state, so going online is the first transition that records a line.
    fixture.deliver(OMARCHY, ":server 730 fred :ghost!u@h");
    assert_eq!(
        status_text(&fixture.controller, OMARCHY)[baseline..],
        ["ghost is online"],
    );
    assert_eq!(
        fixture.controller.monitor_presence(OMARCHY, "ghost"),
        MonitorPresence::Online
    );
    let inbox = fixture.controller.inbox_items();
    assert!(
        inbox
            .iter()
            .any(|item| item.kind == InboxKind::MonitorOnline && item.actor == "ghost"),
        "newly online raises an inbox row"
    );

    // A repeat of the same state is a no-op.
    let before = status_text(&fixture.controller, OMARCHY).len();
    fixture.deliver(OMARCHY, ":server 730 fred :ghost!u@h");
    assert_eq!(status_text(&fixture.controller, OMARCHY).len(), before);

    fixture.deliver(OMARCHY, ":server 731 fred :ghost");
    assert_eq!(
        status_text(&fixture.controller, OMARCHY).last().map(String::as_str),
        Some("ghost is offline")
    );
    assert_eq!(
        fixture.controller.monitor_presence(OMARCHY, "ghost"),
        MonitorPresence::Offline
    );
}

#[test]
fn a_muted_nick_is_recorded_but_does_not_raise_an_inbox_row() {
    let mut fixture = Fixture::seeded();
    fixture.controller.monitor_add(OMARCHY, "ghost");
    fixture.controller.apply_mute(OMARCHY, "ghost", true);
    fixture.deliver(OMARCHY, ":server 730 fred :ghost!u@h");

    assert_eq!(
        status_text(&fixture.controller, OMARCHY).last().map(String::as_str),
        Some("ghost is online"),
        "the Status line still records the change"
    );
    assert!(
        !fixture
            .controller
            .inbox_items()
            .iter()
            .any(|item| item.kind == InboxKind::MonitorOnline && item.actor == "ghost"),
        "a muted nick raises no inbox row"
    );
}

#[test]
fn a_734_list_full_drops_every_named_target() {
    let mut fixture = Fixture::seeded();
    fixture.controller.monitor_add(OMARCHY, "one");
    fixture.controller.monitor_add(OMARCHY, "two");
    fixture.deliver(OMARCHY, ":server 730 fred :one!u@h");
    let before = status_text(&fixture.controller, OMARCHY).len();

    fixture.deliver(OMARCHY, ":server 734 fred 2 one,two :Monitor list is full.");

    assert!(!fixture.controller.watched_nicks(OMARCHY).contains(&"one".to_owned()));
    assert!(!fixture.controller.watched_nicks(OMARCHY).contains(&"two".to_owned()));
    assert_eq!(
        status_text(&fixture.controller, OMARCHY)[before..],
        ["Monitor list is full (2): one, two."],
    );
}

#[test]
fn monitored_lists_each_nick_with_its_presence() {
    let mut fixture = Fixture::seeded();
    fixture.controller.monitor_add(OMARCHY, "anna");
    fixture.controller.monitor_add(OMARCHY, "ghost");
    fixture.controller.monitors.add(OMARCHY, "nobody", Default::default());
    let before = status_text(&fixture.controller, OMARCHY).len();

    assert_eq!(fixture.controller.monitor_list(OMARCHY, ""), MonitorOutcome::Sent);
    assert_eq!(
        status_text(&fixture.controller, OMARCHY)[before..],
        ["Watching: anna (online), ghost (offline), nobody (unknown)"],
    );
    assert_eq!(
        fixture.controller.monitor_list(OMARCHY, "extra"),
        MonitorOutcome::Refused,
        "/monitored takes no argument"
    );
}

#[test]
fn monitored_with_nothing_watched_says_so() {
    let mut fixture = Fixture::seeded();
    let before = status_text(&fixture.controller, OMARCHY).len();

    assert_eq!(fixture.controller.monitor_list(OMARCHY, ""), MonitorOutcome::Sent);
    assert_eq!(
        status_text(&fixture.controller, OMARCHY)[before..],
        ["Not watching anyone"],
    );
}

#[test]
fn an_add_of_a_watched_nick_says_already_watching_without_a_frame() {
    let mut fixture = Fixture::seeded();
    fixture.controller.monitor_add(OMARCHY, "dax");
    fixture.clear_frames(OMARCHY);

    assert_eq!(
        fixture.controller.monitor_add(OMARCHY, "dax"),
        MonitorOutcome::Sent
    );
    assert!(fixture.frames(OMARCHY).is_empty());
    assert_eq!(
        status_text(&fixture.controller, OMARCHY).last().map(String::as_str),
        Some("Already watching dax")
    );
}

#[test]
fn an_add_at_the_limit_is_reported_and_writes_nothing() {
    let mut fixture = Fixture::seeded();
    // The server now allows two nicks; the demo still answers at its own limit.
    fixture.deliver(OMARCHY, ":server 005 fred MONITOR=2 :are supported by this server");
    fixture.controller.monitor_add(OMARCHY, "one");
    fixture.controller.monitor_add(OMARCHY, "two");
    fixture.clear_frames(OMARCHY);

    assert_eq!(
        fixture.controller.monitor_add(OMARCHY, "three"),
        MonitorOutcome::Sent
    );
    assert!(fixture.frames(OMARCHY).is_empty(), "no frame past the limit");
    assert!(!fixture.controller.watched_nicks(OMARCHY).contains(&"three".to_owned()));
    assert_eq!(
        status_text(&fixture.controller, OMARCHY).last().map(String::as_str),
        Some("Monitor list is full (2): three.")
    );
}

#[test]
fn a_failed_frame_rolls_the_store_back_and_says_nothing() {
    let mut fixture = Fixture::seeded();
    let before = status_text(&fixture.controller, OMARCHY).len();

    // A newline cannot be framed, so the session refuses the MONITOR line.
    assert_eq!(
        fixture.controller.monitor_add(OMARCHY, "bad\nnick"),
        MonitorOutcome::Refused
    );
    assert!(!fixture
        .controller
        .watched_nicks(OMARCHY)
        .contains(&"bad\nnick".to_owned()));
    assert_eq!(status_text(&fixture.controller, OMARCHY).len(), before);
}

#[test]
fn unmonitor_writes_the_removal_and_forgets_the_presence() {
    let mut fixture = Fixture::seeded();
    fixture.controller.monitor_add(OMARCHY, "ghost");
    fixture.clear_frames(OMARCHY);

    assert_eq!(
        fixture.controller.monitor_remove(OMARCHY, "ghost"),
        MonitorOutcome::Sent
    );
    assert_eq!(last_frame(&fixture), "MONITOR - ghost\r\n");
    assert!(!fixture.controller.watched_nicks(OMARCHY).contains(&"ghost".to_owned()));
    assert_eq!(
        fixture.controller.monitor_presence(OMARCHY, "ghost"),
        MonitorPresence::Unknown
    );
    assert_eq!(
        status_text(&fixture.controller, OMARCHY).last().map(String::as_str),
        Some("No longer watching ghost")
    );

    fixture.clear_frames(OMARCHY);
    fixture.controller.monitor_remove(OMARCHY, "ghost");
    assert!(fixture.frames(OMARCHY).is_empty());
    assert_eq!(
        status_text(&fixture.controller, OMARCHY).last().map(String::as_str),
        Some("Not watching ghost")
    );
}

#[test]
fn a_network_without_monitor_refuses_the_add_on_status() {
    let mut fixture = Fixture::seeded();
    fixture
        .controller
        .reducer
        .set_server_features(OMARCHY, IrcServerFeatures::new());
    fixture.clear_frames(OMARCHY);

    assert_eq!(
        fixture.controller.monitor_add(OMARCHY, "one"),
        MonitorOutcome::Sent
    );
    assert!(fixture.frames(OMARCHY).is_empty());
    assert_eq!(
        status_text(&fixture.controller, OMARCHY).last().map(String::as_str),
        Some("This network does not support MONITOR.")
    );
}

#[test]
fn an_empty_or_unknown_network_is_refused_or_not_connected() {
    let mut fixture = Fixture::seeded();
    assert_eq!(
        fixture.controller.monitor_list("", ""),
        MonitorOutcome::Refused,
        "no network resolved"
    );
    assert_eq!(
        fixture.controller.monitor_add("nowhere", "one"),
        MonitorOutcome::NotConnected,
        "no session for the network"
    );
    assert_eq!(
        fixture.controller.monitor_add(OMARCHY, "two words"),
        MonitorOutcome::Refused,
        "a second token is refused"
    );
}
