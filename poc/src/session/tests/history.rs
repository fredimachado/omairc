//! Port of `tui/internal/session/history_after_test.go` and
//! `history_before_test.go`: the CHATHISTORY LATEST, AFTER and BEFORE requests,
//! the AFTER chain, TARGETS, and the failure paths.

use chrono::{DateTime, TimeDelta, TimeZone, Utc};

use super::batch::history_config;
use super::fixture::Fixture;
use crate::session::events::{HistoryKind, SessionState};

/// A UTC instant with a millisecond part.
fn at(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millis: i64,
) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .unwrap()
        + TimeDelta::milliseconds(millis)
}

/// The resume time the self-JOIN tests use: 2024-03-09T16:00:00.620Z.
fn resume_time() -> DateTime<Utc> {
    at(2024, 3, 9, 16, 0, 0, 620)
}

/// Resumes `#omarchy` at `when`, and no other target (`resumeOmarchy`).
fn resume_omarchy(fx: &Fixture, when: DateTime<Utc>) {
    fx.session.set_history_resume(Some(Box::new(move |target, _| {
        (target == "#omarchy").then_some(when)
    })));
}

/// Count of frames containing `needle`.
fn count_containing(frames: &[String], needle: &str) -> usize {
    frames.iter().filter(|frame| frame.contains(needle)).count()
}

/// The chathistory-capable registration the AFTER tests use.
fn chathistory_registered(fx: &Fixture) {
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n",
    );
}

/// The registration plus a self-JOIN of `#omarchy` (`completeHistoryJoin`).
fn complete_history_join(fx: &Fixture) {
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n\
         :server 376 omairc :End of MOTD\r\n\
         :omairc!u@h JOIN :#omarchy\r\n",
    );
}

/// One older line closing the LATEST batch (`finishLatestBatch`).
fn finish_latest_batch(fx: &Fixture) {
    fx.inject(
        ":irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=hx;time=2011-10-19T16:40:51.620Z;msgid=old :alice!u@h PRIVMSG #omarchy :older\r\n\
         :irc.host BATCH -hx\r\n",
    );
}

/// Sends one page of an AFTER chain as a batch with a msgid.
fn page(fx: &Fixture, reference: &str, msgid: &str) {
    fx.inject(&format!(
        ":irc.host BATCH +{reference} chathistory #omarchy\r\n\
         @batch={reference};time=2024-03-09T16:00:01.000Z;msgid={msgid} :bob!u@h PRIVMSG #omarchy :line\r\n\
         :irc.host BATCH -{reference}\r\n"
    ));
}

// ---- history_after_test.go ------------------------------------------------

#[test]
fn self_join_resumes_chat_history_after() {
    let fx = Fixture::new(history_config());
    resume_omarchy(&fx, resume_time());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n\
         :omairc!u@h JOIN :#omarchy\r\n",
    );
    let want = "CHATHISTORY AFTER #omarchy timestamp=2024-03-09T16:00:00.620Z 100\r\n";
    assert!(fx.wrote(want), "frames = {:?}", fx.frames());
    assert!(
        !fx.wrote("CHATHISTORY LATEST #omarchy * 100\r\n"),
        "resume sent LATEST"
    );
}

#[test]
fn empty_after_does_not_exhaust_before() {
    let fx = Fixture::new(history_config());
    fx.session
        .set_history_resume(Some(Box::new(|_, _| Some(resume_time()))));
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n\
         :omairc!u@h JOIN :#omarchy\r\n\
         :irc.host BATCH +empty chathistory #omarchy\r\n\
         :irc.host BATCH -empty\r\n",
    );
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "BEFORE after an empty AFTER must succeed"
    );
    assert!(
        fx.wrote("CHATHISTORY BEFORE #omarchy msgid=old 100\r\n"),
        "frames = {:?}",
        fx.frames()
    );
}

#[test]
fn fail_after_does_not_exhaust_before() {
    let fx = Fixture::new(history_config());
    fx.session
        .set_history_resume(Some(Box::new(|_, _| Some(resume_time()))));
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n\
         :omairc!u@h JOIN :#omarchy\r\n\
         :server FAIL CHATHISTORY INVALID_TARGET AFTER #omarchy :no\r\n",
    );
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "BEFORE after FAIL AFTER must succeed"
    );
    assert!(
        fx.wrote("CHATHISTORY BEFORE #omarchy msgid=old 100\r\n"),
        "frames = {:?}",
        fx.frames()
    );
    assert_eq!(fx.session.state(), SessionState::Registered);
}

