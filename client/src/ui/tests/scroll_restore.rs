//! Shell-level proofs for a saved scroll place that waits for history. A place waits
//! only while a CHATHISTORY request for its conversation is in flight, gives up when
//! that request settles without the line, and a message sent to the conversation
//! retires the wait. Sidebar scrollbar drags leave the wait alone.

use std::time::Duration;

use chrono::{TimeZone, Utc};
use gpui::{px, ListAlignment, ListState, Modifiers, ScrollHandle};

use crate::model::scroll_place::SavedPlace;
use crate::session::session::HISTORY_LIMIT;
use crate::storage::scrollplace::ScrollPlace;
use crate::ui::scrollbar::{ScrollbarDrag, ScrollbarDragKind};
use crate::ui::shell::notifications::RESTORE_WAIT;

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

/// Delivers each server line, running the tick after each, inside one update. Nothing
/// renders between the lines, so each tick drains with the list as the last one left it.
fn deliver_burst<S: AsRef<str>>(ui: &mut SeededShell, lines: &[S]) {
    ui.shell.update(ui.cx, |shell, cx| {
        for line in lines {
            let mut bytes = line.as_ref().as_bytes().to_vec();
            bytes.extend_from_slice(b"\r\n");
            shell
                .controller
                .demo()
                .expect("demo controller")
                .inject(OMARCHY, &bytes);
            shell.tick(cx);
        }
    });
    ui.settle();
}

fn deliver(ui: &mut SeededShell, line: &str) {
    deliver_unrendered(ui, line);
    ui.settle();
}

/// Runs the tick the UI loop runs, without delivering a line.
fn tick(ui: &mut SeededShell) {
    ui.shell.update(ui.cx, |shell, cx| shell.tick(cx));
    ui.settle();
}

/// Moves the controller's fake clock forward, then runs the tick that follows it.
fn advance(ui: &mut SeededShell, by: Duration) {
    ui.shell
        .read_with(ui.cx, |shell, _| shell.controller.advance_test_clock(by));
    tick(ui);
}

/// The wait's deadline as a std duration, so a test can stop just before it.
fn wait_window() -> Duration {
    RESTORE_WAIT.to_std().expect("the wait is a positive span")
}

/// One CHATHISTORY page of anna's lines, one per msgid, as the server sends it. The lines
/// carry an old time, so the page sorts before the demo's own lines.
fn anna_page<S: AsRef<str>>(reference: &str, msgids: &[S]) -> Vec<String> {
    let mut lines = vec![format!(":server BATCH +{reference} chathistory anna")];
    for msgid in msgids {
        let msgid = msgid.as_ref();
        lines.push(format!(
            "@batch={reference};msgid={msgid};time=2011-10-19T16:40:51.000Z :anna!u@h PRIVMSG fred :{msgid}"
        ));
    }
    lines.push(format!(":server BATCH -{reference}"));
    lines
}

/// The msgid of the row at the top of the transcript.
fn top_msgid(ui: &SeededShell) -> String {
    ui.shell.read_with(ui.cx, |shell, cx| {
        let transcript = shell.transcript.read(cx);
        let top = transcript.logical_scroll_top().item_ix;
        transcript
            .rows()
            .get(top)
            .map(|row| row.msgid.clone())
            .unwrap_or_default()
    })
}

