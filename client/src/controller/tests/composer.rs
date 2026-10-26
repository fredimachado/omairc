//! The composer's typing hint over the real demo. Ports the Go cases in
//! `tui/internal/controller/composertyping_test.go`. The demo grants `message-tags`,
//! so a live line writes `@+typing=active TAGMSG` and the hint is withdrawn with
//! `done`. The pace is the session's three-second throttle on the fixture's clock.

use std::time::Duration;

use crate::irc::typing::TYPING_SEND_INTERVAL_MS;

use super::fixture::{Fixture, OMARCHY};

const ACTIVE: &str = "@+typing=active TAGMSG #omarchy\r\n";
const DONE: &str = "@+typing=done TAGMSG #omarchy\r\n";

/// How many TAGMSG frames the loopback wrote since the last clear.
fn tagmsg_count(fixture: &Fixture) -> usize {
    fixture
        .frames(OMARCHY)
        .iter()
        .filter(|frame| frame.contains("TAGMSG"))
        .count()
}

/// The fixture with `#omarchy` selected and its frames cleared.
fn selected_channel() -> Fixture {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    fixture
}

fn advance_past_the_pace(fixture: &Fixture) {
    fixture
        .clock
        .advance(Duration::from_millis(TYPING_SEND_INTERVAL_MS as u64));
}

#[test]
fn a_live_line_publishes_active_once_per_pace() {
    let mut fixture = selected_channel();

    fixture.controller.composer_edited(OMARCHY, "#omarchy", "hello");
    assert_eq!(fixture.frames(OMARCHY), [ACTIVE]);
    let first = tagmsg_count(&fixture);

    fixture.controller.composer_edited(OMARCHY, "#omarchy", "hello there");
    assert_eq!(tagmsg_count(&fixture), first, "throttled inside the pace");
}

#[test]
fn a_non_live_line_withdraws_the_hint_and_clears_the_target() {
    let mut fixture = selected_channel();
    fixture.controller.composer_edited(OMARCHY, "#omarchy", "hello");
    let first = tagmsg_count(&fixture);

    // A slash command is not a live message. The done is inside the pace, so nothing
    // new reaches the wire, but the target is released.
    fixture.controller.composer_edited(OMARCHY, "#omarchy", "/join #other");
    assert_eq!(tagmsg_count(&fixture), first, "done throttled inside the pace");
    assert_eq!(fixture.controller.typing_target(), None);

    // With the target released, a second non-live line cannot send again.
    advance_past_the_pace(&fixture);
    fixture.controller.composer_edited(OMARCHY, "#omarchy", "/join #other");
    assert_eq!(tagmsg_count(&fixture), first, "no target, no done");

    // A live line re-arms the target and writes an active hint again.
    fixture.controller.composer_edited(OMARCHY, "#omarchy", "next");
    assert_eq!(tagmsg_count(&fixture), first + 1);
    assert_eq!(fixture.frames(OMARCHY).last().map(String::as_str), Some(ACTIVE));
}

#[test]
fn a_slash_me_is_live_and_other_slash_lines_are_not() {
    let mut fixture = selected_channel();
    fixture.controller.composer_edited(OMARCHY, "#omarchy", "/me waves");
    assert_eq!(fixture.frames(OMARCHY), [ACTIVE]);

    advance_past_the_pace(&fixture);
    fixture.controller.composer_edited(OMARCHY, "#omarchy", "//escaped");
    assert_eq!(
        fixture.frames(OMARCHY).last().map(String::as_str),
        Some(ACTIVE),
        "a doubled slash is text, so it stays live"
    );
}

#[test]
fn clearing_after_a_sent_message_writes_no_done() {
    let mut fixture = selected_channel();
    fixture.controller.composer_edited(OMARCHY, "#omarchy", "hello");
    assert_eq!(fixture.frames(OMARCHY), [ACTIVE]);

    advance_past_the_pace(&fixture);
    assert!(fixture.controller.send_to_target(OMARCHY, "#omarchy", "hello"));
    let before = tagmsg_count(&fixture);

    fixture.controller.composer_edited(OMARCHY, "#omarchy", "");
    assert_eq!(tagmsg_count(&fixture), before, "the done is suppressed after chat");
    assert_eq!(fixture.controller.typing_target(), None);
}

#[test]
fn moving_to_another_conversation_withdraws_the_hint_on_the_old_one() {
    let mut fixture = selected_channel();
    fixture.controller.composer_edited(OMARCHY, "#omarchy", "hello");
    assert_eq!(fixture.controller.typing_target(), Some((OMARCHY, "#omarchy")));

    advance_past_the_pace(&fixture);
    fixture.controller.select_conversation(OMARCHY, "#desktop");

    assert_eq!(fixture.frames(OMARCHY).last().map(String::as_str), Some(DONE));
    assert_eq!(fixture.controller.typing_target(), None);
}

#[test]
fn clearing_the_selection_withdraws_the_hint() {
    let mut fixture = selected_channel();
    fixture.controller.composer_edited(OMARCHY, "#omarchy", "hello");
    advance_past_the_pace(&fixture);

    fixture.controller.clear_selection();
    assert_eq!(fixture.frames(OMARCHY).last().map(String::as_str), Some(DONE));
    assert_eq!(fixture.controller.typing_target(), None);
}

#[test]
fn nothing_publishes_on_status_or_without_message_tags() {
    let mut fixture = selected_channel();
    fixture.controller.open_status(OMARCHY);
    fixture.controller.composer_edited(OMARCHY, "#omarchy", "hello");
    assert_eq!(tagmsg_count(&fixture), 0, "the Status console never publishes");

    fixture.controller.leave_status();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    fixture
        .controller
        .capabilities
        .insert(OMARCHY.to_owned(), Default::default());
    fixture.controller.composer_edited(OMARCHY, "#omarchy", "hello");
    assert_eq!(tagmsg_count(&fixture), 0, "no message-tags, no hint");
    assert_eq!(fixture.controller.typing_target(), None);
}

#[test]
fn a_composer_for_another_target_publishes_nothing() {
    let mut fixture = selected_channel();
    fixture.controller.composer_edited(OMARCHY, "#desktop", "hello");
    assert_eq!(tagmsg_count(&fixture), 0, "the hint follows the selection");
}