#[test]
fn after_chains_three_pages_then_latest() {
    let fx = Fixture::new(history_config());
    fx.session.set_history_after_page_cap(3);
    resume_omarchy(&fx, resume_time());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n\
         :server 005 omairc CHATHISTORY=1 :are supported\r\n\
         :omairc!u@h JOIN :#omarchy\r\n",
    );
    page(&fx, "p1", "m1");
    page(&fx, "p2", "m2");
    page(&fx, "p3", "m3");
    let frames = fx.frames();
    assert_eq!(
        count_containing(&frames, "CHATHISTORY AFTER "),
        3,
        "frames {frames:?}"
    );
    assert!(
        fx.wrote("CHATHISTORY LATEST #omarchy msgid=m3 1\r\n"),
        "frames {frames:?}"
    );

    fx.inject(":irc.host BATCH +tail chathistory #omarchy\r\n:irc.host BATCH -tail\r\n");
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "BEFORE after the cap LATEST must succeed"
    );
    assert!(
        fx.wrote("CHATHISTORY BEFORE #omarchy msgid=old 1\r\n"),
        "frames {:?}",
        fx.frames()
    );
}

#[test]
fn after_short_batch_and_end_tag_stop() {
    let fx = Fixture::new(history_config());
    resume_omarchy(&fx, resume_time());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n\
         :omairc!u@h JOIN :#omarchy\r\n\
         :irc.host BATCH +short chathistory #omarchy\r\n\
         @batch=short;time=2024-03-09T16:00:01.000Z;msgid=one :bob!u@h PRIVMSG #omarchy :line\r\n\
         :irc.host BATCH -short\r\n",
    );
    assert_eq!(
        count_containing(&fx.frames(), "CHATHISTORY AFTER "),
        1,
        "short batch chained"
    );

    let ended = Fixture::new(history_config());
    ended.session.set_history_after_page_cap(10);
    resume_omarchy(&ended, resume_time());
    ended.connect_tls();
    ended.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n\
         :server 005 omairc CHATHISTORY=1 :are supported\r\n\
         :omairc!u@h JOIN :#omarchy\r\n\
         @draft/chathistory-end :irc.host BATCH +end chathistory #omarchy\r\n\
         @batch=end;time=2024-03-09T16:00:01.000Z;msgid=only :bob!u@h PRIVMSG #omarchy :line\r\n\
         :irc.host BATCH -end\r\n",
    );
    assert_eq!(
        count_containing(&ended.frames(), "CHATHISTORY AFTER "),
        1,
        "end tag chained"
    );
}

#[test]
fn after_same_time_uses_msgid() {
    let fx = Fixture::new(history_config());
    resume_omarchy(&fx, resume_time());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n\
         :server 005 omairc CHATHISTORY=2 :are supported\r\n\
         :omairc!u@h JOIN :#omarchy\r\n\
         :irc.host BATCH +p chathistory #omarchy\r\n\
         @batch=p;time=2024-03-09T16:00:01.000Z;msgid=first :a!u@h PRIVMSG #omarchy :one\r\n\
         @batch=p;time=2024-03-09T16:00:01.000Z;msgid=second :b!u@h PRIVMSG #omarchy :two\r\n\
         :irc.host BATCH -p\r\n",
    );
    assert!(
        fx.wrote("CHATHISTORY AFTER #omarchy msgid=second 2\r\n"),
        "frames = {:?}",
        fx.frames()
    );
}

#[test]
fn after_send_failure_does_not_stick() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n",
    );
    assert!(
        !fx.session.request_history_after("#omarchy", resume_time()),
        "AFTER before registration must fail"
    );
    fx.inject(":server 001 omairc :Welcome\r\n");
    assert!(
        fx.session.request_history_after("#omarchy", resume_time()),
        "AFTER after a failed send must succeed"
    );
    assert!(
        fx.wrote("CHATHISTORY AFTER #omarchy timestamp=2024-03-09T16:00:00.620Z 100\r\n"),
        "frames = {:?}",
        fx.frames()
    );
}

