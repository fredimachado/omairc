//! Shell-level proofs for the reader's place: a line and an offset, or the end. The list is a
//! projection of that place, so what the window does to the list (a new size, rows it has not
//! measured) does not move the place, and what moves the rows keeps it on its line.

use super::fixture::{self, SeededShell, OMARCHY, WINDOW_HEIGHT, WINDOW_WIDTH};
use super::scroll_restore::{
    assert_given_up_landing, deliver, request_history, top_msgid, waiting, waiting_above_a_mark,
};
use crate::model::transcript::RowKind;
use crate::model::ConversationRef;
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

#[gpui::test]
fn a_page_key_before_a_layout_keeps_the_place(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    unread_direct_lines(&mut ui, "newbie", 60);
    ui.select("newbie");
    // One update, so no frame lays the list out between the line and the key.
    ui.shell.update(ui.cx, |shell, cx| {
        shell
            .controller
            .demo()
            .expect("demo controller")
            .inject(OMARCHY, b":newbie!u@h PRIVMSG fred :one more\r\n");
        shell.tick(cx);
        shell.transcript.update(cx, |t, cx| {
            t.scroll_page(false, false, cx);
            assert!(
                !t.reports_follow_end(),
                "a reset list has no heights, which must not read as the end"
            );
        });
    });
    ui.settle();
    assert_eq!(ui.save_place("newbie").body, "line 0");
}

#[gpui::test]
fn a_scrollbar_click_after_a_resize_goes_by_the_whole_transcript(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    unread_direct_lines(&mut ui, "newbie", 60);
    ui.select("newbie");
    let width = WINDOW_WIDTH * 0.6;
    ui.resize(width, WINDOW_HEIGHT);
    // The track measures every row once the width has settled. Mid-drag it does nothing
    // (`a_page_key_and_a_scrollbar_click_during_a_drag_do_nothing`).
    ui.settle_width();
    ui.click(width - 2., WINDOW_HEIGHT / 2.);
    assert!(
        ui.scroll_top().item_ix >= 15,
        "the middle of the track is the middle of sixty lines, not of the rows drawn so far"
    );
    assert!(!ui.reported_end());
}

#[gpui::test]
fn a_find_jump_during_a_wait_retires_it_and_saves_the_line(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_above_a_mark(&mut ui, true, request_history);
    ui.jump_to("New messages");
    assert!(!waiting(&ui), "a jump retires the wait");
    deliver(&mut ui, ":anna!u@h PRIVMSG fred :one more");
    assert_eq!(
        ui.save_place("anna").body,
        "landing line 0",
        "the place saved is the line jumped to, not the waiting line or the moved mark"
    );
}

#[gpui::test]
fn a_rebuild_during_a_wait_keeps_the_unread_preference(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_above_a_mark(&mut ui, true, request_history);
    ui.shell
        .update(ui.cx, |shell, cx| shell.rebuild_view_keeping_place(cx));
    ui.settle();
    assert!(waiting(&ui), "a rebuild keeps the wait");
    deliver(
        &mut ui,
        ":server FAIL CHATHISTORY INVALID_TARGET BEFORE anna :no",
    );
    assert!(!waiting(&ui), "the request settled without the line");
    assert_given_up_landing(&mut ui, true);
}

#[gpui::test]
fn a_landing_stays_a_place_when_the_window_grows_past_it(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    unread_direct_lines(&mut ui, "newbie", 60);
    ui.select("newbie");
    assert_eq!(ui.top_kind(), Some(RowKind::UnreadMark));
    ui.resize(WINDOW_WIDTH, WINDOW_HEIGHT * 5.);
    assert!(ui.at_end(), "the whole transcript fits now");
    assert!(
        !ui.reported_end(),
        "the landing settled when it was laid out, and a resize is not the reader reaching the end"
    );
    ui.resize(WINDOW_WIDTH, WINDOW_HEIGHT);
    assert_eq!(
        ui.top_kind(),
        Some(RowKind::UnreadMark),
        "the reader is still on the mark when the window comes back"
    );
}

fn chip_drawn(ui: &SeededShell) -> bool {
    ui.shell
        .read_with(ui.cx, |shell, cx| shell.transcript.read(cx).chip_drawn())
}

#[gpui::test]
fn the_chip_follows_the_layout_with_no_other_frame_to_help(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("anna");
    ui.fill_until_scrollable("anna", "fred", "chip line");
    ui.page_up();
    // Neither step refreshes the window, so the only frames are the ones they ask for.
    ui.shell.update(ui.cx, |shell, cx| {
        shell
            .controller
            .demo()
            .expect("demo controller")
            .inject(OMARCHY, b":anna!u@h PRIVMSG fred :while reading\r\n");
        shell.tick(cx);
    });
    ui.cx.run_until_parked();
    assert!(chip_drawn(&ui), "a line above the end draws the chip");

    unread_direct_lines(&mut ui, "newbie", 2);
    ui.shell.update(ui.cx, |shell, cx| {
        shell.select(ConversationRef::new(OMARCHY, "newbie"), cx);
    });
    ui.cx.run_until_parked();
    assert!(ui.reported_end(), "the mark's landing clamped to the end");
    assert!(
        !chip_drawn(&ui),
        "the frame that found the clamp asks for one without the chip"
    );
}

