//! Shell-level proofs for where the reader is in a transcript: following the end, anchored on a
//! line, waiting on a saved line, or catching up through lines that arrived unseen. Exactly one
//! holds at a time, and only the reader who follows the end reports it to the read marker.

use super::fixture::{self, SeededShell, OMARCHY};

/// `count` lines from a direct message nobody has open, so they arrive unread under a mark.
fn unread_direct_lines(ui: &mut SeededShell, nick: &str, count: usize) {
    ui.select("#omarchy");
    for index in 0..count {
        ui.inject(OMARCHY, &format!(":{nick}!u@h PRIVMSG fred :line {index}"));
    }
}

#[gpui::test]
fn landing_clamped_to_end_reports_follow_end(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    unread_direct_lines(&mut ui, "newbie", 6);
    ui.select("newbie");
    ui.jump_to("line 2");
    assert!(ui.at_end(), "six lines fit on screen, so the landing clamps");
    assert!(
        ui.reported_end(),
        "a landing the layout clamped to the end follows it, and reports it"
    );
}

#[gpui::test]
fn landing_short_of_end_stays_anchored(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    unread_direct_lines(&mut ui, "newbie", 60);
    ui.select("newbie");
    ui.jump_to("line 10");
    assert!(!ui.at_end(), "fifty lines below the landing are not the end");
    assert!(
        !ui.reported_end(),
        "a landing short of the end does not report the end"
    );
}

#[gpui::test]
fn buffer_switch_drops_catch_up(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("anna");
    ui.fill_until_scrollable("anna", "fred", "catch up");
    ui.page_up();
    assert!(!ui.at_end(), "the reader is scrolled up");
    ui.inject_chat("anna", "fred", "unseen arrival");
    assert!(ui.first_unseen().is_some(), "an arrival while scrolled up arms the catch-up");
    ui.select("#help");
    assert!(ui.first_unseen().is_none(), "another buffer has no catch-up");
    ui.select("anna");
    assert!(
        ui.first_unseen().is_none(),
        "coming back does not bring the catch-up back"
    );
}
