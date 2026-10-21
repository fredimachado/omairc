//! The saved scroll place over the seeded transcript. Ports the controller cases behind
//! `tui/internal/controller/scrollplace.go`: a place is remembered for the selected
//! conversation and found again by its anchor, a bottom pin is kept as `follow`, the
//! Status console saves nothing, and an anchor outside the transcript is not saved.

use super::fixture::{Fixture, OMARCHY};

#[test]
fn an_anchor_row_is_remembered_and_found_again() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(fixture.controller.messages().len() > 2, "the seed has a transcript");

    fixture.controller.remember_scroll_place(false, Some(2));

    let view = fixture.controller.current_scroll_place();
    assert!(view.known);
    assert!(!view.follow);
    assert_eq!(view.row, Some(2), "the anchor is found at its row");
}

#[test]
fn a_bottom_pin_is_kept_as_follow() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");

    fixture.controller.remember_scroll_place(true, None);

    let view = fixture.controller.current_scroll_place();
    assert!(view.known && view.follow);
    assert_eq!(view.row, None);
}

#[test]
fn the_status_console_saves_nothing() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.controller.open_status(OMARCHY);

    fixture.controller.remember_scroll_place(true, None);

    fixture.controller.leave_status();
    assert!(
        !fixture.controller.current_scroll_place().known,
        "nothing was saved while Status was open"
    );
}

#[test]
fn an_anchor_outside_the_transcript_is_not_saved() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    let past_the_end = fixture.controller.messages().len();

    fixture.controller.remember_scroll_place(false, Some(past_the_end));
    fixture.controller.remember_scroll_place(false, None);

    assert!(!fixture.controller.current_scroll_place().known);
}

#[test]
fn a_place_is_kept_per_conversation() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.controller.remember_scroll_place(false, Some(1));

    fixture.controller.select_conversation(OMARCHY, "#desktop");
    assert!(
        !fixture.controller.current_scroll_place().known,
        "#desktop has no place of its own"
    );

    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert_eq!(fixture.controller.current_scroll_place().row, Some(1));
}
