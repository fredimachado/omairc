//! `tui/internal/session/labeledresponse_test.go`, session-level half.
//! The Go tests read the request label off each Status entry and assert the
//! routed text ("No such nick: lena"). That routing is the Phase 3 Status
//! recorder's job, so those text assertions are left out. What stays here: the
//! label on each written frame, the pending-label count and membership, the
//! `RequestLabelFinished` events, the batch that closes a label, the timeouts
//! and the open-batch budget.

use std::time::Duration;

use super::fixture::{test_config, Fixture};
use crate::irc::capability::IrcCapability;

/// `labeledConfig`: no reconnect, so a disconnect is terminal, and no autojoin.
fn labeled_config() -> crate::session::config::SessionConfig {
    let mut config = test_config();
    config.reconnect_enabled = false;
    config.autojoin_channels.clear();
    config
}

fn new_labeled() -> Fixture {
    Fixture::new(labeled_config())
}

/// Starts the session and negotiates `message-tags labeled-response`.
fn register_labeled(fx: &Fixture) {
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :message-tags labeled-response\r\n\
         :server CAP omairc ACK :message-tags labeled-response\r\n\
         :server 001 omairc :Welcome\r\n",
    );
}

/// Starts the session without `labeled-response`.
fn register_unlabeled(fx: &Fixture) {
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :multi-prefix\r\n:server 001 omairc :Welcome\r\n");
}

/// The label of a `@label=<x> <command>` frame, or empty.
fn request_label_of(frame: &str) -> String {
    let Some(rest) = frame.strip_prefix("@label=") else {
        return String::new();
    };
    match rest.find(' ') {
        Some(space) => rest[..space].to_owned(),
        None => String::new(),
    }
}

/// The command part of a frame, with any `@tags` removed.
fn command_of(frame: &str) -> String {
    if !frame.starts_with('@') {
        return frame.to_owned();
    }
    match frame.find(' ') {
        Some(space) => frame[space + 1..].to_owned(),
        None => frame.to_owned(),
    }
}

fn finished(fx: &Fixture) -> Vec<String> {
    fx.finished_labels()
}

#[test]
fn labeled_requests_labeled_response_on_own_line() {
    let fx = new_labeled();
    fx.session.start();
    fx.transport.complete_connect();
    fx.inject(":server CAP omairc LS :message-tags labeled-response away-notify\r\n");
    assert!(
        fx.wrote("CAP REQ :message-tags\r\n")
            && fx.wrote("CAP REQ :labeled-response\r\n")
            && fx.wrote("CAP REQ :away-notify\r\n"),
        "frames = {:?}",
        fx.frames()
    );
}

#[test]
fn labeled_does_not_request_labeled_response_without_message_tags() {
    let fx = new_labeled();
    fx.session.start();
    fx.transport.complete_connect();
    fx.inject(":server CAP omairc LS :labeled-response away-notify\r\n");
    assert!(
        !fx.wrote("CAP REQ :labeled-response\r\n"),
        "labeled-response needs message-tags"
    );
    assert!(
        fx.wrote("CAP REQ :away-notify\r\n"),
        "away-notify must still be requested"
    );
}

#[test]
fn labeled_whois_stays_untagged_without_cap() {
    let fx = new_labeled();
    register_unlabeled(&fx);
    assert!(!fx
        .session
        .capabilities()
        .contains(IrcCapability::LabeledResponse));
    assert_eq!(fx.session.start_labeled_request(), "");
    assert!(fx.session.whois("lena", ""));
    assert_eq!(fx.last_frame(), "WHOIS lena lena\r\n");
    assert_eq!(fx.session.pending_request_label_count(), 0);
}

#[test]
fn labeled_whois_and_ctcp_get_unique_labels() {
    let fx = new_labeled();
    register_labeled(&fx);
    let enabled = fx.session.capabilities();
    assert!(
        enabled.contains(IrcCapability::LabeledResponse)
            && enabled.contains(IrcCapability::MessageTags)
    );

    let first = fx.session.start_labeled_request();
    assert!(!first.is_empty());
    assert!(fx.session.whois("lena", &first));
    assert_eq!(request_label_of(&fx.last_frame()), first);
    assert_eq!(command_of(&fx.last_frame()), "WHOIS lena lena\r\n");

    let second = fx.session.start_labeled_request();
    assert!(!second.is_empty() && second != first);
    assert!(fx.session.whois("mira", &second));
    assert_eq!(request_label_of(&fx.last_frame()), second);
    assert_eq!(fx.session.pending_request_label_count(), 2);

    let ctcp = fx.session.start_labeled_request();
    assert!(!ctcp.is_empty() && ctcp != first && ctcp != second);
    assert!(fx.session.send_ctcp("lena", "VERSION", "", &ctcp));
    assert_eq!(request_label_of(&fx.last_frame()), ctcp);
    assert!(command_of(&fx.last_frame()).starts_with("PRIVMSG lena :"));
    assert_eq!(fx.session.pending_request_label_count(), 3);
}

