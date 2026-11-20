//! ZNC playback and chathistory catch-up.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{TimeZone, Utc};

use crate::controller::history::oldest_history_cursor;
use crate::controller::playback::PlaybackCoordinator;
use crate::controller::tests::fixture::{Fixture, OMARCHY};
use crate::controller::tests::persistence::{
    disk, feed, frame_count, launch, libera, save, Loopbacks, LIBERA,
};
use crate::irc::event::{HistoryEvent, HistoryKind, MessageKindTag, MsgId, ReplayLine};
use crate::irc::reducer::EventReducer;
use crate::irc::translator::HistoryTarget;
use crate::irc::world::{Message, MessageKind, Origin};
use crate::session::clock::FakeClock;
use crate::session::config::SessionConfig;
use crate::session::events::SessionState;
use crate::session::loopback::LoopbackTransport;
use crate::session::session::Session;
use crate::session::sts::MemoryStsStore;
use crate::storage::playbacktime::{play_stamp_at, PlaybackTimeStore};
use crate::util::testdir::TempDir;

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

fn registered_freenode() -> (Session, Rc<LoopbackTransport>) {
    let transport = Rc::new(LoopbackTransport::new());
    let clock = Rc::new(FakeClock::new(Utc.timestamp_opt(0, 0).single().unwrap()));
    let sts = Rc::new(MemoryStsStore::default());
    let config = SessionConfig {
        network_id: "freenode".to_owned(),
        host: "irc.example".to_owned(),
        port: 6667,
        tls_enabled: false,
        nick: "omairc".to_owned(),
        username: "omairc".to_owned(),
        realname: "Omairc".to_owned(),
        autojoin_channels: Vec::new(),
        ..SessionConfig::default()
    };
    let session = Session::new(config, transport.clone(), clock, sts);
    session.start();
    transport.complete_connect();
    session.locked(|core| {
        core.set_state(SessionState::Registered);
        core.features.apply_token("CHANTYPES=#");
    });
    (session, transport)
}

fn chathistory_session() -> (Session, Rc<LoopbackTransport>) {
    let (session, transport) = registered_freenode();
    transport.inject_bytes(
        b":server CAP omairc LS :batch chathistory\r\n\
          :server CAP omairc ACK :batch chathistory\r\n\
          :server 001 omairc :Welcome\r\n\
          :server 376 omairc :End of MOTD\r\n",
    );
    assert_eq!(session.state(), SessionState::Registered);
    (session, transport)
}

