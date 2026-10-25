//! Pointer proofs: nick open, middle-click close. URL / file-drop stay pending
//! where the test platform cannot observe the browser or upload host yet.

use crate::model::ConversationRef;

use super::fixture::{self, OMARCHY};

#[gpui::test]
fn open_direct_message_from_transcript_nick(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("#omarchy");
    ui.shell.update(ui.cx, |shell, cx| {
        shell.open_direct_nick("mira", cx);
    });
    ui.settle();
    assert_eq!(ui.selected_key(), "mira");
    assert!(
        ui.shell
            .read_with(ui.cx, |shell, _| !shell.controller.is_channel()),
        "nick activation opens a direct"
    );
}

#[gpui::test]
fn middle_click_closes_a_direct(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("dax");
    assert_eq!(ui.selected_key(), "dax");
    let target = ConversationRef::new(OMARCHY, "dax");
    ui.cx.update(|window, cx| {
        ui.shell.update(cx, |shell, cx| {
            shell.close_conversation_row(&target, window, cx);
        });
    });
    ui.settle();
    let still_open = ui.shell.read_with(ui.cx, |shell, _| {
        shell
            .controller
            .conversations()
            .iter()
            .any(|row| row.target == "dax")
    });
    assert!(!still_open, "middle-click close removes the direct from the sidebar");
    assert_ne!(
        ui.selected_key(),
        "dax",
        "closing the selected direct moves the selection"
    );
}
