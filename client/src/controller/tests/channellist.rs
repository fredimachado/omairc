//! `/list` through the wire over the real demo: the `LIST` frame, the 321, 322 and 323
//! stream the demo answers, the cache a repeat reuses, the pending mask, and the idle
//! timeout with its PING fence and stale end.
//! `tui/internal/controller/channellist_test.go` and `wiring.go`'s list path. The demo
//! answers synchronously, so the timeout and pending cases stage a load by hand with
//! `ChannelListRequests::request` (the state a `LIST` leaves in flight).

use std::time::Duration;

use crate::controller::channellist::{
    ChannelListPhase, ListOutcome, ListedChannel, CHANNEL_LIST_TIMEOUT_TEXT,
};

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

/// The request's phase for `OMARCHY`. The assertions read it only through these helpers.
fn phase(fixture: &Fixture) -> &ChannelListPhase {
    fixture.controller.channel_lists.phase(OMARCHY)
}

fn loading(fixture: &Fixture) -> bool {
    phase(fixture).is_loading()
}

fn complete(fixture: &Fixture) -> bool {
    phase(fixture).is_complete()
}

fn error(fixture: &Fixture) -> String {
    phase(fixture).error().to_owned()
}

/// The fence whose PONG is still to come, while a stale end may still arrive.
fn fence(fixture: &Fixture) -> Option<String> {
    match phase(fixture) {
        ChannelListPhase::Loading { stale_fence, .. }
        | ChannelListPhase::Failed { stale_fence, .. } => stale_fence.clone(),
        ChannelListPhase::Idle | ChannelListPhase::Complete { .. } => None,
    }
}

fn drain(fixture: &Fixture) -> bool {
    fence(fixture).is_some()
}

fn queued(fixture: &Fixture) -> Option<String> {
    match phase(fixture) {
        ChannelListPhase::Loading { queued_mask, .. } => queued_mask.clone(),
        _ => None,
    }
}

fn rows(fixture: &Fixture) -> Vec<ListedChannel> {
    phase(fixture).rows().to_vec()
}

fn row_names(fixture: &Fixture) -> Vec<String> {
    rows(fixture).into_iter().map(|row| row.name).collect()
}

/// The fence token the staged timeouts use. The demo answers a real PING at once, so
/// these tests stage the timeout by hand and deliver the PONG themselves.
const FENCE: &str = "omairc-list-staged";

/// Stages a load that timed out with its fence unanswered, so its stale end may still
/// come.
fn timed_out_load(fixture: &mut Fixture) {
    fixture
        .controller
        .channel_lists
        .request(OMARCHY, "*", false);
    fixture.controller.channel_lists.fail(
        OMARCHY,
        CHANNEL_LIST_TIMEOUT_TEXT,
        Some(FENCE.to_owned()),
    );
}

