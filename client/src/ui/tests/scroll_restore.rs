//! Shell-level proofs for a saved scroll place that waits for history. A place waits
//! only while a CHATHISTORY request for its conversation is in flight, gives up when
//! that request settles without the line, and a message sent to the conversation
//! retires the wait. Sidebar scrollbar drags leave the wait alone.

use gpui::{px, ListAlignment, ListState, Modifiers, ScrollHandle};

use crate::model::scroll_place::SavedPlace;
use crate::storage::scrollplace::ScrollPlace;
use crate::ui::scrollbar::{ScrollbarDrag, ScrollbarDragKind};

use super::fixture::{self, SeededShell, OMARCHY, WINDOW_HEIGHT, WINDOW_WIDTH};

/// A place on a message the transcript does not hold, as a conversation left it earlier.
fn gone_place() -> ScrollPlace {
    ScrollPlace {
        msg_id: "gone-anchor".to_owned(),
        ..ScrollPlace::default()
    }
}

/// Offers CHATHISTORY on the demo's omarchy network, as a server does after CAP NEW.
fn offer_chathistory(ui: &mut SeededShell) {
    ui.inject(OMARCHY, ":server CAP omairc NEW :chathistory");
    ui.inject(OMARCHY, ":server CAP omairc ACK :chathistory");
    assert!(
        ui.shell
            .read_with(ui.cx, |shell, _| shell.controller.chat_history_enabled()),
        "the demo network takes CHATHISTORY"
    );
}

/// Asks for a page of the selected conversation's history, so its request is in flight.
fn request_history(ui: &mut SeededShell) {
    let asked = ui.shell.update(ui.cx, |shell, _| {
        shell.controller.request_older_transcript_history()
    });
    assert!(
        asked,
        "a history page goes out for the selected conversation"
    );
}

/// Leaves the open conversation at `gone_place`, then comes back to `key`, so it opens
/// at that saved place.
fn reopen_at_gone_place(ui: &mut SeededShell, key: &str) {
    ui.select("#help");
    ui.shell.update(ui.cx, |shell, _| {
        shell.controller.remember_place(OMARCHY, key, gone_place());
    });
    ui.select(key);
}

/// The open conversation with a request in flight, reopened at a place it does not hold.
fn waiting_for_history(ui: &mut SeededShell) {
    offer_chathistory(ui);
    ui.select("anna");
    request_history(ui);
    reopen_at_gone_place(ui, "anna");
    assert!(
        waiting(ui),
        "the place waits while its history is in flight"
    );
}

fn waiting(ui: &SeededShell) -> bool {
    ui.shell.read_with(ui.cx, |shell, cx| {
        shell.transcript.read(cx).has_pending_restore()
    })
}

/// Whether the transcript last reported that it follows the end.
fn reported_end(ui: &SeededShell) -> bool {
    ui.shell
        .read_with(ui.cx, |shell, _| shell.controller.transcript_follow_end())
}

/// Delivers one server line to the omarchy network and runs the tick the UI loop runs,
/// without rendering.
fn deliver_unrendered(ui: &mut SeededShell, line: &str) {
    ui.shell.update(ui.cx, |shell, cx| {
        let mut bytes = line.as_bytes().to_vec();
        bytes.extend_from_slice(b"\r\n");
        shell
            .controller
            .demo()
            .expect("demo controller")
            .inject(OMARCHY, &bytes);
        shell.tick(cx);
    });
}

fn deliver(ui: &mut SeededShell, line: &str) {
    deliver_unrendered(ui, line);
    ui.settle();
}

/// Types one line in the composer and presses Enter.
fn send_line(ui: &mut SeededShell, text: &str) {
    submit_line(ui, text);
    ui.settle();
}

/// Types one line in the composer and presses Enter, without rendering afterwards.
fn submit_line(ui: &mut SeededShell, text: &str) {
    ui.cx.update(|window, cx| {
        ui.shell.update(cx, |shell, cx| {
            shell.composer.update(cx, |composer, cx| {
                composer.set_text(text.to_owned());
                cx.notify();
            });
            shell.composer_submit(window, cx);
        });
    });
}

#[gpui::test]
fn a_saved_line_does_not_wait_when_no_history_request_is_in_flight(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    offer_chathistory(&mut ui);
    ui.select("anna");
    ui.shell.update(ui.cx, |shell, cx| {
        shell
            .controller
            .remember_place(OMARCHY, "anna", gone_place());
        // Checked before any drain runs, so a place that waits and is given up later still fails here.
        shell.show_shown_view(None, cx);
        assert!(
            !shell.transcript.read(cx).has_pending_restore(),
            "history already settled, so nothing will bring the line and the place must not wait"
        );
    });
}

#[gpui::test]
fn a_gone_line_with_nothing_in_flight_lands_on_its_row_not_the_end(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    offer_chathistory(&mut ui);
    ui.select("anna");
    ui.fill_until_scrollable("anna", "fred", "landing line");
    let gone = SavedPlace::from_stored(&gone_place()).expect("a message place");
    ui.shell.update(ui.cx, |shell, cx| {
        shell.show_shown_view(Some(gone), cx);
    });
    ui.settle();
    assert!(!waiting(&ui), "nothing is in flight, so nothing waits");
    assert!(
        !ui.at_end(),
        "a gone line lands on its saved row, not at the end of the transcript"
    );
}