/// Starts a CHATHISTORY AFTER chain for the selected conversation, as a catch-up does.
fn start_after_chain(ui: &mut SeededShell) {
    let when = Utc.with_ymd_and_hms(2024, 3, 9, 16, 0, 0).unwrap();
    let started = ui.shell.update(ui.cx, |shell, _| {
        shell.controller.start_history_after_for_tests(when)
    });
    assert!(
        started,
        "an AFTER page goes out for the selected conversation"
    );
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
fn a_second_drain_before_a_frame_keeps_the_restored_place(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_for_history(&mut ui);
    // Both drains run inside one update: gpui's test harness draws dirty windows when an
    // update ends, so only one update keeps the list unlaid-out between the two.
    deliver_burst(
        &mut ui,
        &[
            "@msgid=gone-anchor :anna!u@h PRIVMSG fred :the saved line",
            ":anna!u@h PRIVMSG fred :a line after it",
        ],
    );
    assert!(!waiting(&ui), "the saved line arrived, so the place lands");
    assert!(
        !reported_end(&ui),
        "a second drain before a frame must keep the restored place, not report the end"
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
fn an_untargeted_fail_holds_the_wait_only_until_its_deadline(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_for_history(&mut ui);
    tick(&mut ui);
    deliver(
        &mut ui,
        ":server FAIL CHATHISTORY INVALID_PARAMS LATEST :no",
    );
    assert!(
        waiting(&ui),
        "a FAIL that names no target does not settle this conversation's request"
    );
    advance(&mut ui, wait_window() - Duration::from_secs(1));
    assert!(waiting(&ui), "the wait holds just before its deadline");
    advance(&mut ui, Duration::from_secs(1));
    assert!(!waiting(&ui), "the wait is given up at its deadline");
}

#[gpui::test]
fn a_silent_server_gives_the_wait_up_at_its_deadline(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_for_history(&mut ui);
    tick(&mut ui);
    advance(&mut ui, wait_window() - Duration::from_secs(1));
    assert!(waiting(&ui), "the wait holds just before its deadline");
    advance(&mut ui, Duration::from_secs(1));
    assert!(!waiting(&ui), "the wait is given up at its deadline");
}

#[gpui::test]
fn a_dropped_connection_gives_the_wait_up(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_for_history(&mut ui);
    deliver(&mut ui, ":server ERROR :gone");
    assert!(
        !waiting(&ui),
        "a request on a dropped connection cannot be answered, so the wait is given up"
    );
}

#[gpui::test]
fn a_manual_disconnect_gives_the_wait_up(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_for_history(&mut ui);
    ui.shell.update(ui.cx, |shell, cx| {
        shell.controller.disconnect(OMARCHY);
        shell.tick(cx);
    });
    ui.settle();
    assert!(
        !waiting(&ui),
        "a disconnect leaves no request to answer, so the wait is given up"
    );
}

#[gpui::test]
fn a_page_with_the_saved_line_lands_on_it_as_its_request_settles(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    offer_chathistory(&mut ui);
    ui.select("anna");
    ui.fill_until_scrollable("anna", "fred", "landing line");
    request_history(&mut ui);
    reopen_at_gone_place(&mut ui, "anna");
    assert!(
        waiting(&ui),
        "the place waits while its history is in flight"
    );
    // Rows below the saved line fill the window, so it is not the list's end.
    let mut page = vec!["fill-0".to_owned(), "gone-anchor".to_owned()];
    page.extend((0..30).map(|i| format!("tail-{i}")));
    deliver_burst(&mut ui, &anna_page("h", &page));
    assert!(
        !waiting(&ui),
        "the page closed the request and brought the saved line"
    );
    assert_eq!(
        top_msgid(&ui),
        "gone-anchor",
        "the place lands on the saved line the page brought, not on the row it was saved at"
    );
}

#[gpui::test]
fn an_empty_page_gives_the_wait_up_and_lands_on_the_saved_row(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    offer_chathistory(&mut ui);
    ui.select("anna");
    ui.fill_until_scrollable("anna", "fred", "landing line");
    request_history(&mut ui);
    reopen_at_gone_place(&mut ui, "anna");
    assert!(
        waiting(&ui),
        "the place waits while its history is in flight"
    );
    deliver_burst(&mut ui, &anna_page("e", &[] as &[&str]));
    assert!(
        !waiting(&ui),
        "an empty page settles the request, so the place lands without the line"
    );
    assert!(
        !ui.at_end(),
        "the given-up place lands on its saved row, not at the end"
    );
}

#[gpui::test]
fn a_full_page_without_the_line_keeps_the_wait_for_the_next_page(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    offer_chathistory(&mut ui);
    ui.select("anna");
    ui.fill_until_scrollable("anna", "fred", "landing line");
    start_after_chain(&mut ui);
    reopen_at_gone_place(&mut ui, "anna");
    assert!(
        waiting(&ui),
        "the place waits while its AFTER page is in flight"
    );
    // A full page chains the next AFTER page, so the request is still in flight after it.
    let first: Vec<String> = (0..HISTORY_LIMIT).map(|i| format!("fill-{i}")).collect();
    deliver_burst(&mut ui, &anna_page("p1", &first));
    assert!(
        waiting(&ui),
        "the chained AFTER page is in flight, so the place keeps waiting"
    );
    // As above: rows below the saved line fill the window, so it is not the list's end.
    let mut tail = vec!["fill-tail".to_owned(), "gone-anchor".to_owned()];
    tail.extend((0..30).map(|i| format!("after-{i}")));
    deliver_burst(&mut ui, &anna_page("p2", &tail));
    assert!(
        !waiting(&ui),
        "the chained page brought the saved line, so the place lands"
    );
    assert_eq!(
        top_msgid(&ui),
        "gone-anchor",
        "the place lands on the saved line from the chained page"
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
fn a_line_after_wheel_up_does_not_report_the_end(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("anna");
    ui.fill_until_scrollable("anna", "fred", "wheel base");
    deliver(&mut ui, ":anna!u@h PRIVMSG fred :first line");
    assert!(ui.at_end(), "the reader follows the end before wheeling up");
    wheel(&mut ui, 3200.);
    assert!(!ui.at_end(), "wheel up leaves the reader detached");
    assert!(
        !reported_end(&ui),
        "a detached reader must not report the end"
    );
    deliver(&mut ui, ":anna!u@h PRIVMSG fred :second line");
    assert!(
        !ui.at_end(),
        "another line must not pull a wheeled-up reader back to the end"
    );
    assert!(
        !reported_end(&ui),
        "a detached reader must not report the end when another line arrives"
    );
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

#[gpui::test]
fn a_saved_line_near_the_loaded_end_keeps_its_place_as_later_history_arrives(
    cx: &mut gpui::TestAppContext,
) {
    let mut ui = fixture::open(cx);
    offer_chathistory(&mut ui);
    ui.select("anna");
    ui.fill_until_scrollable("anna", "fred", "landing line");
    start_after_chain(&mut ui);
    reopen_at_gone_place(&mut ui, "anna");
    let first: Vec<String> = (0..HISTORY_LIMIT).map(|i| format!("fill-{i}")).collect();
    deliver_burst(&mut ui, &anna_page("p1", &first));
    assert!(
        waiting(&ui),
        "the chained page is in flight, so the place keeps waiting"
    );
    // The saved line is near the end of the rows loaded so far, so the list cannot scroll it to
    // the top until the next page arrives. The reader must still not be taken for one at the end.
    let mut second: Vec<String> = (0..HISTORY_LIMIT).map(|i| format!("more-{i}")).collect();
    second[80] = "gone-anchor".to_owned();
    deliver_burst(&mut ui, &anna_page("p2", &second));
    assert!(
        !waiting(&ui),
        "page 2 brought the saved line, so the place lands"
    );
    ui.shell
        .update(ui.cx, |shell, cx| shell.remember_open_transcript(cx));
    let saved = ui.shell.read_with(ui.cx, |shell, _| {
        shell.controller.place_for(OMARCHY, "anna")
    });
    assert!(
        !saved.expect("a place is saved").follow_end,
        "leaving the restored reader saves its line, not the end"
    );
    let tail: Vec<String> = (0..50).map(|i| format!("tail-{i}")).collect();
    deliver_burst(&mut ui, &anna_page("p3", &tail));
    assert!(
        !reported_end(&ui),
        "the page after the saved line must not report the reader at the end"
    );
    assert!(
        !ui.at_end(),
        "the page after the saved line must not pull the restored reader to the end"
    );
}