fn pong(fixture: &mut Fixture, token: &str) {
    fixture.deliver(OMARCHY, &format!(":server PONG irc.example :{token}"));
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
fn a_load_is_never_loading_and_complete() {
    let mut fixture = Fixture::seeded();
    fixture.controller.channel_lists.request(OMARCHY, "*", false);
    assert!(loading(&fixture) && !complete(&fixture), "a started load is loading only");

    fixture.deliver(OMARCHY, ":server 323 fred :End of /LIST");
    assert!(!loading(&fixture) && complete(&fixture), "323 completes it and it stops loading");

    fixture.controller.channel_lists.request(OMARCHY, "*", true);
    assert!(loading(&fixture) && !complete(&fixture), "a refresh is loading again, not complete");

    fixture.deliver(OMARCHY, ":server 416 fred LIST :Too many channel matches");
    assert!(!loading(&fixture) && !complete(&fixture), "a failed load is not complete");
    assert_eq!(error(&fixture), "Too many channel matches");
}

#[test]
fn a_second_mask_while_loading_queues_once_and_the_same_mask_clears_it() {
    let mut fixture = Fixture::seeded();
    fixture.controller.channel_lists.request(OMARCHY, "#a", false);
    fixture.clear_frames(OMARCHY);

    assert_eq!(fixture.controller.list_channels(OMARCHY, "#b"), ListOutcome::Sent);
    assert_eq!(queued(&fixture).as_deref(), Some("#b"));
    assert_eq!(fixture.controller.list_channels(OMARCHY, "#c"), ListOutcome::Sent);
    assert_eq!(
        queued(&fixture).as_deref(),
        Some("#c"),
        "one slot: the latest mask replaces the queued one"
    );
    assert!(
        fixture.frames(OMARCHY).is_empty(),
        "no second LIST while #a is in flight"
    );

    fixture.controller.list_channels(OMARCHY, "#A");
    assert_eq!(queued(&fixture), None, "the in-flight mask, in any case, clears the queue");
    assert!(loading(&fixture));
    assert!(fixture.frames(OMARCHY).is_empty());
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
    timed_out_load(&mut fixture);

    assert!(!loading(&fixture) && !complete(&fixture));
    assert_eq!(error(&fixture), CHANNEL_LIST_TIMEOUT_TEXT);
    assert!(drain(&fixture), "a 323 before the PONG is the stale one");

    // The server's late 323 is absorbed, and the next load runs cleanly.
    fixture.deliver(OMARCHY, ":server 323 fred :End of /LIST");
    assert!(!drain(&fixture));
    assert_eq!(error(&fixture), CHANNEL_LIST_TIMEOUT_TEXT, "the failed load stays failed");
    pong(&mut fixture, FENCE);

    assert_eq!(fixture.controller.list_channels(OMARCHY, "*"), ListOutcome::Sent);
    assert!(fixture.controller.channel_list().complete);
}

#[test]
fn the_fence_pong_ends_the_drain_and_the_next_end_completes() {
    let mut fixture = Fixture::seeded();
    timed_out_load(&mut fixture);

    pong(&mut fixture, "omairc-list-other");
    assert!(drain(&fixture), "another fence's PONG changes nothing");
    pong(&mut fixture, FENCE);
    assert!(!drain(&fixture), "no answer to the timed-out LIST can follow its fence");

    fixture.controller.channel_lists.request(OMARCHY, "#new", false);
    assert!(!drain(&fixture));
    fixture.deliver(OMARCHY, ":server 323 fred :End of /LIST");
    assert!(complete(&fixture), "the first 323 after the PONG is the load's own");
}

#[test]
fn a_pong_after_the_newer_list_still_hands_the_end_to_it() {
    // The server answers in order: the fence's PONG comes before the newer LIST's
    // rows and 323, even when the newer LIST went out before the PONG came back.
    let mut fixture = Fixture::seeded();
    timed_out_load(&mut fixture);
    fixture.controller.channel_lists.request(OMARCHY, "#new", false);
    assert!(drain(&fixture), "the newer load carries the unanswered fence");

    pong(&mut fixture, FENCE);
    fixture.deliver(OMARCHY, ":server 322 fred #new 3 :new channel");
    fixture.deliver(OMARCHY, ":server 323 fred :End of /LIST");
    assert!(complete(&fixture) && !loading(&fixture));
    assert_eq!(row_names(&fixture), ["#new"]);
}

#[test]
fn a_list_after_a_timeout_completes_on_its_own_prompt_end() {
    // The timed-out LIST never gets a late 323. The demo answers every later LIST at
    // once, with its rows and its 323, the way Ergo does.
    let mut fixture = Fixture::seeded();
    fixture.controller.list_channels(OMARCHY, "*");
    fixture.controller.channel_lists.request(OMARCHY, "*", true);
    fixture.clock.advance(Duration::from_secs(31));
    fixture.controller.tick();
    assert_eq!(error(&fixture), CHANNEL_LIST_TIMEOUT_TEXT);

    for attempt in 1..=3 {
        fixture.clear_frames(OMARCHY);
        assert_eq!(fixture.controller.list_channels(OMARCHY, "*"), ListOutcome::Sent);
        assert_eq!(fixture.frames(OMARCHY), ["LIST *\r\n"], "attempt {attempt}");
        let view = fixture.controller.channel_list();
        assert!(
            view.complete && !view.loading && view.error.is_empty(),
            "attempt {attempt}: the prompt 323 completes the load: {view:?}"
        );
        assert!(!view.rows.is_empty(), "attempt {attempt}");
    }
}

#[test]
fn a_timeout_writes_a_ping_fence_and_its_pong_ends_the_drain() {
    let mut fixture = Fixture::seeded();
    fixture.controller.channel_lists.request(OMARCHY, "*", false);
    fixture.clear_frames(OMARCHY);
    fixture.clock.advance(Duration::from_secs(31));
    fixture.controller.tick();

    let frames = fixture.frames(OMARCHY);
    assert_eq!(frames.len(), 1, "one fence: {frames:?}");
    assert!(
        frames[0].starts_with("PING :omairc-list-"),
        "the timeout fences the wire: {frames:?}"
    );
    // The demo answers the PING at once, so no stale end can still come.
    assert!(!drain(&fixture), "the PONG ended the drain");
    assert_eq!(error(&fixture), CHANNEL_LIST_TIMEOUT_TEXT);
}

#[test]
fn a_stale_end_after_a_timeout_does_not_complete_the_newer_load() {
    let mut fixture = Fixture::seeded();
    timed_out_load(&mut fixture);
    assert!(!loading(&fixture) && drain(&fixture));

    // A newer load starts. It is staged, so the demo does not answer it. The timed-out
    // load's late row and stale 323 arrive before the fence's PONG: the row is dropped,
    // the 323 is drained, and the newer load keeps going.
    fixture.controller.channel_lists.request(OMARCHY, "#new", false);
    assert!(drain(&fixture), "the newer load carries the pending drain");
    fixture.deliver(OMARCHY, ":server 322 fred #old 9 :old channel");
    fixture.deliver(OMARCHY, ":server 323 fred :End of /LIST");
    assert!(loading(&fixture) && !complete(&fixture), "the stale end does not complete #new");
    assert!(!drain(&fixture));
    assert!(rows(&fixture).is_empty(), "the stale row is not #new's");
    pong(&mut fixture, FENCE);
    assert!(loading(&fixture), "the late PONG changes nothing");

    // The real end of #new completes it, with the row that streamed in.
    fixture.deliver(OMARCHY, ":server 322 fred #new 3 :new channel");
    fixture.deliver(OMARCHY, ":server 323 fred :End of /LIST");
    assert!(complete(&fixture) && !loading(&fixture));
    assert_eq!(row_names(&fixture), ["#new"]);
}

#[test]
fn a_completed_newer_mask_carries_no_drain_flag() {
    let mut fixture = Fixture::seeded();
    timed_out_load(&mut fixture);

    fixture.controller.channel_lists.request(OMARCHY, "#x", false);
    fixture.deliver(OMARCHY, ":server 323 fred :End of /LIST");
    assert!(loading(&fixture) && !drain(&fixture), "the stale end is drained");
    fixture.deliver(OMARCHY, ":server 323 fred :End of /LIST");
    assert!(complete(&fixture) && !drain(&fixture), "the real end completes #x");

    // The next load is not swallowed: its first end completes it.
    fixture.controller.channel_lists.request(OMARCHY, "#y", false);
    fixture.deliver(OMARCHY, ":server 323 fred :End of /LIST");
    assert!(complete(&fixture) && !loading(&fixture));
}

#[test]
fn a_failed_load_keeps_its_rows_and_error() {
    let mut fixture = Fixture::seeded();
    fixture.controller.channel_lists.request(OMARCHY, "*", false);
    fixture.deliver(OMARCHY, ":server 322 fred #kept 4 :a kept channel");
    fixture.clock.advance(Duration::from_secs(31));
    fixture.controller.tick();

    assert!(!loading(&fixture) && !complete(&fixture));
    assert_eq!(error(&fixture), CHANNEL_LIST_TIMEOUT_TEXT);
    assert_eq!(row_names(&fixture), ["#kept"]);
    assert_eq!(fixture.controller.channel_list().error, "", "the overlay was not on this load");
}

#[test]
fn a_non_timeout_failure_keeps_a_stale_end_still_to_drain() {
    // Pinned as it behaves today: a failure that is not a timeout keeps the drain of the
    // timed-out load it replaced, and the stale end is absorbed by the failed state.
    let mut fixture = Fixture::seeded();
    timed_out_load(&mut fixture);

    fixture.controller.channel_lists.request(OMARCHY, "#x", false);
    fixture.deliver(OMARCHY, ":server 416 fred LIST :Too many channel matches");
    assert!(!loading(&fixture) && !complete(&fixture) && drain(&fixture));

    fixture.deliver(OMARCHY, ":server 323 fred :End of /LIST");
    assert!(!drain(&fixture) && !complete(&fixture) && !loading(&fixture));
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
    assert!(!loading(&fixture), "the 1 ms floor fired");
    assert_eq!(error(&fixture), CHANNEL_LIST_TIMEOUT_TEXT);
}

#[test]
fn activity_restarts_the_idle_window() {
    let mut fixture = Fixture::seeded();
    fixture.controller.channel_lists.request(OMARCHY, "*", false);
    fixture.clock.advance(Duration::from_secs(20));
    fixture.deliver(OMARCHY, ":server 321 fred Channel :Users  Name");
    fixture.clock.advance(Duration::from_secs(20));
    fixture.controller.tick();
    assert!(loading(&fixture), "321 at 20s re-armed the 30s window");
}

#[test]
fn a_list_failure_numeric_names_the_error_on_the_overlay() {
    let mut fixture = Fixture::seeded();
    fixture.controller.channel_lists.request(OMARCHY, "*", false);
    fixture.controller.list_channels(OMARCHY, "*");
    fixture.controller.channel_lists.request(OMARCHY, "*", false);

    fixture.deliver(OMARCHY, ":server 416 fred LIST :Too many channel matches");

    assert!(!loading(&fixture));
    assert_eq!(error(&fixture), "Too many channel matches");
    assert_eq!(
        fixture.controller.channel_list().error,
        "Too many channel matches"
    );
}