#[test]
fn labeled_single_numeric_finishes_its_label() {
    let fx = new_labeled();
    register_labeled(&fx);
    let label = fx.session.start_labeled_request();
    assert!(fx.session.whois("lena", &label));
    fx.inject(&format!(
        "@label={label} :irc 401 omairc lena :No such nick/channel\r\n"
    ));
    assert!(
        !fx.session.has_pending_request_label(&label),
        "the labeled numeric must finish the label"
    );
    assert_eq!(finished(&fx), vec![label]);
}

#[test]
fn labeled_response_batch_routes_inner_lines_and_finishes() {
    let fx = new_labeled();
    register_labeled(&fx);
    let label = fx.session.start_labeled_request();
    assert!(fx.session.whois("lena", &label));
    fx.inject(&format!(
        "@label={label} :irc.example BATCH +NMzYSq45x labeled-response\r\n\
         @batch=NMzYSq45x :irc 311 omairc lena ~lena user/host * :Lena\r\n\
         @batch=NMzYSq45x :irc 319 omairc lena :#omarchy\r\n\
         @batch=NMzYSq45x :irc 318 omairc lena :End of /WHOIS list.\r\n\
         :irc.example BATCH -NMzYSq45x\r\n"
    ));
    assert!(
        !fx.session.has_pending_request_label(&label),
        "closing the labeled batch must finish the label"
    );
    assert_eq!(finished(&fx), vec![label]);
}

#[test]
fn labeled_unlabeled_leftover_does_not_steal_labeled_waiter() {
    let fx = new_labeled();
    register_labeled(&fx);
    let label = fx.session.start_labeled_request();
    assert!(fx.session.whois("lena", &label));
    fx.inject(":irc 311 omairc lena ~other host * :Other\r\n:irc 318 omairc lena :End of /WHOIS list.\r\n");
    assert!(
        fx.session.has_pending_request_label(&label),
        "the label must stay pending"
    );
    assert!(finished(&fx).is_empty());

    fx.inject(&format!(
        "@label={label} :irc.example BATCH +whois1 labeled-response\r\n\
         @batch=whois1 :irc 311 omairc lena ~lena user/host * :Lena\r\n\
         @batch=whois1 :irc 318 omairc lena :End of /WHOIS list.\r\n\
         :irc.example BATCH -whois1\r\n"
    ));
    assert_eq!(finished(&fx), vec![label]);
}

#[test]
fn labeled_unsolicited_label_does_not_crash_or_steal() {
    let fx = new_labeled();
    register_labeled(&fx);
    let label = fx.session.start_labeled_request();
    assert!(fx.session.whois("lena", &label));
    fx.inject(
        "@label=nope :irc 311 omairc lena ~x h * :Nope\r\n\
         @label=nope :irc 318 omairc lena :End of /WHOIS list.\r\n\
         @label=nope :irc.example ACK\r\n",
    );
    assert!(
        fx.session.has_pending_request_label(&label),
        "the pending label must survive an unsolicited label"
    );
    fx.inject(&format!(
        "@label={label} :irc 401 omairc lena :No such nick/channel\r\n"
    ));
    assert!(!fx.session.has_pending_request_label(&label));
}

#[test]
fn labeled_ack_completes_waiter_without_transcript() {
    let fx = new_labeled();
    register_labeled(&fx);
    let label = fx.session.start_labeled_request();
    assert!(fx.session.whois("lena", &label));
    assert_eq!(fx.session.pending_request_label_count(), 1);
    fx.inject(&format!("@label={label} :irc.example ACK\r\n"));
    assert_eq!(fx.session.pending_request_label_count(), 0);
    assert_eq!(finished(&fx), vec![label]);

    let before = fx.messages().len();
    fx.inject(":irc 311 omairc lena ~lena user/host * :Lena\r\n:irc 318 omairc lena :End of /WHOIS list.\r\n");
    let messages = fx.messages();
    assert!(
        messages.len() > before,
        "the unlabeled lines still reach the translator"
    );
}

#[test]
fn labeled_fail_finishes_the_label() {
    let fx = new_labeled();
    register_labeled(&fx);
    let label = fx.session.start_labeled_request();
    assert!(fx.session.whois("lena", &label));
    fx.inject(&format!(
        "@label={label} :irc FAIL WHOIS TEMPORARILY_UNAVAILABLE lena :try later\r\n"
    ));
    assert!(!fx.session.has_pending_request_label(&label));
    assert_eq!(finished(&fx), vec![label]);
}

