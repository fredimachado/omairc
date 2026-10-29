//! Transcript placement proofs that only lived in QML ListView behaviour.

use std::collections::HashMap;

use chrono::{TimeZone, Utc};

use crate::irc::event::MsgId;
use crate::irc::world::{Message, MessageKind, Origin};
use crate::ui::transcript::view::ConversationLines;

use super::fixture::{self, OMARCHY, WINDOW_HEIGHT, WINDOW_WIDTH};

#[gpui::test]
fn short_transcript_sits_at_bottom(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("#help");
    assert_eq!(ui.selected_key(), "#help");
    assert!(
        !ui.overflows(),
        "#help should be shorter than the transcript viewport"
    );
    assert!(ui.at_end(), "a short transcript rests on the bottom");

    ui.cx.update(|window, cx| {
        ui.shell.update(cx, |shell, cx| {
            shell.open_status(OMARCHY, window, cx);
        });
    });
    ui.resize(WINDOW_WIDTH, 1200.);
    assert!(ui.row_count() > 0, "Status has seeded lines");
    assert!(ui.at_end(), "short Status rests on the bottom");
}

#[gpui::test]
fn short_transcript_grows_upward(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("anna");
    assert!(!ui.overflows(), "anna starts short");
    assert!(ui.at_end());

    let mut guard = 0;
    while !ui.overflows() {
        assert!(guard < 40, "a short list should overflow within 40 rows");
        ui.inject_chat("anna", "fred", &format!("grow {guard}"));
        assert!(ui.at_end(), "appending keeps the newest row on the bottom");
        guard += 1;
    }
    assert!(ui.at_end(), "once tall, the list still follows the end");
}

#[gpui::test]
fn short_transcript_resize_keeps_bottom(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("anna");
    ui.inject_chat("anna", "fred", "second");
    ui.inject_chat("anna", "fred", "third");
    assert!(!ui.overflows());
    assert!(ui.at_end());

    ui.resize(WINDOW_WIDTH, 240.);
    assert!(ui.at_end(), "shrinking the window keeps the short list pinned");

    ui.resize(WINDOW_WIDTH, WINDOW_HEIGHT);
    assert!(ui.at_end(), "growing the window keeps the short list pinned");
}

#[gpui::test]
fn following_append_stays_pinned_through_layout(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("#omarchy");
    ui.fill_until_scrollable("anna", "#omarchy", "scroll line");

    let before = ui.scroll_top();
    let previous = ui.row_count();
    ui.inject_chat("anna", "#omarchy", "incoming layout pin");
    assert_eq!(ui.row_count(), previous + 1);
    assert!(ui.at_end(), "following growth stays pinned while the row lays out");
    assert!(
        ui.scroll_top().item_ix >= before.item_ix,
        "following growth must not jump toward older lines"
    );
    assert!(ui.first_unseen().is_none(), "no unseen jump while following");
}

#[gpui::test]
fn detached_arrival_keeps_viewport_and_jumps_to_first_unseen(
    cx: &mut gpui::TestAppContext,
) {
    let mut ui = fixture::open(cx);
    ui.select("#omarchy");
    ui.fill_until_scrollable("anna", "#omarchy", "scroll line");

    ui.page_up();
    assert!(!ui.at_end(), "Page Up leaves the reader detached");
    let frozen = ui.scroll_top();

    let previous = ui.shell.update(ui.cx, |shell, cx| {
        let previous = shell.transcript.read(cx).rows().len();
        assert!(
            !shell.transcript.read(cx).at_end(),
            "must be detached before the arrival refresh"
        );
        previous
    });

    ui.shell.update(ui.cx, |shell, cx| {
        let bytes = b":anna!u@h PRIVMSG #omarchy :first unseen\r\n";
        shell
            .controller
            .demo()
            .expect("demo")
            .inject(OMARCHY, bytes);
        shell.controller.tick();
        shell.refresh_shown_conversation(cx);
        assert_eq!(
            shell.transcript.read(cx).first_unseen_index(),
            Some(previous),
            "first unseen arms on the pre-arrival row count"
        );
    });
    ui.settle();
    assert_eq!(ui.first_unseen(), Some(previous));
    assert!(
        !ui.at_end(),
        "after layout the detached viewport must stay off the end"
    );
    assert_eq!(
        ui.scroll_top().item_ix,
        frozen.item_ix,
        "detached arrival keeps the same top item"
    );

    ui.inject_chat("anna", "#omarchy", "later unseen");
    assert_eq!(
        ui.first_unseen(),
        Some(previous),
        "later arrivals keep the first unseen index"
    );
    assert_eq!(ui.scroll_top().item_ix, frozen.item_ix);
}

