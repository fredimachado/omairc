//! Port of `tui/internal/session/batch_test.go`: the batch ledger, replay
//! batches, the open-batch budget, and the bouncer playback cases.

use super::fixture::{test_config, Fixture};
use crate::irc::message::IrcMessage;
use crate::session::events::{HistoryBatch, HistoryKind, SessionState};

/// The Go `historyConfig`: no autojoin, so the tests drive JOIN themselves.
pub fn history_config() -> crate::session::config::SessionConfig {
    let mut config = test_config();
    config.autojoin_channels.clear();
    config
}

/// Registers with `batch chathistory`, and self-joins `#omarchy` when asked.
pub fn register_with_history(fx: &Fixture, self_join: bool) {
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch chathistory\r\n\
         :server CAP omairc ACK :batch chathistory\r\n\
         :server 001 omairc :Welcome\r\n",
    );
    if self_join {
        fx.inject(":omairc!u@h JOIN :#omarchy\r\n");
    }
}

/// The trailing parameter of a PRIVMSG, in order.
pub fn bodies(messages: &[IrcMessage]) -> Vec<String> {
    messages
        .iter()
        .filter(|message| message.command == "PRIVMSG" && !message.params.is_empty())
        .map(|message| String::from_utf8_lossy(message.params.last().unwrap()).into_owned())
        .collect()
}

/// The messages received after the first `start` of them.
pub fn since(fx: &Fixture, start: usize) -> Vec<IrcMessage> {
    fx.messages().into_iter().skip(start).collect()
}

/// The text of a batch line's trailing parameter.
pub fn last_param(message: &IrcMessage) -> String {
    String::from_utf8_lossy(message.params.last().map(Vec::as_slice).unwrap_or_default())
        .into_owned()
}

/// A line of `n` unknown batches, `+d0..`, which fill the open-batch budget.
pub fn open_unknown_batches(count: usize) -> String {
    (0..count)
        .map(|index| format!(":irc.host BATCH +d{index} unknown.example/foo\r\n"))
        .collect()
}

#[test]
fn batch_open_and_close_do_not_emit_batch_lines() {
    let fx = Fixture::new(history_config());
    fx.register_with_welcome();
    let start = fx.messages().len();
    fx.inject(
        ":irc.host BATCH +ns netsplit irc.example irc.other\r\n\
         @batch=ns :alice!u@h PRIVMSG #omarchy :still here\r\n\
         :irc.host BATCH -ns\r\n",
    );
    let commands: Vec<String> = since(&fx, start)
        .iter()
        .map(|m| m.command.clone())
        .collect();
    assert_eq!(commands, vec!["PRIVMSG"]);
    assert_eq!(fx.session.state(), SessionState::Registered);
}

#[test]
fn nested_batches_do_not_fail_the_session() {
    let fx = Fixture::new(history_config());
    fx.register_with_welcome();
    let start = fx.messages().len();
    fx.inject(
        ":irc.host BATCH +outer netsplit irc.a irc.b\r\n\
         @batch=outer :irc.host BATCH +inner netjoin irc.a irc.b\r\n\
         @batch=inner :alice!u@h PRIVMSG #omarchy :Hi\r\n\
         @batch=outer :irc.host BATCH -inner\r\n\
         :irc.host BATCH -outer\r\n",
    );
    assert!(fx.errors().is_empty());
    assert_eq!(since(&fx, start).len(), 1);
    assert_eq!(fx.session.state(), SessionState::Registered);
}

#[test]
fn unknown_batch_type_and_close_stay_registered() {
    let fx = Fixture::new(history_config());
    fx.register_with_welcome();
    let start = fx.messages().len();
    fx.inject(
        ":irc.host BATCH +x unknown.example/foo\r\n\
         @batch=x :alice!u@h PRIVMSG #omarchy :delivered\r\n\
         :irc.host BATCH -x\r\n\
         :irc.host BATCH -missing\r\n\
         :bob!u@h PRIVMSG #omarchy :after\r\n",
    );
    assert!(fx.errors().is_empty());
    assert_eq!(bodies(&since(&fx, start)), vec!["delivered", "after"]);
    assert_eq!(fx.session.state(), SessionState::Registered);
}

#[test]
fn history_batch_swallows_inner_privmsg() {
    let fx = Fixture::new(history_config());
    register_with_history(&fx, true);
    let start = fx.messages().len();
    fx.inject(
        ":irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n\
         :irc.host BATCH -hx\r\n",
    );
    assert_eq!(
        fx.messages().len(),
        start,
        "the batch lines must be swallowed"
    );
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].target, "#omarchy");
    assert_eq!(batches[0].lines.len(), 1);
    assert_eq!(batches[0].lines[0].command, "PRIVMSG");
    assert_eq!(fx.session.state(), SessionState::Registered);

    fx.inject(":irc.host BATCH +empty chathistory #omarchy\r\n:irc.host BATCH -empty\r\n");
    let batches = fx.batches();
    assert_eq!(batches.len(), 2);
    assert!(batches[1].lines.is_empty());
    assert_eq!(batches[1].target, "#omarchy");
}