#[test]
fn labeled_ctcp_reply_finishes_the_label() {
    let fx = new_labeled();
    register_labeled(&fx);
    let label = fx.session.start_labeled_request();
    assert!(fx.session.send_ctcp("lena", "VERSION", "", &label));
    assert!(command_of(&fx.last_frame()).starts_with("PRIVMSG lena :"));
    fx.inject(&format!(
        "@label={label} :lena!u@h NOTICE omairc :\u{1}VERSION Omairc 0.4.0\u{1}\r\n"
    ));
    assert!(!fx.session.has_pending_request_label(&label));
}

#[test]
fn labeled_unlabeled_ctcp_reply_after_ack_copies_into_asking_transcript() {
    let fx = new_labeled();
    register_labeled(&fx);
    let label = fx.session.start_labeled_request();
    assert!(fx.session.send_ctcp("lena", "VERSION", "", &label));
    assert_eq!(fx.session.pending_request_label_count(), 1);
    fx.inject(&format!("@label={label} :irc.example ACK\r\n"));
    assert_eq!(fx.session.pending_request_label_count(), 0);
    fx.inject(":lena!u@h NOTICE omairc :\u{1}VERSION Omairc 0.4.0\u{1}\r\n");
    assert!(
        fx.messages().iter().any(|m| m.command == "NOTICE"),
        "the unlabeled CTCP reply still reaches the translator"
    );
}

#[test]
fn labeled_unlabeled_ctcp_reply_after_echo_message_copies_into_asking_transcript() {
    let fx = new_labeled();
    register_labeled(&fx);
    let label = fx.session.start_labeled_request();
    assert!(fx.session.send_ctcp("lena", "VERSION", "", &label));
    assert!(command_of(&fx.last_frame()).starts_with("PRIVMSG lena :"));
    fx.inject(&format!(
        "@label={label} :omairc!u@h PRIVMSG lena :\u{1}VERSION\u{1}\r\n"
    ));
    assert!(
        !fx.session.has_pending_request_label(&label),
        "the echo carries the label, so it must finish"
    );
    fx.inject(":lena!u@h NOTICE omairc :\u{1}VERSION Omairc 0.4.0\u{1}\r\n");
    assert!(fx.messages().iter().any(|m| m.command == "NOTICE"));
}

#[test]
fn labeled_ctcp_401_finishes_the_label() {
    let fx = new_labeled();
    register_labeled(&fx);
    let label = fx.session.start_labeled_request();
    assert!(fx.session.send_ctcp("missingnick", "VERSION", "", &label));
    assert!(command_of(&fx.last_frame()).starts_with("PRIVMSG missingnick :"));
    assert_eq!(fx.session.pending_request_label_count(), 1);
    fx.inject(&format!(
        "@label={label} :irc 401 omairc missingnick :No such nick/channel\r\n"
    ));
    assert_eq!(fx.session.pending_request_label_count(), 0);

    let before = fx.messages().len();
    fx.inject(":irc 401 omairc missingnick :No such nick/channel\r\n");
    assert_eq!(
        fx.messages().len(),
        before + 1,
        "a later unlabeled 401 still reaches the translator"
    );
    assert_eq!(fx.session.pending_request_label_count(), 0);
}

#[test]
fn labeled_unsolicited_batches_share_the_open_batch_budget() {
    let fx = new_labeled();
    register_labeled(&fx);
    assert_eq!(fx.session.open_batch_count(), 0);
    let mut fill = String::new();
    for index in 0..16 {
        fill.push_str(&format!(
            ":irc.example BATCH +d{index} unknown.example/foo\r\n"
        ));
    }
    fx.inject(&fill);
    assert_eq!(fx.session.open_batch_count(), 16);

    let mut flood = String::new();
    for index in 0..40 {
        let id = format!("lr{index}");
        flood.push_str(&format!(
            ":irc.example BATCH +{id} labeled-response\r\n\
             @batch={id} :irc 311 omairc lena ~x h * :Nope\r\n\
             @batch={id} :irc 318 omairc lena :End of /WHOIS list.\r\n"
        ));
    }
    fx.inject(&flood);
    assert_eq!(fx.session.open_batch_count(), 16);

    let label = fx.session.start_labeled_request();
    assert!(fx.session.whois("lena", &label));
    let label = request_label_of(&fx.last_frame());
    assert!(!label.is_empty() && fx.session.has_pending_request_label(&label));
    fx.inject(&format!(
        "@label={label} :irc.example BATCH +whois1 labeled-response\r\n\
         @batch=whois1 :irc 311 omairc lena ~lena user/host * :Lena\r\n\
         @batch=whois1 :irc 318 omairc lena :End of /WHOIS list.\r\n\
         :irc.example BATCH -whois1\r\n"
    ));
    assert_eq!(finished(&fx), vec![label]);
    assert_eq!(fx.session.open_batch_count(), 16);
}