#[gpui::test]
fn history_splice_keeps_the_reader_on_the_same_message(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("#omarchy");
    let base: Vec<Message> = (0..30)
        .map(|i| tagged_chat("anna", &format!("line {i}"), i as i64 + 1, &format!("id-{i}")))
        .collect();
    let self_nick = ui
        .shell
        .read_with(ui.cx, |shell, _| shell.controller.current_nick_for(OMARCHY));
    let features = ui
        .shell
        .read_with(ui.cx, |shell, _| shell.controller.features(OMARCHY).clone());

    ui.shell.update(ui.cx, |shell, cx| {
        shell.transcript.update(cx, |t, _| {
            t.set_conversation(ConversationLines {
                messages: &base,
                mark: None,
                facts: HashMap::new(),
                self_nick: &self_nick,
                features: &features,
                typing_peer: None,
                today: chrono::Local::now().date_naive(),
            });
        });
    });
    ui.settle();
    ui.shell.update(ui.cx, |shell, cx| {
        shell.transcript.update(cx, |t, cx| {
            t.scroll_to_bottom(cx);
            for _ in 0..4 {
                t.scroll_page(false, false, cx);
            }
        });
    });
    ui.settle();
    assert!(!ui.at_end());

    let anchor_body = ui.shell.read_with(ui.cx, |shell, cx| {
        let top = shell.transcript.read(cx).logical_scroll_top();
        shell.transcript.read(cx).rows()[top.item_ix]
            .body
            .plain
            .clone()
    });

    let mut combined: Vec<Message> = (0..9)
        .map(|i| tagged_chat("anna", &format!("old {i}"), i as i64 + 1, &format!("old-{i}")))
        .collect();
    let mut rest = base;
    for (i, message) in rest.iter_mut().enumerate() {
        message.sequence = 100 + i as i64;
    }
    combined.append(&mut rest);

    ui.shell.update(ui.cx, |shell, cx| {
        shell.transcript.update(cx, |t, cx| {
            t.refresh_conversation(
                ConversationLines {
                    messages: &combined,
                    mark: None,
                    facts: HashMap::new(),
                    self_nick: &self_nick,
                    features: &features,
                    typing_peer: None,
                    today: chrono::Local::now().date_naive(),
                },
                cx,
            );
        });
    });
    ui.settle();

    let body = ui.shell.read_with(ui.cx, |shell, cx| {
        let top = shell.transcript.read(cx).logical_scroll_top();
        shell.transcript.read(cx).rows()[top.item_ix]
            .body
            .plain
            .clone()
    });
    assert_eq!(body, anchor_body, "splice must keep the reader on the same message");
}

