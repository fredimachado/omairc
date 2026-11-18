//! The one-phase-per-target CHATHISTORY ledger (#467): generation bumps, late
//! batches, the AFTER page cap, and the TARGETS latch, through the public session
//! API and the phase ledger. Go twins live in `tui/internal/session`.

use super::batch::history_config;
use super::fixture::Fixture;
use super::history::{
    at, complete_history_join, count_containing, finish_latest_batch, page, resume_omarchy,
    resume_time,
};
use crate::session::history::{HistoryCursor, HistoryPhase, InFlightRequest};

// Behavior pins for the one-phase-per-target ledger (#467). They use the public
// session API only, so they hold before and after the ledger refactor.

/// Registers `chathistory` with a page size of one, so a single line is a full
/// page, then self-joins `#omarchy`.
fn page_size_one_join(fx: &Fixture) {
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n\
         :server 005 omairc CHATHISTORY=1 :are supported\r\n\
         :omairc!u@h JOIN :#omarchy\r\n",
    );
}

#[test]
fn late_batch_from_an_older_generation_does_not_finish_the_new_request() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    // The LATEST batch is open when a second self-JOIN bumps the generation and
    // BEFORE is sent under the new one.
    fx.inject(
        ":irc.host BATCH +late chathistory #omarchy\r\n\
         @batch=late;msgid=old :alice!u@h PRIVMSG #omarchy :older\r\n",
    );
    fx.inject(":omairc!u@h JOIN :#omarchy\r\n");
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "BEFORE under the new generation"
    );
    assert!(fx.session.history_inflight_for("#omarchy"));
    fx.inject(":irc.host BATCH -late\r\n");
    assert!(
        fx.session.history_inflight_for("#omarchy"),
        "the late batch settled the new request"
    );
    assert!(fx.batches().is_empty(), "late batch was delivered");
    assert!(
        !fx.session.request_older_history("#omarchy", "old", None),
        "the new BEFORE is still in flight"
    );
}

#[test]
fn generation_bump_clears_inflight_phase() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    assert!(fx.session.request_older_history("#omarchy", "old", None));
    assert!(fx.session.history_inflight_for("#omarchy"));
    fx.inject(":omairc!u@h JOIN :#omarchy\r\n");
    assert!(
        !fx.session.history_inflight_for("#omarchy"),
        "a membership change left the request in flight"
    );
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "BEFORE must be possible again after the bump"
    );
    assert_eq!(count_containing(&fx.frames(), "CHATHISTORY BEFORE"), 2);
}

#[test]
fn after_page_cap_sends_after_tail_and_a_full_tail_does_not_chain() {
    let fx = Fixture::new(history_config());
    fx.session.set_history_after_page_cap(2);
    resume_omarchy(&fx, resume_time());
    page_size_one_join(&fx);
    page(&fx, "p1", "m1");
    page(&fx, "p2", "m2");
    assert_eq!(count_containing(&fx.frames(), "CHATHISTORY AFTER "), 2);
    assert!(
        fx.wrote("CHATHISTORY LATEST #omarchy msgid=m2 1\r\n"),
        "frames {:?}",
        fx.frames()
    );
    let sent = fx.frames().len();
    // A full page closing the tail is the end of the chain, not a third page.
    page(&fx, "tail", "m3");
    assert_eq!(
        fx.frames().len(),
        sent,
        "the tail chained: {:?}",
        fx.written_since(sent)
    );
    assert!(!fx.session.history_inflight_for("#omarchy"));
}

#[test]
fn empty_after_tail_does_not_exhaust_before() {
    let fx = Fixture::new(history_config());
    fx.session.set_history_after_page_cap(2);
    resume_omarchy(&fx, resume_time());
    page_size_one_join(&fx);
    page(&fx, "p1", "m1");
    page(&fx, "p2", "m2");
    fx.inject(":irc.host BATCH +tail chathistory #omarchy\r\n:irc.host BATCH -tail\r\n");
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "an empty tail must not exhaust BEFORE"
    );
}

#[test]
fn stale_batch_after_part_is_discarded_and_part_forgets_the_target() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    fx.inject(
        ":irc.host BATCH +gone chathistory #omarchy\r\n\
         @batch=gone;msgid=old :alice!u@h PRIVMSG #omarchy :older\r\n",
    );
    fx.inject(":omairc!u@h PART #omarchy\r\n");
    fx.inject(":irc.host BATCH -gone\r\n");
    assert!(fx.batches().is_empty(), "stale batch was delivered");
    assert!(!fx.session.history_inflight_for("#omarchy"));
    assert_eq!(count_containing(&fx.frames(), "CHATHISTORY LATEST"), 1);
    fx.inject(":omairc!u@h JOIN :#omarchy\r\n");
    assert_eq!(
        count_containing(&fx.frames(), "CHATHISTORY LATEST"),
        2,
        "the target must be eligible again after PART"
    );
}