#[test]
fn labeled_timeout_clears_waiters() {
    let fx = new_labeled();
    register_labeled(&fx);
    let label = fx.session.start_labeled_request();
    assert!(!label.is_empty() && fx.session.whois("lena", &label));
    assert_eq!(fx.session.pending_request_label_count(), 1);
    assert!(fx.session.has_pending_request_label(&label));
    fx.clock.advance(Duration::from_millis(45000));
    assert_eq!(fx.session.pending_request_label_count(), 0);
    assert!(!fx.session.has_pending_request_label(&label));
    assert_eq!(finished(&fx), vec![label]);
}

#[test]
fn labeled_timeout_expires_only_elapsed_labels() {
    let fx = new_labeled();
    register_labeled(&fx);
    let first = fx.session.start_labeled_request();
    assert!(!first.is_empty() && fx.session.whois("lena", &first));
    assert!(fx.clock.pending() > 0, "the label timer must be pending");

    fx.clock.advance(Duration::from_millis(10000));
    let second = fx.session.start_labeled_request();
    assert!(!second.is_empty() && second != first);
    assert!(fx.session.whois("mira", &second));
    assert_eq!(fx.session.pending_request_label_count(), 2);

    fx.clock.advance(Duration::from_millis(35000));
    assert!(
        !fx.session.has_pending_request_label(&first),
        "the first label must have expired"
    );
    assert!(
        fx.session.has_pending_request_label(&second),
        "the second label must survive"
    );
    assert_eq!(fx.session.pending_request_label_count(), 1);
    assert_eq!(finished(&fx), vec![first.clone()]);

    fx.clock.advance(Duration::from_millis(10000));
    assert_eq!(fx.session.pending_request_label_count(), 0);
    assert!(!fx.session.has_pending_request_label(&second));
    assert_eq!(finished(&fx), vec![first, second]);
}

#[test]
fn labeled_timeout_drops_elapsed_watch_without_stealing_newer() {
    let fx = new_labeled();
    register_labeled(&fx);
    let first = fx.session.start_labeled_request();
    assert!(!first.is_empty() && fx.session.whois("lena", &first));
    fx.clock.advance(Duration::from_millis(10000));
    let second = fx.session.start_labeled_request();
    assert!(!second.is_empty() && second != first && fx.session.whois("mira", &second));
    fx.clock.advance(Duration::from_millis(35000));
    assert!(!fx.session.has_pending_request_label(&first));
    assert!(fx.session.has_pending_request_label(&second));

    // A late answer for the expired watch must not resurrect it or steal the newer one.
    fx.inject(&format!(
        "@label={first} :irc 318 omairc lena :End of /WHOIS list.\r\n"
    ));
    assert!(!fx.session.has_pending_request_label(&first));
    assert!(fx.session.has_pending_request_label(&second));

    fx.inject(&format!(
        "@label={second} :irc.example BATCH +b labeled-response\r\n\
         @batch=b :irc 311 omairc mira ~m h * :FromHelp\r\n\
         @batch=b :irc 318 omairc mira :End of /WHOIS list.\r\n\
         :irc.example BATCH -b\r\n"
    ));
    assert!(
        !fx.session.has_pending_request_label(&second),
        "the second label must be finished"
    );
}

#[test]
fn labeled_disconnect_clears_waiters() {
    let fx = new_labeled();
    register_labeled(&fx);
    let label = fx.session.start_labeled_request();
    assert!(!label.is_empty() && fx.session.whois("lena", &label));
    assert_eq!(fx.session.pending_request_label_count(), 1);
    fx.session.stop();
    assert_eq!(fx.session.pending_request_label_count(), 0);
    assert_eq!(finished(&fx), vec![label]);
}

#[test]
fn labeled_two_whois_for_same_nick_stay_independent() {
    let fx = new_labeled();
    register_labeled(&fx);
    let first = fx.session.start_labeled_request();
    assert!(fx.session.whois("mira", &first));
    let second = fx.session.start_labeled_request();
    assert!(second != first && fx.session.whois("mira", &second));
    fx.inject(&format!(
        "@label={first} :irc.example BATCH +a labeled-response\r\n\
         @batch=a :irc 311 omairc mira ~m h * :FromOmarchy\r\n\
         @batch=a :irc 318 omairc mira :End of /WHOIS list.\r\n\
         :irc.example BATCH -a\r\n"
    ));
    assert!(
        fx.session.has_pending_request_label(&second),
        "the second label must stay pending"
    );
    assert_eq!(finished(&fx), vec![first]);
}