fn record_znc(sent: Rc<RefCell<Vec<String>>>) -> Box<dyn Fn(&Session, &str, &str) -> bool> {
    Box::new(move |_session, target, from| {
        sent.borrow_mut().push(format!("{target} {from}"));
        true
    })
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
fn note_joined_channel_dedupes_with_case_mapping() {
    let (mut playback, _, mut reducer) = coordinator();
    let mut features = reducer.server_features("freenode").clone();
    features.apply_token("CASEMAPPING=ascii");
    reducer.set_server_features("freenode", features);
    playback.note_joined_channel(&reducer, "freenode", "#chan");
    playback.note_joined_channel(&reducer, "freenode", "#CHAN");
    playback.note_joined_channel(&reducer, "freenode", "#other");
    let sent = Rc::new(RefCell::new(Vec::new()));
    let (session, _) = registered_freenode();
    playback.set_send_znc(record_znc(Rc::clone(&sent)));
    playback.request(&session, &reducer, true, true, &[]);
    let plays = sent.borrow().iter().filter(|line| line.starts_with("#")).count();
    assert_eq!(plays, 2, "joined channels: {:?}", sent.borrow());
}

#[test]
fn request_empty_snapshot_sends_play_all() {
    let (mut playback, _, reducer) = coordinator();
    let (session, transport) = registered_freenode();
    transport.inject_bytes(b":server 001 omairc :Welcome\r\n");
    assert_eq!(session.state(), SessionState::Registered);
    let sent = Rc::new(RefCell::new(Vec::new()));
    playback.set_send_znc(record_znc(Rc::clone(&sent)));
    playback.on_registered("freenode", &[]);
    playback.request(&session, &reducer, true, true, &[]);
    assert_eq!(sent.borrow().as_slice(), &["* 0".to_owned()]);
}

#[test]
fn request_snapshot_sends_per_target_and_queries() {
    let (mut playback, times, reducer) = coordinator();
    let mapping = reducer.server_features("freenode").case_mapping();
    let when = Utc.with_ymd_and_hms(2026, 9, 26, 9, 0, 0).unwrap();
    assert!(times.borrow_mut().note("freenode", "#chan", when, mapping));
    let (session, _) = registered_freenode();
    let sent = Rc::new(RefCell::new(Vec::new()));
    playback.set_send_znc(record_znc(Rc::clone(&sent)));
    playback.on_registered("freenode", &[]);
    playback.request(&session, &reducer, true, true, &[]);
    assert_eq!(
        sent.borrow().as_slice(),
        &[
            format!("#chan {}", play_stamp_at(when, true)),
            "* 0".to_owned(),
        ]
    );
}

#[test]
fn trim_bouncer_batch_respects_snapshot_bound() {
    let (mut playback, times, reducer) = coordinator();
    let mapping = reducer.server_features("freenode").case_mapping();
    let when = Utc.with_ymd_and_hms(2026, 9, 26, 10, 0, 0).unwrap();
    assert!(times.borrow_mut().note("freenode", "#chan", when, mapping));
    playback.on_registered("freenode", &[]);
    playback
        .request(&registered_freenode().0, &reducer, true, true, &[]);
    let older = when - chrono::Duration::milliseconds(1);
    let newer = when + chrono::Duration::milliseconds(1);
    let line = |body: &str, server_time: Option<chrono::DateTime<Utc>>, msgid: &str| ReplayLine {
        author: String::new(),
        body: body.to_owned(),
        timestamp: server_time.unwrap_or(when),
        kind: MessageKindTag::Chat,
        msgid: MsgId {
            value: msgid.to_owned(),
        },
        reply_to: MsgId::default(),
        server_time,
        only_visible_to_you: false,
    };
    let mut event = HistoryEvent {
        target: "#chan".to_owned(),
        kind: HistoryKind::BouncerPlayback,
        older_page: false,
        lines: vec![
            line("older", Some(older), ""),
            line("equal-no-msgid", Some(when), ""),
            line("equal-msgid", Some(when), "m1"),
            line("newer", Some(newer), ""),
            line("notime", None, ""),
        ],
        conversation: reducer.conversation_key("freenode", "#chan"),
    };
    playback.trim_bouncer_batch("freenode", &reducer, &mut event);
    let bodies = event.lines.iter().map(|line| line.body.as_str()).collect::<Vec<_>>();
    assert_eq!(
        bodies,
        &["equal-msgid", "newer", "notime"]
    );
}

#[test]
fn catch_up_retry_targets_once() {
    let (mut playback, times, reducer) = coordinator();
    let mapping = reducer.server_features("freenode").case_mapping();
    let when = Utc.with_ymd_and_hms(2024, 3, 9, 16, 0, 0).unwrap()
        + chrono::Duration::milliseconds(620);
    assert!(times.borrow_mut().note("freenode", "#omarchy", when, mapping));
    let (session, transport) = chathistory_session();
    playback.on_registered("freenode", &[]);
    let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
    playback.request_catch_up(&session, &reducer, true, true, &[], now);
    assert!(
        transport
            .written_frames()
            .iter()
            .any(|frame| frame.starts_with(b"CHATHISTORY TARGETS ")),
        "first TARGETS missing"
    );
    assert!(playback.retry_targets("freenode"));
    transport.inject_bytes(b":server FAIL CHATHISTORY MESSAGE_ERROR TARGETS :no\r\n");
    playback.request_catch_up(&session, &reducer, true, true, &[], now);
    let count = transport
        .written_frames()
        .iter()
        .filter(|frame| frame.starts_with(b"CHATHISTORY TARGETS "))
        .count();
    assert_eq!(count, 2, "retry must send TARGETS again");
    assert!(!playback.retry_targets("freenode"));
    playback.request_catch_up(&session, &reducer, true, true, &[], now);
    assert_eq!(
        transport
            .written_frames()
            .iter()
            .filter(|frame| frame.starts_with(b"CHATHISTORY TARGETS "))
            .count(),
        2,
        "second retry must not send TARGETS"
    );
}

#[test]
fn discovered_empty_after_drops_query() {
    let (mut playback, times, mut reducer) = coordinator();
    let mapping = reducer.server_features("freenode").case_mapping();
    let when = Utc.with_ymd_and_hms(2024, 3, 9, 16, 0, 0).unwrap()
        + chrono::Duration::milliseconds(620);
    assert!(times.borrow_mut().note("freenode", "#omarchy", when, mapping));
    let (session, _) = registered_freenode();
    playback.on_registered("freenode", &[]);
    let targets = [HistoryTarget {
        name: "nobody".to_owned(),
        latest: Some(when),
    }];
    let created = playback.note_discovered_targets(
        &session,
        &mut reducer,
        &targets,
        "omairc",
        &[],
        &[],
    );
    assert_eq!(created, 0);
    let key = reducer.conversation_key("freenode", "nobody");
    assert!(reducer.find(&key).is_none(), "empty discovered query must drop");
}

#[test]
fn select_clears_history_page_cap_on_previous() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    let key = fixture
        .controller
        .reducer
        .conversation_key(OMARCHY, "#omarchy");
    fixture.controller.reducer.mark_history_page_cap_tail(&key);
    assert!(
        fixture
            .controller
            .reducer
            .find(&key)
            .is_some_and(|conversation| conversation.history_page_cap_tail)
    );
    fixture.controller.select_conversation(OMARCHY, "#desktop");
    assert!(
        fixture
            .controller
            .reducer
            .find(&key)
            .is_some_and(|conversation| !conversation.history_page_cap_tail)
    );
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
            reply_to: String::new(),
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
            reply_to: String::new(),
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

/// Ergo offers `znc.in/playback` and `chathistory` together. A `ZNC *playback
/// PLAY` there makes Ergo answer with `chathistory` batches the session takes for
/// CHATHISTORY answers: the join replay settles LATEST, and the PLAY replay lands
/// its older lines below the newer page. CHATHISTORY owns history when both are
/// offered, so no PLAY is sent and the playback clock keeps moving for it.
#[test]
fn chathistory_owns_history_when_znc_playback_is_also_offered() {
    let dir = TempDir::new("chathistory-over-znc");
    let backing = disk(&dir);
    let loopbacks = Loopbacks::default();
    let mut controller = launch(&backing, Box::new(loopbacks.clone()));
    save(&mut controller, libera(LIBERA));

    let mapping = controller.reducer.server_features(LIBERA).case_mapping();
    let stamped = Utc.with_ymd_and_hms(2024, 3, 9, 16, 0, 0).unwrap()
        + chrono::Duration::milliseconds(620);
    assert!(controller
        .playback_times
        .borrow_mut()
        .note(LIBERA, "#omarchy", stamped, mapping));

    assert!(controller.start(LIBERA));
    feed(
        &mut controller,
        &loopbacks,
        LIBERA,
        ":irc.libera.chat CAP fred LS :batch chathistory znc.in/playback\r\n\
         :irc.libera.chat CAP fred ACK :batch chathistory\r\n\
         :irc.libera.chat CAP fred ACK :znc.in/playback\r\n\
         :irc.libera.chat 001 fred :Welcome\r\n\
         :irc.libera.chat 376 fred :End of MOTD\r\n\
         :fred!u@h JOIN :#omarchy\r\n\
         :fred!u@h JOIN :#fresh",
    );
    let frames = || {
        loopbacks
            .get(LIBERA)
            .written_frames()
            .iter()
            .map(|frame| String::from_utf8_lossy(frame).into_owned())
            .collect::<Vec<_>>()
    };
    assert!(
        controller
            .session(LIBERA)
            .expect("session")
            .capabilities()
            .contains(crate::irc::capability::IrcCapability::ZncPlayback),
        "znc.in/playback stays negotiated; frames: {:?}",
        frames()
    );
    assert_eq!(
        frame_count(&loopbacks, LIBERA, "ZNC *playback PLAY"),
        0,
        "chathistory owns history; frames: {:?}",
        frames()
    );
    assert_eq!(
        frame_count(&loopbacks, LIBERA, "CHATHISTORY AFTER #omarchy timestamp=2024-03-09T16:00:00.620Z"),
        1,
        "frames: {:?}",
        frames()
    );
    assert_eq!(
        frame_count(&loopbacks, LIBERA, "CHATHISTORY LATEST #fresh * "),
        1,
        "frames: {:?}",
        frames()
    );

    // The AFTER answer: lines kept from a CHATHISTORY batch move the clock
    // with no live line in the channel.
    feed(
        &mut controller,
        &loopbacks,
        LIBERA,
        ":irc.libera.chat BATCH +after chathistory #omarchy\r\n\
         @batch=after;time=2024-03-09T17:00:00.000Z;msgid=a1 :bob!u@h PRIVMSG #omarchy :one\r\n\
         @batch=after;time=2024-03-09T17:05:00.500Z;msgid=a2 :bob!u@h PRIVMSG #omarchy :two\r\n\
         :irc.libera.chat BATCH -after",
    );
    let caught_up = Utc.with_ymd_and_hms(2024, 3, 9, 17, 5, 0).unwrap()
        + chrono::Duration::milliseconds(500);
    assert_eq!(
        controller
            .playback_times
            .borrow()
            .noted(LIBERA, "#omarchy", mapping),
        Some(caught_up),
        "kept CHATHISTORY lines move the resume clock"
    );

    feed(
        &mut controller,
        &loopbacks,
        LIBERA,
        "@time=2024-03-10T09:30:00.250Z :alice!u@h PRIVMSG #fresh :hi",
    );
    let heard = Utc.with_ymd_and_hms(2024, 3, 10, 9, 30, 0).unwrap()
        + chrono::Duration::milliseconds(250);
    assert_eq!(
        controller
            .playback_times
            .borrow()
            .noted(LIBERA, "#fresh", mapping),
        Some(heard),
        "the CHATHISTORY resume clock still follows live lines"
    );
}

#[test]
fn cap_del_chathistory_hands_history_to_znc_playback() {
    let dir = TempDir::new("cap-del-chathistory");
    let backing = disk(&dir);
    let loopbacks = Loopbacks::default();
    let mut controller = launch(&backing, Box::new(loopbacks.clone()));
    save(&mut controller, libera(LIBERA));
    let mapping = controller.reducer.server_features(LIBERA).case_mapping();

    assert!(controller.start(LIBERA));
    feed(
        &mut controller,
        &loopbacks,
        LIBERA,
        ":irc.libera.chat CAP fred LS :batch chathistory znc.in/playback\r\n\
         :irc.libera.chat CAP fred ACK :batch chathistory\r\n\
         :irc.libera.chat CAP fred ACK :znc.in/playback\r\n\
         :irc.libera.chat 001 fred :Welcome\r\n\
         :irc.libera.chat 376 fred :End of MOTD\r\n\
         :fred!u@h JOIN :#omarchy",
    );
    assert_eq!(frame_count(&loopbacks, LIBERA, "ZNC *playback PLAY"), 0);

    feed(
        &mut controller,
        &loopbacks,
        LIBERA,
        ":irc.libera.chat CAP fred DEL :chathistory",
    );
    assert_eq!(
        frame_count(&loopbacks, LIBERA, "ZNC *playback PLAY * 0"),
        1,
        "ZNC playback owns history after CAP DEL"
    );
    feed(
        &mut controller,
        &loopbacks,
        LIBERA,
        "@time=2024-03-10T09:30:00.250Z :alice!u@h PRIVMSG #omarchy :hi",
    );
    let heard = Utc.with_ymd_and_hms(2024, 3, 10, 9, 30, 0).unwrap()
        + chrono::Duration::milliseconds(250);
    assert_eq!(
        controller
            .playback_times
            .borrow()
            .noted(LIBERA, "#omarchy", mapping),
        Some(heard),
        "the playback clock still moves after CAP DEL"
    );
}