#[test]
fn cap_latest_send_failure_clears_asked() {
    let fx = Fixture::new(history_config());
    fx.session.set_history_after_page_cap(1);
    resume_omarchy(&fx, resume_time());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n\
         :server 005 omairc CHATHISTORY=1 :are supported\r\n\
         :omairc!u@h JOIN :#omarchy\r\n",
    );
    // A msgid this long pushes the LATEST bound past the frame, so it is refused.
    let msgid = "a".repeat(480);
    fx.inject(&format!(
        ":irc.host BATCH +p chathistory #omarchy\r\n\
         @batch=p;time=2024-03-09T16:00:01.000Z;msgid={msgid} :bob!u@h PRIVMSG #omarchy :line\r\n\
         :irc.host BATCH -p\r\n"
    ));
    assert!(
        !fx.frames()
            .iter()
            .any(|frame| frame.contains("CHATHISTORY LATEST ")),
        "oversized LATEST was sent: {:?}",
        fx.frames()
    );
    assert!(
        fx.session.request_history_latest("#omarchy"),
        "failed cap LATEST left the target asked"
    );
    assert!(
        fx.wrote("CHATHISTORY LATEST #omarchy * 1\r\n"),
        "frames = {:?}",
        fx.frames()
    );
}

#[test]
fn history_targets_command_and_batch() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n",
    );
    let from = at(2024, 3, 9, 15, 59, 59, 620);
    let until = at(2026, 10, 4, 12, 0, 10, 0);
    assert!(fx.session.request_history_targets(from, until));
    assert!(
        fx.wrote(
            "CHATHISTORY TARGETS timestamp=2024-03-09T15:59:59.620Z timestamp=2026-10-04T12:00:10.000Z 100\r\n"
        ),
        "frames = {:?}",
        fx.frames()
    );

    fx.inject(
        ":irc.host BATCH +t draft/chathistory-targets\r\n\
         @batch=t :irc.host CHATHISTORY TARGETS lena 2024-03-09T16:00:00.620Z\r\n\
         @batch=t :irc.host CHATHISTORY TARGETS alice timestamp=2024-03-09T16:00:01.000Z\r\n\
         :irc.host BATCH -t\r\n",
    );
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    let batch = &batches[0];
    assert_eq!(batch.kind, Some(HistoryKind::Targets));
    assert_eq!(batch.targets.len(), 2);
    assert!(batch.lines.is_empty());
    assert_eq!(batch.targets[0].name, "lena");
    assert_eq!(batch.targets[0].latest, Some(at(2024, 3, 9, 16, 0, 0, 620)));
    assert_eq!(batch.targets[1].name, "alice");
}

#[test]
fn fail_targets_can_be_asked_again() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n",
    );
    let from = at(2024, 3, 9, 15, 59, 59, 620);
    let until = at(2026, 10, 4, 12, 0, 10, 0);
    assert!(
        fx.session.request_history_targets(from, until),
        "first TARGETS"
    );
    fx.inject(":server FAIL CHATHISTORY MESSAGE_ERROR TARGETS :no\r\n");
    assert!(
        fx.session.request_history_targets(from, until),
        "TARGETS after FAIL must succeed"
    );
    let count = fx
        .frames()
        .iter()
        .filter(|frame| frame.contains("CHATHISTORY TARGETS "))
        .count();
    assert_eq!(count, 2);
}

// ---- history_before_test.go -----------------------------------------------

#[test]
fn page_up_at_top_sends_before_with_oldest_msgid() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "RequestOlderHistory must succeed"
    );
    assert!(
        fx.wrote("CHATHISTORY BEFORE #omarchy msgid=old 100\r\n"),
        "frames = {:?}",
        fx.frames()
    );
}

#[test]
fn before_without_msgid_uses_timestamp() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    let when = at(2011, 10, 19, 16, 40, 51, 620);
    assert!(fx.session.request_older_history("#omarchy", "", Some(when)));
    assert!(
        fx.wrote("CHATHISTORY BEFORE #omarchy timestamp=2011-10-19T16:40:51.620Z 100\r\n"),
        "frames = {:?}",
        fx.frames()
    );
}

#[test]
fn inflight_second_before_does_not_send() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "first request must succeed"
    );
    assert!(
        !fx.session.request_older_history("#omarchy", "old", None),
        "second request while pending must fail"
    );
    assert_eq!(count_containing(&fx.frames(), "CHATHISTORY BEFORE"), 1);
}

#[test]
fn empty_batch_stops_further_before_until_rejoin() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "first BEFORE must succeed"
    );
    fx.inject(":irc.host BATCH +empty chathistory #omarchy\r\n:irc.host BATCH -empty\r\n");
    assert!(
        !fx.session.request_older_history("#omarchy", "old", None),
        "BEFORE after empty batch must fail"
    );
    assert!(
        fx.errors().is_empty(),
        "unexpected errors: {:?}",
        fx.errors()
    );
    assert_eq!(fx.session.state(), SessionState::Registered);

    fx.inject(":omairc!u@h PART #omarchy\r\n:omairc!u@h JOIN :#omarchy\r\n");
    fx.inject(
        ":irc.host BATCH +hx2 chathistory #omarchy\r\n\
         @batch=hx2;msgid=older2 :alice!u@h PRIVMSG #omarchy :again\r\n\
         :irc.host BATCH -hx2\r\n",
    );
    assert!(
        fx.session.request_older_history("#omarchy", "older2", None),
        "BEFORE after rejoin must succeed"
    );
}

