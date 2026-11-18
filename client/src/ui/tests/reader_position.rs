//! Shell-level proofs for where the reader is in a transcript: following the end, anchored on a
//! line, waiting on a saved line, or catching up through lines that arrived unseen. Exactly one
//! holds at a time, and only the reader who follows the end reports it to the read marker.

use std::collections::HashMap;

use chrono::{TimeZone, Utc};

use crate::irc::event::MsgId;
use crate::irc::world::{Message, MessageKind, Origin};
use crate::ui::transcript::view::ConversationLines;

use super::fixture::{self, SeededShell, OMARCHY};

fn tagged(body: &str, sequence: i64, msgid: &str) -> Message {
    Message {
        kind: MessageKind::Message,
        author: "anna".to_owned(),
        body: body.to_owned(),
        time: Utc.with_ymd_and_hms(2026, 9, 10, 9, 0, 0).unwrap(),
        server_time: None,
        msgid: MsgId::new(msgid),
        reply_to: String::new(),
        origin: Origin::Live,
        mentioned: false,
        collapsible: false,
        sequence,
        only_visible_to_you: false,
    }
}

/// `count` lines from a direct message nobody has open, so they arrive unread under a mark.
fn unread_direct_lines(ui: &mut SeededShell, nick: &str, count: usize) {
    ui.select("#omarchy");
    for index in 0..count {
        ui.inject(OMARCHY, &format!(":{nick}!u@h PRIVMSG fred :line {index}"));
    }
}

/// Shows `messages` in the open transcript, as a refresh of the same buffer does, or as a first
/// show when `fresh`.
fn show(ui: &mut SeededShell, messages: &[Message], fresh: bool) {
    let self_nick = ui
        .shell
        .read_with(ui.cx, |shell, _| shell.controller.current_nick_for(OMARCHY));
    let features = ui
        .shell
        .read_with(ui.cx, |shell, _| shell.controller.features(OMARCHY).clone());
    ui.shell.update(ui.cx, |shell, cx| {
        shell.transcript.update(cx, |t, cx| {
            let lines = ConversationLines {
                messages,
                mark: None,
                facts: HashMap::new(),
                self_nick: &self_nick,
                features: &features,
                typing_peer: None,
                today: chrono::Local::now().date_naive(),
            };
            if fresh {
                t.set_conversation(lines);
            } else {
                t.refresh_conversation(lines, cx);
            }
        });
    });
    ui.settle();
}

fn history(count: i64) -> Vec<Message> {
    (0..count)
        .map(|i| tagged(&format!("line {i}"), i + 1, &format!("id-{i}")))
        .collect()
}

fn body_at(ui: &SeededShell, row: usize) -> String {
    ui.shell.read_with(ui.cx, |shell, cx| {
        shell.transcript.read(cx).rows()[row].body.plain.clone()
    })
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
fn prepend_keeps_the_catch_up_and_the_reader_on_its_line(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("anna");
    show(&mut ui, &history(24), true);
    ui.shell.update(ui.cx, |shell, cx| {
        shell.transcript.update(cx, |t, cx| {
            t.scroll_to_bottom(cx);
            for _ in 0..2 {
                t.scroll_page(false, false, cx);
            }
        });
    });
    ui.settle();
    assert!(!ui.at_end(), "the reader is scrolled up");
    let mut arrived = history(24);
    arrived.push(tagged("first unseen", 25, "id-new"));
    show(&mut ui, &arrived, false);
    let before = ui.first_unseen().expect("an arrival while scrolled up arms the catch-up");
    assert_eq!(body_at(&ui, before), "first unseen");

    // Nine older lines arrive above everything the reader has.
    let mut older: Vec<Message> = (0..9)
        .map(|i| tagged(&format!("old {i}"), i + 1, &format!("old-{i}")))
        .collect();
    let mut rest = arrived;
    for (i, message) in rest.iter_mut().enumerate() {
        message.sequence = 100 + i as i64;
    }
    older.append(&mut rest);
    show(&mut ui, &older, false);

    let after = ui.first_unseen().expect("a prepend keeps the catch-up");
    assert_eq!(
        body_at(&ui, after),
        "first unseen",
        "the catch-up row moves with the rows prepended above it"
    );
    assert!(!ui.at_end(), "a prepend leaves the reader where it was");
    assert!(!ui.reported_end(), "a scrolled-up reader does not report the end");
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
