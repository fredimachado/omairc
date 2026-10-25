//! Typing-row proofs: the dots ride the transcript list end.

use crate::model::transcript::minute_of;
use crate::model::typing_tail::typing_continues;

use super::fixture::{self, OMARCHY};

#[gpui::test]
fn typing_indicator_rides_the_transcript_end(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("anna");
    assert_eq!(ui.selected_key(), "anna");
    assert_eq!(
        ui.typing_peer().as_deref(),
        Some("anna"),
        "seeded anna DM shows typing dots"
    );
    assert_eq!(
        ui.list_item_count(),
        ui.row_count() + 1,
        "typing dots are an extra list item after the last row"
    );
    assert!(ui.at_end(), "typing dots ride the transcript end");
}

#[gpui::test]
fn typing_indicator_moves_below_my_own_message(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("anna");
    assert!(ui.typing_peer().is_some());

    let previous = ui.row_count();
    ui.cx.update(|window, cx| {
        ui.shell.update(cx, |shell, cx| {
            shell.composer.update(cx, |c, cx| {
                c.set_text("hello anna");
                cx.notify();
            });
            shell.composer_submit(window, cx);
        });
    });
    ui.settle();
    // Echo may be async through the demo; inject the self line if needed.
    if ui.row_count() == previous {
        ui.inject(
            OMARCHY,
            ":fred!u@h PRIVMSG anna :hello anna",
        );
        ui.shell.update(ui.cx, |shell, cx| {
            shell.refresh_shown_typing(cx);
        });
        ui.settle();
    }
    assert!(ui.row_count() > previous, "own message lands in the DM");
    assert!(
        ui.typing_peer().is_some(),
        "typing dots stay after the own message"
    );

    let continues = ui.shell.read_with(ui.cx, |shell, cx| {
        let rows = shell.transcript.read(cx).rows();
        typing_continues(rows, "anna", &minute_of(chrono::Utc::now()))
    });
    assert!(
        !continues,
        "after my own line the dots take a header of their own"
    );
}

#[gpui::test]
fn typing_indicator_keeps_scroll_while_reading_history(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("dax");
    ui.fill_until_scrollable("dax", "fred", "scroll line");
    ui.page_up();
    assert!(!ui.at_end());
    let frozen = ui.scroll_top();

    ui.inject(OMARCHY, "@+typing=active :dax!u@h TAGMSG fred");
    ui.shell.update(ui.cx, |shell, cx| {
        shell.refresh_shown_typing(cx);
    });
    ui.settle();

    assert_eq!(ui.typing_peer().as_deref(), Some("dax"));
    assert_eq!(
        ui.scroll_top().item_ix,
        frozen.item_ix,
        "typing-only refresh must not jump the detached viewport"
    );
    assert!(!ui.at_end());
}
