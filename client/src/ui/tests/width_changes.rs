//! A width change that is not a drag: the member panel toggles, or a channel switch changes it.
//! One change, and input straight after it. The first change of a burst measures its rows at
//! once, so the page key, the wheel and Ctrl+Home take effect with no tick and no settle.

use super::fixture::{self, SeededShell, OMARCHY};
use super::scroll_place::long_body;

/// `count` long lines in #help in one burst, so the channel's member panel shows beside them.
fn long_channel_lines(ui: &mut SeededShell, count: usize) {
    ui.select("#help");
    ui.shell.update(ui.cx, |shell, cx| {
        let demo = shell.controller.demo().expect("demo controller");
        for index in 0..count {
            let line = format!(":anna!u@h PRIVMSG #help :{}\r\n", long_body(index));
            demo.inject(OMARCHY, line.as_bytes());
        }
        shell.tick(cx);
    });
    ui.settle();
}

/// Reader mid-transcript in #help, then the member panel closes: one width change, nothing else.
/// The setup settles first, since opening #help can change the width too.
fn closed_the_member_panel_mid_transcript(cx: &mut gpui::TestAppContext) -> SeededShell<'_> {
    let mut ui = fixture::open(cx);
    long_channel_lines(&mut ui, 200);
    ui.jump_to(&long_body(100));
    ui.settle_width();
    ui.keys("secondary-shift-m");
    ui
}

#[gpui::test]
fn a_page_key_straight_after_a_lone_width_change_moves_the_view(cx: &mut gpui::TestAppContext) {
    let mut ui = closed_the_member_panel_mid_transcript(cx);
    let top = ui.scroll_top().item_ix;
    ui.keys("pageup");
    assert!(
        ui.scroll_top().item_ix < top,
        "a page up after the member panel closes moves the view up"
    );
}

#[gpui::test]
fn a_wheel_straight_after_a_lone_width_change_moves_the_view(cx: &mut gpui::TestAppContext) {
    let mut ui = closed_the_member_panel_mid_transcript(cx);
    let top = ui.scroll_top().item_ix;
    ui.wheel(800.);
    assert!(
        ui.scroll_top().item_ix < top,
        "a wheel up after the member panel closes moves the view up"
    );
}

#[gpui::test]
fn ctrl_home_straight_after_a_lone_width_change_lands_at_the_top_and_stays(
    cx: &mut gpui::TestAppContext,
) {
    let mut ui = closed_the_member_panel_mid_transcript(cx);
    ui.keys("secondary-home");
    assert_eq!(
        ui.scroll_top().item_ix,
        0,
        "Ctrl+Home lands on the first row"
    );
    ui.settle_width();
    assert_eq!(
        ui.scroll_top().item_ix,
        0,
        "and the settle keeps the first row"
    );
}

#[gpui::test]
fn a_change_after_a_settle_starts_a_new_burst_and_measures_at_once(cx: &mut gpui::TestAppContext) {
    let mut ui = closed_the_member_panel_mid_transcript(cx);
    ui.settle_width();
    ui.keys("secondary-shift-m");
    let top = ui.scroll_top().item_ix;
    ui.keys("pageup");
    assert!(
        ui.scroll_top().item_ix < top,
        "a change after the quiet period is a new lone change, so a page up moves the view"
    );
}