#[gpui::test]
fn a_waiting_place_stays_while_unrelated_lines_arrive(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_for_history(&mut ui);
    deliver(&mut ui, ":anna!u@h PRIVMSG fred :meanwhile");
    deliver(&mut ui, ":anna!u@h PRIVMSG fred :still meanwhile");
    assert!(
        waiting(&ui),
        "the request is still in flight, so the place keeps waiting"
    );
}

#[gpui::test]
fn a_history_request_that_settles_without_the_line_gives_the_wait_up(
    cx: &mut gpui::TestAppContext,
) {
    let mut ui = fixture::open(cx);
    waiting_for_history(&mut ui);
    deliver(
        &mut ui,
        ":server FAIL CHATHISTORY INVALID_TARGET BEFORE anna :no",
    );
    assert!(
        !waiting(&ui),
        "the request for this conversation settled, so the place lands without the line"
    );
}

#[gpui::test]
fn another_conversations_request_does_not_hold_the_wait(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    offer_chathistory(&mut ui);
    ui.select("dax");
    request_history(&mut ui);
    ui.select("anna");
    request_history(&mut ui);
    reopen_at_gone_place(&mut ui, "anna");
    assert!(waiting(&ui));
    deliver(
        &mut ui,
        ":server FAIL CHATHISTORY INVALID_TARGET BEFORE anna :no",
    );
    assert!(
        !waiting(&ui),
        "dax still has a request in flight, but anna's own request settled"
    );
}

#[gpui::test]
fn lines_that_land_while_a_place_waits_do_not_raise_the_unseen_chip(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    offer_chathistory(&mut ui);
    ui.select("anna");
    ui.fill_until_scrollable("anna", "fred", "waiting line");
    request_history(&mut ui);
    reopen_at_gone_place(&mut ui, "anna");
    assert!(
        waiting(&ui),
        "the place waits while its history is in flight"
    );
    deliver_unrendered(&mut ui, ":anna!u@h PRIVMSG fred :a line from history");
    ui.page_up();
    assert!(!waiting(&ui), "scrolling up gives the wait up");
    assert_eq!(
        ui.first_unseen(),
        None,
        "a line that landed while the place waited does not arm the unseen chip"
    );
}

#[gpui::test]
fn a_restore_that_lands_does_not_report_the_end(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_for_history(&mut ui);
    assert!(
        !reported_end(&ui),
        "a waiting place reports that it is not at the end"
    );
    deliver(
        &mut ui,
        "@msgid=gone-anchor :anna!u@h PRIVMSG fred :the saved line",
    );
    assert!(!waiting(&ui), "the saved line arrived, so the place lands");
    assert!(
        !reported_end(&ui),
        "landing on a saved line must not report the end, or the read marker advances"
    );
}

#[gpui::test]
fn a_command_keeps_a_waiting_place_but_a_message_here_retires_it(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_for_history(&mut ui);
    send_line(&mut ui, "/pref");
    assert!(
        waiting(&ui),
        "a command that is not a message to this conversation keeps the place waiting"
    );
    submit_line(&mut ui, "hello from the reader");
    assert!(
        !waiting(&ui),
        "a message to this conversation retires the wait so the live page is saved"
    );
    assert!(
        reported_end(&ui),
        "the send itself reports the end to the read marker, before any render"
    );
}

#[gpui::test]
fn a_sidebar_scrollbar_drag_keeps_a_waiting_place(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_for_history(&mut ui);
    let centre = gpui::point(px(WINDOW_WIDTH / 2.), px(WINDOW_HEIGHT / 2.));

    set_drag(&mut ui, ScrollbarDragKind::Div(ScrollHandle::new()));
    ui.cx
        .simulate_mouse_move(centre, None, Modifiers::default());
    ui.settle();
    assert!(
        waiting(&ui),
        "a sidebar or member scrollbar drag does not retire the wait"
    );

    set_drag(
        &mut ui,
        ScrollbarDragKind::List(ListState::new(0, ListAlignment::Top, px(0.))),
    );
    ui.cx
        .simulate_mouse_move(centre, None, Modifiers::default());
    ui.settle();
    assert!(
        !waiting(&ui),
        "dragging the transcript's own scrollbar retires the wait"
    );
}

/// Starts a scrollbar drag on the shell, as a press on a thumb does.
fn set_drag(ui: &mut SeededShell, kind: ScrollbarDragKind) {
    ui.shell.update(ui.cx, |shell, _| {
        *shell.scrollbar_drag.borrow_mut() = Some(ScrollbarDrag {
            kind,
            pointer_offset: px(0.),
        });
    });
}

/// Scrolls the transcript with the wheel over its middle. Positive pixels move up.
fn wheel(ui: &mut SeededShell, pixels: f32) {
    let centre = gpui::point(px(WINDOW_WIDTH / 2.), px(WINDOW_HEIGHT / 2.));
    ui.cx.simulate_event(gpui::ScrollWheelEvent {
        position: centre,
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(pixels))),
        modifiers: Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    ui.settle();
}

#[gpui::test]
fn a_wheel_back_to_the_end_reports_the_end(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("anna");
    ui.fill_until_scrollable("anna", "fred", "wheel line");
    ui.page_up();
    assert!(!reported_end(&ui), "a page up leaves the end");
    wheel(&mut ui, -6000.);
    assert!(
        reported_end(&ui),
        "a wheel back to the end reports it, so the read marker can advance again"
    );
}