/// A line that wraps to several rows at the window's width, numbered so a test can find it.
fn long_body(index: usize) -> String {
    format!("{}{index}", "words that wrap ".repeat(12))
}

/// `count` long lines from anna, injected in one burst so one refresh shows them all. Lines
/// that wrap are what make a width change expensive.
fn long_lines(ui: &mut SeededShell, count: usize) {
    ui.select("anna");
    ui.shell.update(ui.cx, |shell, cx| {
        let demo = shell.controller.demo().expect("demo controller");
        for index in 0..count {
            let line = format!(":anna!u@h PRIVMSG fred :{}\r\n", long_body(index));
            demo.inject(OMARCHY, line.as_bytes());
        }
        shell.tick(cx);
    });
    ui.settle();
}

/// `steps` width changes in a row from `from` to `to`, one frame each, with no quiet period
/// between them. Returns the most rows one of those frames laid out.
fn drag_between(ui: &mut SeededShell, from: f32, to: f32, steps: usize) -> usize {
    let mut most = 0;
    for step in 1..=steps {
        let before = ui.rows_rendered();
        ui.resize(
            from + (to - from) * step as f32 / steps as f32,
            WINDOW_HEIGHT,
        );
        most = most.max(ui.rows_rendered() - before);
    }
    most
}

/// A drag of `steps` changes, each 8 px narrower than the last, from the window's width.
fn drag(ui: &mut SeededShell, steps: usize) -> usize {
    drag_between(ui, WINDOW_WIDTH, dragged_width(steps), steps)
}

/// The window's width after `drag(ui, steps)`.
fn dragged_width(steps: usize) -> f32 {
    WINDOW_WIDTH - 8. * steps as f32
}

/// Rows one frame may lay out in a drag: a screenful with its overdraw, not the transcript.
const SCREENFUL_ROWS: usize = 200;

/// The drag that precedes a guarantee left the rows unmeasured: no frame of it laid out the
/// transcript. Each guarantee below needs that.
fn assert_unmeasured(most_rows: usize, transcript_rows: usize) {
    assert!(
        most_rows < transcript_rows / 2,
        "a drag frame laid out {most_rows} of {transcript_rows} rows: the rows were measured mid-drag"
    );
}

#[gpui::test]
fn a_drag_of_many_width_changes_lays_out_no_rows_until_it_settles(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    long_lines(&mut ui, 2000);
    ui.settle_width();
    for step in 1..=30 {
        let before = ui.rows_rendered();
        ui.resize(dragged_width(step), WINDOW_HEIGHT);
        let laid_out = ui.rows_rendered() - before;
        assert!(
            laid_out < SCREENFUL_ROWS,
            "drag frame {step} laid out {laid_out} rows; only a screenful is laid out mid-drag"
        );
    }
    let before = ui.rows_rendered();
    ui.settle_width();
    let settled = ui.rows_rendered() - before;
    assert!(
        settled >= 2000,
        "the settle measures every row, once: {settled} rows laid out"
    );
    let before = ui.rows_rendered();
    ui.settle();
    let after = ui.rows_rendered() - before;
    assert!(
        after < SCREENFUL_ROWS,
        "the frame after the settle lays out a screenful again, not every row: {after}"
    );
}

#[gpui::test]
fn a_width_is_not_measured_until_it_has_been_still_for_the_quiet_period(
    cx: &mut gpui::TestAppContext,
) {
    let mut ui = fixture::open(cx);
    long_lines(&mut ui, 2000);
    ui.settle_width();
    ui.shell.update(ui.cx, |shell, cx| {
        shell.transcript.update(cx, |t, cx| t.scroll_to_bottom(cx));
    });
    ui.settle();
    ui.jump_to(&long_body(1000));
    assert_unmeasured(drag(&mut ui, 10), ui.row_count());
    // The first tick sees the last change and starts the quiet period. Less than the period later,
    // the width is still unsettled and a wheel does nothing.
    ui.shell.update(ui.cx, |shell, cx| shell.tick(cx));
    ui.shell.read_with(ui.cx, |shell, _| {
        shell
            .controller
            .advance_test_clock(std::time::Duration::from_millis(100))
    });
    ui.shell.update(ui.cx, |shell, cx| shell.tick(cx));
    let top = ui.scroll_top().item_ix;
    ui.wheel(-800.);
    assert_eq!(
        ui.scroll_top().item_ix,
        top,
        "a width that is still in its quiet period is not measured"
    );
    ui.shell.read_with(ui.cx, |shell, _| {
        shell
            .controller
            .advance_test_clock(std::time::Duration::from_millis(100))
    });
    ui.shell.update(ui.cx, |shell, cx| shell.tick(cx));
    ui.settle();
    ui.wheel(-800.);
    assert!(
        ui.scroll_top().item_ix > top,
        "once the quiet period has passed the width is settled, and a wheel moves"
    );
}