#[test]
fn targets_latch_is_independent_of_channel_phases() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    let from = at(2024, 3, 9, 15, 59, 59, 620);
    let until = at(2026, 10, 4, 12, 0, 10, 0);
    assert!(fx.session.request_history_targets(from, until));
    fx.inject(":omairc!u@h PART #omarchy\r\n:omairc!u@h JOIN :#omarchy\r\n");
    assert!(
        !fx.session.request_history_targets(from, until),
        "a membership change cleared the TARGETS latch"
    );
    fx.inject(
        ":irc.host BATCH +t draft/chathistory-targets\r\n\
         :irc.host BATCH -t\r\n",
    );
    assert!(
        fx.session.request_history_targets(from, until),
        "the TARGETS batch must release the latch"
    );
}

#[test]
fn after_on_an_exhausted_target_clears_exhaustion() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    assert!(fx.session.request_older_history("#omarchy", "old", None));
    fx.inject(":irc.host BATCH +empty chathistory #omarchy\r\n:irc.host BATCH -empty\r\n");
    assert!(
        !fx.session.request_older_history("#omarchy", "old", None),
        "an empty BEFORE exhausts the target"
    );
    assert!(fx.session.request_history_after("#omarchy", resume_time()));
    page(&fx, "gap", "m1");
    assert!(
        fx.session.request_older_history("#omarchy", "old", None),
        "AFTER on an exhausted target clears the exhaustion"
    );
}

/// The phase of `#omarchy`, read through the ledger.
fn omarchy_phase(fx: &Fixture) -> Option<HistoryPhase> {
    fx.session.locked(|core| {
        core.history
            .phase
            .get(&core.fold_channel("#omarchy"))
            .cloned()
    })
}

#[test]
fn a_request_in_flight_carries_only_its_own_payload() {
    // The type holds the AFTER pages and cursor on `After` alone, and has no
    // pending-without-kind or exhausted-while-in-flight value to write, so those
    // need no runtime test. This pins the transitions that rewrite the payload.
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    assert_eq!(omarchy_phase(&fx), Some(HistoryPhase::Settled));

    assert!(fx.session.request_history_after("#omarchy", resume_time()));
    assert_eq!(
        omarchy_phase(&fx),
        Some(HistoryPhase::InFlight {
            generation: 1,
            request: InFlightRequest::After {
                pages: 1,
                cursor: HistoryCursor {
                    msgid: String::new(),
                    when: Some(resume_time()),
                },
            },
        })
    );
    // A short page settles it and drops the pages and cursor with the phase.
    page(&fx, "one", "m1");
    assert_eq!(omarchy_phase(&fx), Some(HistoryPhase::Settled));
}

#[test]
fn exhausted_target_refuses_before_and_keeps_its_phase() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    assert!(fx.session.request_older_history("#omarchy", "old", None));
    fx.inject(":irc.host BATCH +empty chathistory #omarchy\r\n:irc.host BATCH -empty\r\n");
    assert_eq!(omarchy_phase(&fx), Some(HistoryPhase::Exhausted));
    assert!(!fx.session.request_older_history("#omarchy", "old", None));
    assert_eq!(omarchy_phase(&fx), Some(HistoryPhase::Exhausted));
    assert_eq!(count_containing(&fx.frames(), "CHATHISTORY BEFORE"), 1);
}

#[test]
fn batch_that_disagrees_on_kind_does_not_finish_the_request() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    complete_history_join(&fx);
    finish_latest_batch(&fx);
    assert!(fx.session.request_older_history("#omarchy", "old", None));
    // The BEFORE batch opens, then the request it answered is replaced at the
    // same generation, which the wire alone cannot express.
    fx.inject(
        ":irc.host BATCH +older chathistory #omarchy\r\n\
         @batch=older;msgid=page :bob!u@h PRIVMSG #omarchy :backlog\r\n",
    );
    let replacement = HistoryPhase::InFlight {
        generation: 1,
        request: InFlightRequest::Latest,
    };
    fx.session.locked(|core| {
        let folded = core.fold_channel("#omarchy");
        core.history.phase.insert(folded, replacement.clone());
    });
    fx.inject(":irc.host BATCH -older\r\n");
    assert_eq!(
        omarchy_phase(&fx),
        Some(replacement),
        "a BEFORE batch settled a LATEST request"
    );
    assert!(fx.session.history_inflight_for("#omarchy"));
}
