//! `/list` through the wire over the real demo: the `LIST` frame, the 321, 322 and 323
//! stream the demo answers, the cache a repeat reuses, the pending mask, and the idle
//! timeout with its stale end. Ports the controller cases in
//! `tui/internal/controller/channellist_test.go` and `wiring.go`'s list path. The demo
//! answers synchronously, so the timeout and pending cases stage a load by hand with
//! `ChannelListRequests::request` (the state a `LIST` leaves in flight).

use std::time::Duration;

use crate::controller::channellist::{ListOutcome, CHANNEL_LIST_TIMEOUT_TEXT};

use super::fixture::{Fixture, OMARCHY};

/// The channel names of the overlay, in its order.
fn names(fixture: &Fixture) -> Vec<String> {
    fixture
        .controller
        .channel_list()
        .rows
        .iter()
        .map(|row| row.name.clone())
        .collect()
}

#[test]
fn list_writes_the_frame_and_the_demo_streams_the_rows() {
    let mut fixture = Fixture::seeded();
    fixture.clear_frames(OMARCHY);

    assert_eq!(fixture.controller.list_channels(OMARCHY, "  "), ListOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["LIST\r\n"], "a blank mask lists everything");

    let view = fixture.controller.channel_list();
    assert!(view.open);
    assert!(view.complete && !view.loading, "323 completes the load");
    assert!(!view.rows.is_empty());
    assert_eq!(view.network_id, OMARCHY);
    assert!(
        view.rows
            .windows(2)
            .all(|pair| pair[0].users >= pair[1].users),
        "most users first"
    );
    assert!(names(&fixture).contains(&"#omarchy".to_owned()));
}

#[test]
fn a_mask_is_sent_and_filters_the_rows() {
    let mut fixture = Fixture::seeded();
    fixture.clear_frames(OMARCHY);

    assert_eq!(fixture.controller.list_channels(OMARCHY, "#desk*"), ListOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["LIST #desk*\r\n"]);
    let rows = names(&fixture);
    assert!(!rows.is_empty(), "the demo has channels that match");
    assert!(
        rows.iter().all(|name| name.starts_with("#desk")),
        "the demo keeps only the matching rows: {rows:?}"
    );
    assert_eq!(fixture.controller.channel_list().mask, "#desk*");
}

#[test]
fn a_repeat_with_the_same_mask_after_closing_is_cached_and_writes_nothing() {
    let mut fixture = Fixture::seeded();
    fixture.controller.list_channels(OMARCHY, "*");
    fixture.controller.dismiss_channel_list();
    fixture.clear_frames(OMARCHY);

    assert_eq!(fixture.controller.list_channels(OMARCHY, "  *  "), ListOutcome::Sent);
    assert!(fixture.frames(OMARCHY).is_empty(), "the cache answers, no LIST is written");
    let view = fixture.controller.channel_list();
    assert!(view.open && view.cached && view.complete);
    assert!(!names(&fixture).is_empty());
}

#[test]
fn a_repeat_while_the_overlay_is_open_refreshes_from_the_wire() {
    let mut fixture = Fixture::seeded();
    fixture.controller.list_channels(OMARCHY, "");
    fixture.clear_frames(OMARCHY);

    assert_eq!(fixture.controller.list_channels(OMARCHY, ""), ListOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["LIST\r\n"], "an open overlay asks again");
}

#[test]
fn a_list_on_an_unknown_or_unregistered_network_is_not_connected() {
    let mut fixture = Fixture::seeded();
    assert_eq!(
        fixture.controller.list_channels("nowhere", ""),
        ListOutcome::NotConnected
    );
    assert_eq!(fixture.controller.list_channels("", ""), ListOutcome::Refused);
    assert!(!fixture.controller.channel_list().open);
}

#[test]
fn a_pending_mask_starts_the_next_load_when_the_first_ends() {
    let mut fixture = Fixture::seeded();
    fixture.controller.channel_lists.request(OMARCHY, "#a", false);
    fixture.controller.list_channels(OMARCHY, "#b");
    fixture.clear_frames(OMARCHY);
    assert!(
        fixture.frames(OMARCHY).is_empty(),
        "a second mask while loading only becomes the pending mask"
    );

    fixture.deliver(OMARCHY, ":server 323 fred :End of /LIST");

    assert_eq!(fixture.frames(OMARCHY), ["LIST #b\r\n"]);
    let view = fixture.controller.channel_list();
    assert_eq!(view.mask, "#b");
    assert!(view.complete && !view.loading);
}

#[test]
fn an_idle_timeout_fails_the_load_and_the_stale_end_is_absorbed() {
    let mut fixture = Fixture::seeded();
    fixture.controller.channel_lists.request(OMARCHY, "*", false);
    fixture.controller.list_channels(OMARCHY, "*");

    fixture.clock.advance(Duration::from_secs(31));
    fixture.controller.tick();

    let state = fixture
        .controller
        .channel_lists
        .state(OMARCHY)
        .expect("the request is kept");
    assert!(!state.loading);
    assert_eq!(state.error, CHANNEL_LIST_TIMEOUT_TEXT);
    assert!(state.drain_timed_out_end, "the stale 323 will be drained");

    // The server's late 323 is absorbed, and the next load runs cleanly.
    fixture.deliver(OMARCHY, ":server 323 fred :End of /LIST");
    assert!(!fixture
        .controller
        .channel_lists
        .state(OMARCHY)
        .expect("kept")
        .drain_timed_out_end);

    assert_eq!(fixture.controller.list_channels(OMARCHY, "*"), ListOutcome::Sent);
    assert!(fixture.controller.channel_list().complete);
}

#[test]
fn a_zero_idle_timeout_is_floored_to_one_millisecond() {
    let mut fixture = Fixture::seeded();
    fixture
        .controller
        .channel_lists
        .set_idle_timeout(Duration::ZERO);
    fixture.controller.channel_lists.request(OMARCHY, "*", false);

    fixture.clock.advance(Duration::from_millis(1));
    fixture.controller.tick();
    let state = fixture
        .controller
        .channel_lists
        .state(OMARCHY)
        .expect("kept");
    assert!(!state.loading, "the 1 ms floor fired");
    assert_eq!(state.error, CHANNEL_LIST_TIMEOUT_TEXT);
}

#[test]
fn activity_restarts_the_idle_window() {
    let mut fixture = Fixture::seeded();
    fixture.controller.channel_lists.request(OMARCHY, "*", false);
    fixture.clock.advance(Duration::from_secs(20));
    fixture.deliver(OMARCHY, ":server 321 fred Channel :Users  Name");
    fixture.clock.advance(Duration::from_secs(20));
    fixture.controller.tick();
    assert!(
        fixture.controller.channel_lists.state(OMARCHY).expect("kept").loading,
        "321 at 20s re-armed the 30s window"
    );
}

#[test]
fn a_list_failure_numeric_names_the_error_on_the_overlay() {
    let mut fixture = Fixture::seeded();
    fixture.controller.channel_lists.request(OMARCHY, "*", false);
    fixture.controller.list_channels(OMARCHY, "*");
    fixture.controller.channel_lists.request(OMARCHY, "*", false);

    fixture.deliver(OMARCHY, ":server 416 fred LIST :Too many channel matches");

    let state = fixture
        .controller
        .channel_lists
        .state(OMARCHY)
        .expect("kept");
    assert!(!state.loading);
    assert_eq!(state.error, "Too many channel matches");
}
