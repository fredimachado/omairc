//! Shell-level proofs for the reader's place: a line and an offset, or the end. The list is a
//! projection of that place, so what the window does to the list (a new size, rows it has not
//! measured) does not move the place, and what moves the rows keeps it on its line.

use super::fixture::{self, SeededShell, OMARCHY, WINDOW_HEIGHT, WINDOW_WIDTH};
use super::scroll_restore::top_msgid;
use crate::model::transcript::RowKind;
use crate::storage::scrollplace::ScrollPlace;

/// `count` lines from a direct message nobody has open, so they arrive unread under a mark.
fn unread_direct_lines(ui: &mut SeededShell, nick: &str, count: usize) {
    ui.select("#omarchy");
    for index in 0..count {
        ui.inject(OMARCHY, &format!(":{nick}!u@h PRIVMSG fred :line {index}"));
    }
}

/// Opens anna at a saved line ten rows above the end, so the list clamps it to the end. The
/// lines are long enough that a narrower window wraps them past the height of the window.
fn on_a_clamped_line(ui: &mut SeededShell) {
    ui.select("anna");
    let body = "words that wrap ".repeat(10);
    for index in 0..60 {
        ui.inject(
            OMARCHY,
            &format!("@msgid=kept-{index} :anna!u@h PRIVMSG fred :{body}{index}"),
        );
    }
    ui.select("#help");
    ui.shell.update(ui.cx, |shell, _| {
        shell.controller.remember_place(
            OMARCHY,
            "anna",
            ScrollPlace {
                msg_id: "kept-50".to_owned(),
                ..ScrollPlace::default()
            },
        );
    });
    ui.select("anna");
    assert!(ui.at_end(), "ten rows fit on screen, so the list clamps");
    assert!(!ui.reported_end(), "the reader has not reached the end");
}

#[gpui::test]
fn a_jump_after_a_resize_keeps_its_line_while_rows_below_are_unmeasured(
    cx: &mut gpui::TestAppContext,
) {
    let mut ui = fixture::open(cx);
    unread_direct_lines(&mut ui, "newbie", 60);
    ui.select("newbie");
    ui.resize(WINDOW_WIDTH * 0.6, WINDOW_HEIGHT);
    ui.jump_to("line 10");
    assert!(
        !ui.reported_end(),
        "fifty lines below the jump are not the end, so the read marker stays put"
    );
    let saved = ui.save_place("newbie");
    assert_eq!(
        saved.body, "line 10",
        "a width change must not make the rows below read as the end"
    );
    assert!(!saved.follow_end, "the jump is off the end");
}

#[gpui::test]
fn a_wheel_after_a_resize_does_not_pin_early(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    unread_direct_lines(&mut ui, "newbie", 60);
    ui.select("newbie");
    ui.resize(WINDOW_WIDTH * 0.6, WINDOW_HEIGHT);
    ui.wheel(-400.);
    assert!(!ui.at_end(), "rows below the wheel are not the end");
    assert!(!ui.reported_end(), "so the read marker stays put");
    assert!(
        !ui.save_place("newbie").follow_end,
        "a wheel short of the end does not save follow-end"
    );
}

#[gpui::test]
fn a_page_down_after_a_resize_does_not_pin_early(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    unread_direct_lines(&mut ui, "newbie", 60);
    ui.select("newbie");
    ui.resize(WINDOW_WIDTH * 0.6, WINDOW_HEIGHT);
    ui.page_down();
    assert!(!ui.at_end(), "rows below the page are not the end");
    assert!(!ui.reported_end(), "so the read marker stays put");
    assert!(
        !ui.save_place("newbie").follow_end,
        "a page short of the end does not save follow-end"
    );
}

#[gpui::test]
fn a_clamped_line_comes_back_when_the_window_gets_shorter(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    on_a_clamped_line(&mut ui);
    ui.resize(WINDOW_WIDTH, WINDOW_HEIGHT * 0.4);
    assert_eq!(
        top_msgid(&ui),
        "kept-50",
        "the place is the reader's, so the list goes back to it when it can show it"
    );
    assert!(!ui.reported_end(), "the reader has not reached the end");
}

#[gpui::test]
fn a_clamped_line_comes_back_when_the_window_gets_narrower(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    on_a_clamped_line(&mut ui);
    ui.resize(WINDOW_WIDTH * 0.6, WINDOW_HEIGHT);
    assert_eq!(
        top_msgid(&ui),
        "kept-50",
        "the place is the reader's, so the list goes back to it when it can show it"
    );
    assert!(!ui.reported_end(), "the reader has not reached the end");
}

#[gpui::test]
fn a_following_reader_stays_at_the_end_when_the_window_gets_shorter(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    unread_direct_lines(&mut ui, "newbie", 60);
    ui.select("newbie");
    ui.shell.update(ui.cx, |shell, cx| {
        shell.transcript.update(cx, |t, cx| t.scroll_to_bottom(cx));
    });
    ui.settle();
    ui.resize(WINDOW_WIDTH, WINDOW_HEIGHT * 0.4);
    assert!(ui.at_end(), "a reader who follows the end keeps showing it");
    assert!(ui.reported_end());
}

#[gpui::test]
fn a_line_arriving_above_the_end_arms_the_chip_before_a_layout(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("anna");
    ui.fill_until_scrollable("anna", "fred", "chip line");
    ui.page_up();
    // One update, so no frame lays the list out between the line and the read.
    ui.shell.update(ui.cx, |shell, cx| {
        shell
            .controller
            .demo()
            .expect("demo controller")
            .inject(OMARCHY, b":anna!u@h PRIVMSG fred :while reading\r\n");
        shell.tick(cx);
        assert!(
            shell.transcript.read(cx).unseen_target().is_some(),
            "the frame this line asks for draws the chip, so a reset list must not read as the end"
        );
    });
}

#[gpui::test]
fn a_rebuild_keeps_a_reader_where_they_are(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    unread_direct_lines(&mut ui, "newbie", 60);
    ui.select("newbie");
    ui.shell
        .update(ui.cx, |shell, cx| shell.rebuild_view_keeping_place(cx));
    ui.settle();
    assert_eq!(
        ui.top_kind(),
        Some(RowKind::UnreadMark),
        "a rebuild moves no rows, so the view stays as it was"
    );
    assert!(!ui.reported_end());
    ui.shell.update(ui.cx, |shell, cx| {
        shell.transcript.update(cx, |t, cx| t.scroll_to_bottom(cx));
    });
    ui.settle();
    ui.shell
        .update(ui.cx, |shell, cx| shell.rebuild_view_keeping_place(cx));
    ui.settle();
    assert!(
        ui.at_end(),
        "a reader who went to the end is not sent back to the mark"
    );
    assert!(ui.reported_end());
}