#[test]
fn draft_history_batch_swallows_inner_privmsg() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch draft/chathistory\r\n\
         :server CAP omairc ACK :batch draft/chathistory\r\n\
         :server 001 omairc :Welcome\r\n\
         :omairc!u@h JOIN :#omarchy\r\n",
    );
    let start = fx.messages().len();
    fx.inject(
        ":irc.host BATCH +hx draft/chathistory #omarchy\r\n\
         @batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n\
         :irc.host BATCH -hx\r\n",
    );
    assert_eq!(fx.messages().len(), start);
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].target, "#omarchy");
}

#[test]
fn unsolicited_history_flood_keeps_live_traffic() {
    let fx = Fixture::new(history_config());
    register_with_history(&fx, false);
    let start = fx.messages().len();
    let mut fill = open_unknown_batches(16);
    for index in 0..40 {
        fill.push_str(&format!(
            ":irc.host BATCH +x{index} chathistory #omarchy\r\n"
        ));
    }
    fill.push_str(
        ":irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=x0 :alice!u@h PRIVMSG #omarchy :recorded\r\n\
         @batch=x39 :alice!u@h PRIVMSG #omarchy :unrecorded\r\n\
         @batch=hx :alice!u@h PRIVMSG #omarchy :overflow\r\n\
         :irc.host BATCH -hx\r\n\
         :bob!u@h PRIVMSG #omarchy :after\r\n",
    );
    fx.inject(&fill);
    assert_eq!(
        bodies(&since(&fx, start)),
        vec!["unrecorded", "overflow", "after"]
    );
    assert!(fx.batches().is_empty());
}

#[test]
fn overflowed_unknown_batch_passes_inner_privmsg() {
    let fx = Fixture::new(history_config());
    register_with_history(&fx, false);
    let start = fx.messages().len();
    let mut fill = open_unknown_batches(16);
    fill.push_str(
        ":irc.host BATCH +extra unknown.example/foo\r\n\
         @batch=extra :alice!u@h PRIVMSG #omarchy :passed\r\n\
         :irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=hx :alice!u@h PRIVMSG #omarchy :overflow\r\n\
         :bob!u@h PRIVMSG #omarchy :after\r\n",
    );
    fx.inject(&fill);
    assert_eq!(bodies(&since(&fx, start)), vec!["passed", "after"]);
    assert!(fx.batches().is_empty());
}

#[test]
fn leftover_nested_batch_line_after_root_close_is_swallowed() {
    let fx = Fixture::new(history_config());
    register_with_history(&fx, true);
    let start = fx.messages().len();
    fx.inject(
        ":irc.host BATCH +stale chathistory #omarchy\r\n\
         @batch=stale :irc.host BATCH +child chathistory #omarchy\r\n\
         @batch=child :alice!u@h PRIVMSG #omarchy :old\r\n\
         :irc.host BATCH -stale\r\n\
         @batch=child :alice!u@h PRIVMSG #omarchy :late\r\n\
         :irc.host BATCH -child\r\n\
         :bob!u@h PRIVMSG #omarchy :after\r\n",
    );
    assert_eq!(bodies(&since(&fx, start)), vec!["after"]);
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].lines.len(), 1);
}

#[test]
fn ignored_batch_ledger_recovers_after_close() {
    let fx = Fixture::new(history_config());
    register_with_history(&fx, false);
    let start = fx.messages().len();
    let mut fill = open_unknown_batches(16);
    for index in 0..32 {
        fill.push_str(&format!(
            ":irc.host BATCH +x{index} chathistory #omarchy\r\n"
        ));
    }
    fill.push_str(
        ":irc.host BATCH -x0\r\n\
         :irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=hx :alice!u@h PRIVMSG #omarchy :overflow\r\n\
         :bob!u@h PRIVMSG #omarchy :after\r\n",
    );
    fx.inject(&fill);
    assert_eq!(bodies(&since(&fx, start)), vec!["after"]);
    assert!(fx.batches().is_empty());
}

#[test]
fn unsolicited_history_batch_still_emits() {
    let fx = Fixture::new(history_config());
    register_with_history(&fx, false);
    fx.inject(
        ":irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n\
         :irc.host BATCH -hx\r\n",
    );
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].target, "#omarchy");
    assert_eq!(batches[0].lines.len(), 1);
    assert_eq!(last_param(&batches[0].lines[0]), "older");
}

#[test]
fn history_batch_without_capability_is_ignored() {
    let fx = Fixture::new(history_config());
    fx.register_with_welcome();
    let start = fx.messages().len();
    fx.inject(
        ":irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n\
         :irc.host BATCH -hx\r\n\
         :bob!u@h PRIVMSG #omarchy :after\r\n",
    );
    assert!(fx.batches().is_empty());
    assert_eq!(bodies(&since(&fx, start)), vec!["after"]);
}

