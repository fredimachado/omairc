//! ZNC playback and chathistory catch-up. Ports `playback_test.go` and
//! `history_cursor_test.go`.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{TimeZone, Utc};

use crate::controller::history::oldest_history_cursor;
use crate::controller::playback::PlaybackCoordinator;
use crate::irc::event::MsgId;
use crate::irc::reducer::EventReducer;
use crate::irc::world::{Message, MessageKind, Origin};
use crate::storage::playbacktime::PlaybackTimeStore;

fn coordinator() -> (PlaybackCoordinator, Rc<RefCell<PlaybackTimeStore>>, EventReducer) {
    let times = Rc::new(RefCell::new(PlaybackTimeStore::new(
        &crate::storage::Backing::Ephemeral,
    )));
    times.borrow_mut().set_ephemeral(true);
    let reducer = EventReducer::new();
    let coordinator = PlaybackCoordinator::new(Rc::clone(&times));
    (coordinator, times, reducer)
}

fn privmsg_time(target: &str, nick: &str, when: chrono::DateTime<Utc>) -> crate::irc::message::IrcMessage {
    crate::irc::parser::parse(
        format!(
            "@time={} :{}!u@h PRIVMSG {} :hi",
            when.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            nick,
            target
        )
        .as_bytes(),
    )
    .expect("line")
}

#[test]
fn on_registered_snapshots_stored_times() {
    let (mut playback, times, reducer) = coordinator();
    let mapping = reducer.server_features("freenode").case_mapping();
    let when = Utc.with_ymd_and_hms(2026, 9, 26, 9, 0, 0).unwrap();
    assert!(times.borrow_mut().note("freenode", "#chan", when, mapping));
    playback.on_registered("freenode", &["#auto".to_owned()]);
    assert_eq!(
        playback.resume_time(&reducer, "freenode", "#chan"),
        Some(when)
    );
    playback.on_left_registration("freenode");
    assert!(playback.resume_time(&reducer, "freenode", "#chan").is_none());
}

#[test]
fn note_playback_clock_notices_newer_server_time() {
    let (playback, times, reducer) = coordinator();
    let mapping = reducer.server_features("freenode").case_mapping();
    let when = Utc.with_ymd_and_hms(2026, 9, 26, 9, 0, 0).unwrap();
    let message = privmsg_time("omairc", "alice", when);
    playback.note_playback_clock(&reducer, "freenode", &message, "omairc", false);
    assert_eq!(times.borrow().noted("freenode", "alice", mapping), Some(when));
    let older = when - chrono::Duration::hours(1);
    let message = privmsg_time("omairc", "alice", older);
    playback.note_playback_clock(&reducer, "freenode", &message, "omairc", false);
    assert_eq!(times.borrow().noted("freenode", "alice", mapping), Some(when));
}

#[test]
fn oldest_history_cursor_prefers_first_msgid() {
    let when = Utc.with_ymd_and_hms(2011, 10, 19, 16, 40, 50).unwrap();
    let messages = vec![
        Message {
            kind: MessageKind::Notice,
            body: "note".to_owned(),
            time: when,
            msgid: MsgId {
                value: "notice-id".to_owned(),
            },
            origin: Origin::Live,
            author: String::new(),
            server_time: None,
            mentioned: false,
            collapsible: false,
            sequence: 0,
            only_visible_to_you: false,
        },
        Message {
            kind: MessageKind::Message,
            body: "chat".to_owned(),
            time: when,
            msgid: MsgId {
                value: "chat-id".to_owned(),
            },
            origin: Origin::Live,
            author: String::new(),
            server_time: None,
            mentioned: false,
            collapsible: false,
            sequence: 1,
            only_visible_to_you: false,
        },
    ];
    let (msgid, _) = oldest_history_cursor(&messages).expect("cursor");
    assert_eq!(msgid, "notice-id");
}
