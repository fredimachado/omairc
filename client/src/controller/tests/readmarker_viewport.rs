//! The read-marker gate follows the viewport the shell reports: focused and
//! following the end.

use super::fixture::{Fixture, OMARCHY};
use super::readmarker::{
    count_containing, deliver_privmsg, enable_read_marker, ensure_room, frames_contain, when,
};

const NET: &str = OMARCHY;

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
fn follow_end_false_blocks_publish_including_pending_restore() {
    let mut fixture = Fixture::seeded();
    enable_read_marker(&mut fixture);
    fixture.controller.note_transcript_viewport(true, true);
    ensure_room(&mut fixture, "#room");
    fixture.controller.select_conversation(NET, "#room");
    fixture.clear_frames(NET);
    // A pending scroll restore reports follow_end = false to the shell.
    fixture.controller.note_transcript_viewport(true, false);
    let at = when(2024, 6, 1, 12, 0, 0, 123);
    deliver_privmsg(&mut fixture, "#room", "peer", "hi", at);
    assert_eq!(count_containing(&fixture.frames(NET), "MARKREAD"), 0);
    fixture.controller.note_transcript_viewport(true, false);
    assert_eq!(count_containing(&fixture.frames(NET), "MARKREAD"), 0);
    fixture.controller.note_transcript_viewport(true, true);
    assert_eq!(
        count_containing(&fixture.frames(NET), "MARKREAD #room timestamp=2024-06-01T12:00:00.123Z\r\n"),
        1,
        "frames = {:?}",
        fixture.frames(NET)
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