#[test]
fn bouncer_playback_batch_needs_only_the_batch_capability() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch\r\n\
         :server CAP omairc ACK :batch\r\n\
         :server 001 omairc :Welcome\r\n\
         :omairc!u@h JOIN :#omarchy\r\n",
    );
    assert!(fx
        .session
        .capabilities()
        .contains(crate::irc::capability::IrcCapability::Batch));
    assert!(!fx
        .session
        .capabilities()
        .contains(crate::irc::capability::IrcCapability::ChatHistory));
    let start = fx.messages().len();
    fx.inject(
        ":znc.in BATCH +pb znc.in/playback #omarchy\r\n\
         @batch=pb :lena!u@h PRIVMSG #omarchy :yesterday\r\n\
         :znc.in BATCH -pb\r\n\
         :bob!u@h PRIVMSG #omarchy :after\r\n",
    );
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].target, "#omarchy");
    assert_eq!(batches[0].lines.len(), 1);
    let line = &batches[0].lines[0];
    assert_eq!(line.command, "PRIVMSG");
    assert_eq!(last_param(line), "yesterday");
    assert_eq!(bodies(&since(&fx, start)), vec!["after"]);
}

#[test]
fn bouncer_query_playback_batch_collects_under_the_peer_nick() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch echo-message\r\n\
         :server CAP omairc ACK :batch echo-message\r\n\
         :server 001 omairc :Welcome\r\n",
    );
    fx.inject(
        ":znc.in BATCH +q znc.in/playback lena\r\n\
         @batch=q :lena!u@h PRIVMSG omairc :are you there\r\n\
         @batch=q :omairc!u@h PRIVMSG lena :just got back\r\n\
         :znc.in BATCH -q\r\n",
    );
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].target, "lena");
    assert_eq!(batches[0].lines.len(), 2);
    assert_eq!(last_param(&batches[0].lines[0]), "are you there");
    assert_eq!(last_param(&batches[0].lines[1]), "just got back");
}

#[test]
fn bouncer_batch_playback_type_is_not_replay() {
    let fx = Fixture::new(history_config());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch\r\n\
         :server CAP omairc ACK :batch\r\n\
         :server 001 omairc :Welcome\r\n",
    );
    let start = fx.messages().len();
    fx.inject(
        ":znc.in BATCH +q znc.in/batch/playback lena\r\n\
         @batch=q :lena!u@h PRIVMSG omairc :are you there\r\n\
         :znc.in BATCH -q\r\n",
    );
    assert!(fx.batches().is_empty());
    assert_eq!(bodies(&since(&fx, start)), vec!["are you there"]);
}

#[test]
fn unsolicited_bouncer_playback_shares_the_open_batch_budget() {
    let fx = Fixture::new(history_config());
    register_with_history(&fx, true);
    assert!(
        fx.session.history_pending(),
        "history must be pending after the self join"
    );
    let mut fill = open_unknown_batches(16);
    fill.push_str(
        ":znc.in BATCH +pb znc.in/playback #omarchy\r\n\
         @batch=pb :lena!u@h PRIVMSG #omarchy :crowded out\r\n\
         :znc.in BATCH -pb\r\n\
         :irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=hx :alice!u@h PRIVMSG #omarchy :asked for\r\n\
         :irc.host BATCH -hx\r\n",
    );
    fx.inject(&fill);
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].target, "#omarchy");
    assert_eq!(batches[0].lines.len(), 1);
    assert_eq!(last_param(&batches[0].lines[0]), "asked for");
    assert!(
        !fx.session.history_pending(),
        "history must no longer be pending"
    );
}

#[test]
fn solicited_history_batch_survives_open_batch_flood() {
    let fx = Fixture::new(history_config());
    register_with_history(&fx, true);
    let mut fill = open_unknown_batches(16);
    fill.push_str(
        ":irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n\
         :irc.host BATCH -hx\r\n",
    );
    fx.inject(&fill);
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].target, "#omarchy");
    assert_eq!(batches[0].lines.len(), 1);
    assert!(!fx.session.history_pending());
}

#[test]
fn open_playback_does_not_make_pending_history_unsolicited() {
    let fx = Fixture::new(history_config());
    register_with_history(&fx, true);
    let mut fill = String::from(
        ":znc.in BATCH +pb znc.in/playback #omarchy\r\n\
         @batch=pb :lena!u@h PRIVMSG #omarchy :from the bouncer\r\n",
    );
    fill.push_str(&open_unknown_batches(16));
    fill.push_str(
        ":irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n\
         :irc.host BATCH -hx\r\n\
         :znc.in BATCH -pb\r\n",
    );
    fx.inject(&fill);
    let batches: Vec<HistoryBatch> = fx.batches();
    assert_eq!(batches.len(), 2, "{batches:?}");
    let first = &batches[0];
    assert_eq!(first.target, "#omarchy");
    assert_eq!(first.lines.len(), 1);
    assert_eq!(last_param(&first.lines[0]), "older");
    let second = &batches[1];
    assert_eq!(second.target, "#omarchy");
    assert_eq!(second.lines.len(), 1);
    assert_eq!(last_param(&second.lines[0]), "from the bouncer");
    assert_eq!(second.kind, Some(HistoryKind::BouncerPlayback));
    assert!(!fx.session.history_pending());
}
