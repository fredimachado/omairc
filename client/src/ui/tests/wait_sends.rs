//! A line sent to the conversation the transcript shows retires a waiting place, whatever
//! sent it: composer text, `/me`, `/msg`, `/notice`, or the product CLI's `send`. A line to
//! another target keeps the wait.

use crate::desktop::cli_cursor::CursorStore;
use crate::desktop::ipchandler::CliHandler;

use super::fixture::{self, SeededShell};
use super::scroll_restore::{submit_line, waiting, waiting_for_history};

/// One request from the product CLI, answered as the window's loop answers it: the handler
/// runs on the shell's controller, then the shell follows what the request changed.
fn cli_request(ui: &mut SeededShell, line: &str) -> String {
    let handler = CliHandler::new(CursorStore::at(
        std::env::temp_dir().join("omairc-wait-sends"),
    ));
    ui.shell.update(ui.cx, |shell, cx| {
        let reply = handler.handle_line(&mut shell.controller, line);
        shell.drain_notifications(cx);
        reply.line
    })
}

#[gpui::test]
fn a_msg_to_the_open_direct_message_retires_a_waiting_place(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_for_history(&mut ui);
    submit_line(&mut ui, "/msg lena hello lena");
    assert!(waiting(&ui), "a /msg to another nick keeps the wait");
    submit_line(&mut ui, "/msg anna reply");
    assert!(
        !waiting(&ui),
        "a /msg to the open direct message retires the wait, as a composer line does"
    );
    assert!(
        ui.reported_end(),
        "the send reports the end before any render, as a composer send does"
    );
}

#[gpui::test]
fn a_notice_to_the_open_conversation_retires_a_waiting_place(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_for_history(&mut ui);
    submit_line(&mut ui, "/notice #help hello channel");
    assert!(waiting(&ui), "a /notice to another target keeps the wait");
    submit_line(&mut ui, "/notice anna reply");
    assert!(
        !waiting(&ui),
        "a /notice to the open direct message retires the wait"
    );
    assert!(ui.reported_end());
}

#[gpui::test]
fn a_me_action_retires_a_waiting_place(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_for_history(&mut ui);
    submit_line(&mut ui, "/msg lena hello lena");
    assert!(waiting(&ui), "a /msg to another nick keeps the wait");
    submit_line(&mut ui, "/me waves");
    assert!(
        !waiting(&ui),
        "a /me in the open direct message retires the wait"
    );
    assert!(ui.reported_end());
}

#[gpui::test]
fn a_cli_send_to_the_open_conversation_retires_a_waiting_place(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    waiting_for_history(&mut ui);
    let reply = cli_request(
        &mut ui,
        r#"{"cmd":"send","network":"omarchy","target":"lena","text":"hello lena"}"#,
    );
    assert_eq!(reply, r#"{"ok":true}"#);
    assert!(waiting(&ui), "a CLI send to another target keeps the wait");
    let reply = cli_request(
        &mut ui,
        r#"{"cmd":"send","network":"omarchy","target":"anna","text":"reply"}"#,
    );
    assert_eq!(reply, r#"{"ok":true}"#);
    assert!(
        !waiting(&ui),
        "a CLI send to the open direct message retires the wait"
    );
    assert!(
        ui.reported_end(),
        "the CLI send reports the end, as a composer send does"
    );
}