#[gpui::test]
fn a_find_jump_during_a_drag_lands_on_its_line(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    long_lines(&mut ui, 2000);
    ui.settle_width();
    assert_unmeasured(drag(&mut ui, 10), ui.row_count());
    ui.jump_to(&long_body(1000));
    assert!(
        !ui.at_end(),
        "a line in the middle is not the end, mid-drag"
    );
    assert!(!ui.reported_end(), "so the read marker stays put, mid-drag");
    assert_eq!(
        ui.save_place("anna").body,
        long_body(1000),
        "the jump lands on its line while the width is still moving"
    );
    ui.settle_width();
    assert!(
        !ui.at_end(),
        "still a line in the middle once the width settles"
    );
    assert!(!ui.reported_end());
    assert_eq!(ui.save_place("anna").body, long_body(1000));
}

#[gpui::test]
fn a_wheel_during_a_drag_does_nothing_and_a_wheel_after_it_moves(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    long_lines(&mut ui, 2000);
    ui.settle_width();
    ui.jump_to(&long_body(1000));
    assert_unmeasured(drag(&mut ui, 10), ui.row_count());
    let top = ui.scroll_top().item_ix;
    ui.wheel(-800.);
    assert_eq!(
        ui.scroll_top().item_ix,
        top,
        "a wheel mid-drag does nothing, since the rows below are not measured"
    );
    assert!(!ui.reported_end(), "and it does not report the end");
    ui.settle_width();
    ui.wheel(-800.);
    assert!(
        ui.scroll_top().item_ix > top,
        "after the settle a wheel moves the reader"
    );
}

#[gpui::test]
fn a_following_reader_stays_at_the_end_through_a_drag(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    long_lines(&mut ui, 2000);
    ui.settle_width();
    assert!(
        ui.at_end() && ui.reported_end(),
        "the reader follows the end before the drag"
    );
    assert_unmeasured(drag(&mut ui, 30), ui.row_count());
    assert!(
        ui.at_end(),
        "a following reader stays at the end while the width moves"
    );
    assert!(ui.reported_end());
    ui.wheel(3200.);
    assert!(
        ui.at_end() && ui.reported_end(),
        "a wheel up mid-drag does not detach the reader, so no early unpin"
    );
    ui.settle_width();
    assert!(ui.at_end() && ui.reported_end(), "nor does the settle");
    ui.wheel(3200.);
    assert!(
        !ui.at_end(),
        "a wheel up after the settle detaches the reader"
    );
    assert!(!ui.reported_end());
}

#[gpui::test]
fn a_page_key_and_a_scrollbar_click_during_a_drag_do_nothing(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    long_lines(&mut ui, 2000);
    ui.settle_width();
    ui.jump_to(&long_body(1000));
    assert_unmeasured(drag(&mut ui, 10), ui.row_count());
    assert!(
        ui.overflows(),
        "the transcript overflows, so the track is there to click"
    );
    let top = ui.scroll_top().item_ix;
    ui.page_down();
    assert_eq!(
        ui.scroll_top().item_ix,
        top,
        "a page key mid-drag does nothing"
    );
    ui.click(dragged_width(10) - 2., WINDOW_HEIGHT / 2.);
    assert_eq!(
        ui.scroll_top().item_ix,
        top,
        "a scrollbar click mid-drag does nothing: the track's rows are not measured"
    );
    assert!(!ui.reported_end());
}

#[gpui::test]
fn a_clamped_saved_line_comes_back_when_the_drag_settles(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    on_a_clamped_line(&mut ui);
    // Down to the width the round-8 test narrows to, so the line's rows wrap as they do there.
    assert_unmeasured(
        drag_between(&mut ui, WINDOW_WIDTH, WINDOW_WIDTH * 0.6, 10),
        ui.row_count(),
    );
    // Mid-drag the layout may draw the end, since the rows below the line are not measured. The
    // reader's place stays the line, and it is drawn again once the width settles.
    assert_eq!(
        ui.save_place("anna").msg_id,
        "kept-50",
        "mid-drag the reader's place is still the line"
    );
    assert!(!ui.reported_end(), "the reader has not reached the end");
    ui.settle_width();
    assert_eq!(
        top_msgid(&ui),
        "kept-50",
        "once the width settles the line is on top again"
    );
    assert!(
        !ui.reported_end(),
        "and the reader still has not reached the end"
    );
}

#[gpui::test]
fn a_landing_near_the_end_mid_drag_follows_the_end_once_it_settles(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    long_lines(&mut ui, 2000);
    ui.settle_width();
    assert_unmeasured(drag(&mut ui, 10), ui.row_count());
    // Four lines below the landing do not fill the window, so the layout clamps it to the end.
    ui.jump_to(&long_body(1995));
    assert!(
        !ui.reported_end(),
        "the clamp cannot be judged while the rows are not measured, so the reader does not follow yet"
    );
    ui.settle_width();
    assert!(
        ui.at_end(),
        "once the rows are measured the landing is clamped to the end"
    );
    assert!(
        ui.reported_end(),
        "a landing the layout clamped follows the end, as a landing does once measured"
    );
}