#[gpui::test]
fn history_prepend_keeps_anchor_message(cx: &mut gpui::TestAppContext) {
    // Viewport-Y equality is QML ListView-specific; Rust proves the msgid anchor
    // survives a prepend through the same refresh/restore path.
    let mut ui = fixture::open(cx);
    ui.select("anna");
    let tail: Vec<Message> = (0..24)
        .map(|i| {
            tagged_chat(
                "anna",
                &format!("scroll line {i}"),
                i as i64 + 1,
                &format!("anna-{i}"),
            )
        })
        .collect();
    let self_nick = ui
        .shell
        .read_with(ui.cx, |shell, _| shell.controller.current_nick_for(OMARCHY));
    let features = ui
        .shell
        .read_with(ui.cx, |shell, _| shell.controller.features(OMARCHY).clone());

    ui.shell.update(ui.cx, |shell, cx| {
        shell.transcript.update(cx, |t, _| {
            t.set_conversation(ConversationLines {
                messages: &tail,
                mark: None,
                facts: HashMap::new(),
                self_nick: &self_nick,
                features: &features,
                typing_peer: None,
                today: chrono::Local::now().date_naive(),
            });
        });
    });
    ui.settle();
    ui.shell.update(ui.cx, |shell, cx| {
        shell.transcript.update(cx, |t, cx| {
            t.scroll_to_bottom(cx);
            for _ in 0..6 {
                t.scroll_page(false, false, cx);
            }
        });
    });
    ui.settle();
    assert!(!ui.at_end(), "reader must be detached before the prepend");

    let anchor_body = ui.shell.read_with(ui.cx, |shell, cx| {
        let top = shell.transcript.read(cx).logical_scroll_top();
        shell.transcript.read(cx).rows()[top.item_ix]
            .body
            .plain
            .clone()
    });

    let mut combined: Vec<Message> = (0..4)
        .map(|i| {
            tagged_chat(
                "anna",
                &format!("head day {i}"),
                i as i64 + 1,
                &format!("head-{i}"),
            )
        })
        .collect();
    let mut rest = tail;
    for (i, message) in rest.iter_mut().enumerate() {
        message.sequence = 50 + i as i64;
    }
    combined.append(&mut rest);

    ui.shell.update(ui.cx, |shell, cx| {
        shell.transcript.update(cx, |t, cx| {
            t.refresh_conversation(
                ConversationLines {
                    messages: &combined,
                    mark: None,
                    facts: HashMap::new(),
                    self_nick: &self_nick,
                    features: &features,
                    typing_peer: None,
                    today: chrono::Local::now().date_naive(),
                },
                cx,
            );
        });
    });
    ui.settle();

    let body = ui.shell.read_with(ui.cx, |shell, cx| {
        let rows = shell.transcript.read(cx).rows();
        let top = shell.transcript.read(cx).logical_scroll_top();
        let ix = top.item_ix.min(rows.len().saturating_sub(1));
        rows[ix].body.plain.clone()
    });
    assert_eq!(
        body, anchor_body,
        "prepend must keep the anchored message at the top"
    );
}

#[gpui::test]
fn capped_trim_keeps_detached_place(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("#omarchy");

    let mut batch = String::new();
    for index in 0..2000 {
        batch.push_str(&format!(
            "@time=2026-09-12T12:00:00.000Z @msgid=cap-{index} :anna!u@h PRIVMSG #omarchy :capped fill {index}\r\n"
        ));
    }
    ui.shell.update(ui.cx, |shell, cx| {
        shell
            .controller
            .demo()
            .expect("demo")
            .inject(OMARCHY, batch.as_bytes());
        shell.controller.tick();
        shell.refresh_shown_conversation(cx);
    });
    ui.settle();
    ui.shell.update(ui.cx, |shell, cx| {
        shell.transcript.update(cx, |t, cx| t.scroll_to_bottom(cx));
    });
    ui.settle();
    assert!(ui.at_end());

    ui.page_up();
    assert!(!ui.at_end());
    let place = ui.shell.update(ui.cx, |shell, cx| {
        shell
            .transcript
            .update(cx, |t, _| t.capture_scroll_place().to_stored())
    });
    assert!(
        !place.msg_id.is_empty() || !place.follow_end,
        "Page Up should leave a message place"
    );
    let anchor = place.msg_id.clone();

    ui.inject(
        OMARCHY,
        "@time=2026-09-12T12:02:00.000Z @msgid=cap-trim-new :anna!u@h PRIVMSG #omarchy :capped while detached",
    );
    assert!(!ui.at_end(), "trim while detached must not jump to the end");
    if !anchor.is_empty() {
        let top_msgid = ui.shell.read_with(ui.cx, |shell, cx| {
            let top = shell.transcript.read(cx).logical_scroll_top();
            shell.transcript.read(cx).rows()[top.item_ix].msgid.clone()
        });
        assert_eq!(top_msgid, anchor, "capped trim keeps the detached msgid place");
    }
}

fn tagged_chat(author: &str, body: &str, sequence: i64, msgid: &str) -> Message {
    Message {
        kind: MessageKind::Message,
        author: author.to_owned(),
        body: body.to_owned(),
        time: Utc.with_ymd_and_hms(2026, 9, 10, 9, 0, 0).unwrap(),
        server_time: None,
        msgid: MsgId::new(msgid),
        origin: Origin::Live,
        mentioned: false,
        collapsible: false,
        sequence,
        only_visible_to_you: false,
    }
}