#[test]
fn fail_chat_history_stops_before_without_disconnect() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "first BEFORE must succeed"
    );
    fx.inject(":server FAIL CHATHISTORY INVALID_TARGET BEFORE #omarchy :no\r\n");
    assert!(
        !fx.session.request_older_history("#omarchy", "old", None),
        "BEFORE after FAIL must fail"
    );
    assert!(
        fx.errors().is_empty(),
        "unexpected errors: {:?}",
        fx.errors()
    );
}

#[test]
fn join_still_sends_exactly_one_latest() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    assert_eq!(count_containing(&fx.frames(), "CHATHISTORY LATEST"), 1);
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "BEFORE must be allowed after LATEST"
    );
}

#[test]
fn no_cap_page_up_sends_nothing() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch\r\n\
         :server CAP omairc ACK :batch\r\n\
         :server 001 omairc :Welcome\r\n\
         :server 376 omairc :End of MOTD\r\n\
         :omairc!u@h JOIN :#omarchy\r\n",
    );
    assert!(
        !fx.session.request_older_history("#omarchy", "old", None),
        "BEFORE without chathistory must fail"
    );
    assert!(!fx
        .frames()
        .iter()
        .any(|frame| frame.contains("CHATHISTORY")));
}

#[test]
fn before_batch_sets_older_page() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "BEFORE must succeed"
    );
    fx.inject(
        ":irc.host BATCH +older chathistory #omarchy\r\n\
         @batch=older;msgid=page :bob!u@h PRIVMSG #omarchy :backlog\r\n\
         :irc.host BATCH -older\r\n",
    );
    let batches = fx.batches();
    assert_eq!(batches.len(), 2);
    assert!(batches[1].older_page, "BEFORE batch must set older_page");
}

#[test]
fn before_batch_is_delivered() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "BEFORE must succeed"
    );
    fx.inject(
        ":irc.host BATCH +older chathistory #omarchy\r\n\
         @batch=older;msgid=page :bob!u@h PRIVMSG #omarchy :backlog\r\n\
         :irc.host BATCH -older\r\n",
    );
    assert_eq!(fx.batches().len(), 2);
}

#[test]
fn before_ignores_stale_history_pending_generation() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    fx.session.locked(|core| {
        let folded = core.fold_channel("#omarchy");
        core.history.pending.insert(folded.clone(), 0);
        core.history.generation.insert(folded, 2);
    });
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "BEFORE must ignore a stale history pending generation"
    );
}

#[test]
fn reconnect_clears_history_pending_before() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "BEFORE must succeed"
    );
    fx.session.locked(|core| {
        assert!(
            core.history
                .pending_before
                .contains(&core.fold_channel("#omarchy")),
            "pending BEFORE must be set while the page is in flight"
        );
    });
    fx.inject(":server ERROR :gone\r\n");
    assert_eq!(fx.session.state(), SessionState::Reconnecting);
    fx.fire_reconnect();
    fx.transport.complete_connect();
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n\
         :server 376 omairc :End of MOTD\r\n",
    );
    fx.session.locked(|core| {
        assert!(
            core.history.pending_before.is_empty(),
            "pending BEFORE must clear on reconnect"
        );
    });
    fx.inject(
        ":irc.host BATCH +unsol chathistory #omarchy\r\n\
         @batch=unsol;msgid=fresh :alice!u@h PRIVMSG #omarchy :latest\r\n\
         :irc.host BATCH -unsol\r\n",
    );
    let batches = fx.batches();
    let last = batches.last().expect("unsolicited batch must be delivered");
    assert!(
        !last.older_page,
        "unsolicited LATEST after reconnect must not set older_page"
    );
}

#[test]
fn before_blocked_until_latest_on_channel() {
    let fx = Fixture::new(history_config());
    chathistory_registered(&fx);
    fx.inject(
        ":server 376 omairc :End of MOTD\r\n\
         :omairc!u@h JOIN :#omarchy\r\n",
    );
    assert!(
        !fx.session.request_older_history("#omarchy", "old", None),
        "BEFORE before LATEST batch finishes must fail when pending"
    );
}
