//! `draft/multiline` receive (#478): a batch of lines is one message, live and
//! nested in a CHATHISTORY batch, with a ceiling on what one batch buffers and a
//! flush when the connection ends first. Go twins live in `tui/internal/session`.

use super::batch::{bodies, history_config, open_unknown_batches, since};
use super::fixture::Fixture;
use crate::irc::message::IrcMessage;
use crate::session::events::SessionState;

const CAPS: &str = "batch message-tags draft/multiline chathistory";

/// Registers with the given CAP list (advertised and granted as is), then
/// self-joins `#omarchy`.
fn register_with(fx: &Fixture, caps: &str) {
    fx.connect_tls();
    fx.inject(&format!(
        ":server CAP omairc LS :{caps}\r\n\
         :server CAP omairc ACK :{caps}\r\n\
         :server 001 omairc :Welcome\r\n\
         :omairc!u@h JOIN :#omarchy\r\n"
    ));
}

fn register(fx: &Fixture) {
    register_with(fx, CAPS);
}

/// One closed multiline batch of `lines`, each a `PRIVMSG` from mira.
fn batch(reference: &str, lines: &[&str]) -> String {
    let mut text = format!(":mira!u@h BATCH +{reference} draft/multiline #omarchy\r\n");
    for line in lines {
        text.push_str(&format!(
            "@batch={reference} :mira!u@h PRIVMSG #omarchy :{line}\r\n"
        ));
    }
    text.push_str(&format!(":mira!u@h BATCH -{reference}\r\n"));
    text
}

fn body(message: &IrcMessage) -> String {
    message.text_param(1)
}

#[test]
fn a_three_line_batch_is_one_message() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    fx.inject(&batch("ml", &["one", "two", "three"]));
    let messages = since(&fx, start);
    assert_eq!(messages.len(), 1, "one message, no per-line events");
    assert_eq!(body(&messages[0]), "one\ntwo\nthree");
    assert_eq!(messages[0].command, "PRIVMSG");
    assert_eq!(messages[0].text_param(0), "#omarchy");
    assert!(fx.batches().is_empty());
    assert_eq!(fx.session.state(), SessionState::Registered);
}

#[test]
fn lines_inside_the_batch_are_not_delivered_before_it_closes() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    fx.inject(
        ":mira!u@h BATCH +ml draft/multiline #omarchy\r\n\
         @batch=ml :mira!u@h PRIVMSG #omarchy :one\r\n",
    );
    assert!(since(&fx, start).is_empty(), "held until BATCH -ml");
    fx.inject(":mira!u@h BATCH -ml\r\n");
    assert_eq!(bodies(&since(&fx, start)), vec!["one"]);
}

#[test]
fn a_concat_line_joins_with_no_separator() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    fx.inject(
        ":mira!u@h BATCH +ml draft/multiline #omarchy\r\n\
         @batch=ml :mira!u@h PRIVMSG #omarchy :first half, \r\n\
         @batch=ml;draft/multiline-concat :mira!u@h PRIVMSG #omarchy :second half\r\n\
         @batch=ml :mira!u@h PRIVMSG #omarchy :next line\r\n\
         :mira!u@h BATCH -ml\r\n",
    );
    assert_eq!(
        bodies(&since(&fx, start)),
        vec!["first half, second half\nnext line"]
    );
}

#[test]
fn the_batch_line_supplies_msgid_time_and_account() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    fx.inject(
        "@msgid=open1;time=2026-01-02T03:04:05.000Z;account=mira :mira!u@h BATCH +ml draft/multiline #omarchy\r\n\
         @batch=ml;msgid=line1;time=1999-01-01T00:00:00.000Z :mira!u@h PRIVMSG #omarchy :one\r\n\
         @batch=ml;msgid=line2 :mira!u@h PRIVMSG #omarchy :two\r\n\
         :mira!u@h BATCH -ml\r\n",
    );
    let messages = since(&fx, start);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].msgid(), "open1");
    assert_eq!(
        messages[0].tag_text("time").as_deref(),
        Some("2026-01-02T03:04:05.000Z")
    );
    assert_eq!(messages[0].tag_text("account").as_deref(), Some("mira"));
    assert!(!messages[0].has_tag("batch"));
}

#[test]
fn a_notice_batch_is_one_notice() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    fx.inject(
        ":mira!u@h BATCH +ml draft/multiline #omarchy\r\n\
         @batch=ml :mira!u@h NOTICE #omarchy :one\r\n\
         @batch=ml :mira!u@h NOTICE #omarchy :two\r\n\
         :mira!u@h BATCH -ml\r\n",
    );
    let messages = since(&fx, start);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].command, "NOTICE");
    assert_eq!(body(&messages[0]), "one\ntwo");
}

#[test]
fn a_ctcp_batch_falls_back_to_its_lines() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    fx.inject(
        ":mira!u@h BATCH +ml draft/multiline #omarchy\r\n\
         @batch=ml :mira!u@h PRIVMSG #omarchy :one\r\n\
         @batch=ml :mira!u@h PRIVMSG #omarchy :\u{1}ACTION waves\u{1}\r\n\
         :mira!u@h BATCH -ml\r\n",
    );
    let messages = since(&fx, start);
    assert_eq!(messages.len(), 2, "delivered one by one, as without multiline");
    assert!(!messages[0].has_tag("batch"));
}

#[test]
fn two_batches_interleave_without_mixing() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    fx.inject(
        ":mira!u@h BATCH +a draft/multiline #omarchy\r\n\
         :bob!u@h BATCH +b draft/multiline #omarchy\r\n\
         @batch=a :mira!u@h PRIVMSG #omarchy :a1\r\n\
         @batch=b :bob!u@h PRIVMSG #omarchy :b1\r\n\
         @batch=a :mira!u@h PRIVMSG #omarchy :a2\r\n\
         @batch=b :bob!u@h PRIVMSG #omarchy :b2\r\n\
         :bob!u@h BATCH -b\r\n\
         :mira!u@h BATCH -a\r\n",
    );
    assert_eq!(bodies(&since(&fx, start)), vec!["b1\nb2", "a1\na2"]);
}

// Nested in CHATHISTORY.

fn history_with(inner: &str) -> String {
    format!(
        ":irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=hx;msgid=m0 :alice!u@h PRIVMSG #omarchy :before\r\n\
         {inner}\
         @batch=hx;msgid=m2 :alice!u@h PRIVMSG #omarchy :after\r\n\
         :irc.host BATCH -hx\r\n"
    )
}

/// A multiline batch whose lines and BATCH frames name `hx` as their parent.
fn nested_batch(lines: &[&str]) -> String {
    let mut text =
        "@batch=hx;msgid=ml1;time=2026-01-02T03:04:05.000Z :mira!u@h BATCH +ml draft/multiline #omarchy\r\n"
            .to_owned();
    for line in lines {
        text.push_str(&format!(
            "@batch=ml :mira!u@h PRIVMSG #omarchy :{line}\r\n"
        ));
    }
    text.push_str("@batch=hx :mira!u@h BATCH -ml\r\n");
    text
}

#[test]
fn a_multiline_nested_in_chathistory_is_one_line_of_the_replay() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    fx.inject(&history_with(&nested_batch(&["one", "two", "three"])));
    assert_eq!(since(&fx, start).len(), 0, "nothing leaks as live traffic");
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    let lines = &batches[0].lines;
    assert_eq!(lines.len(), 3, "before, the assembled message, after");
    assert_eq!(body(&lines[0]), "before");
    assert_eq!(body(&lines[1]), "one\ntwo\nthree");
    assert_eq!(lines[1].msgid(), "ml1");
    assert!(!lines[1].has_tag("batch"));
    assert_eq!(body(&lines[2]), "after");
}

#[test]
fn a_multiline_nested_in_chathistory_without_the_capability_is_collected_line_by_line() {
    let fx = Fixture::new(history_config());
    register_with(&fx, "batch message-tags chathistory");
    fx.inject(&history_with(&nested_batch(&["one", "two"])));
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    let collected: Vec<String> = batches[0].lines.iter().map(body).collect();
    assert_eq!(collected, vec!["before", "one", "two", "after"]);
}

#[test]
fn the_replay_ceiling_holds_for_assembled_messages() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let mut text = ":irc.host BATCH +hx chathistory #omarchy\r\n".to_owned();
    for index in 0..257 {
        text.push_str(&format!(
            "@batch=hx :mira!u@h BATCH +m{index} draft/multiline #omarchy\r\n\
             @batch=m{index} :mira!u@h PRIVMSG #omarchy :a{index}\r\n\
             @batch=m{index} :mira!u@h PRIVMSG #omarchy :b{index}\r\n\
             @batch=hx :mira!u@h BATCH -m{index}\r\n"
        ));
    }
    text.push_str(":irc.host BATCH -hx\r\n");
    fx.inject(&text);
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].lines.len(), 256, "the newest 256 are kept");
    assert_eq!(body(&batches[0].lines[0]), "a1\nb1");
    assert_eq!(body(&batches[0].lines[255]), "a256\nb256");
}

#[test]
fn a_stale_replay_drops_its_assembled_message_with_the_rest() {
    let fx = Fixture::new(history_config());
    register(&fx);
    fx.inject(
        ":irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=hx :alice!u@h PRIVMSG #omarchy :before\r\n",
    );
    fx.inject(&nested_batch(&["one", "two"]));
    // A second self-JOIN bumps the generation while the replay is still open.
    fx.inject(":omairc!u@h JOIN :#omarchy\r\n:irc.host BATCH -hx\r\n");
    assert!(fx.batches().is_empty(), "a stale replay is dropped whole");
}

// A batch that never closes.

fn open_unclosed(fx: &Fixture) {
    fx.inject(
        ":mira!u@h BATCH +ml draft/multiline #omarchy\r\n\
         @batch=ml :mira!u@h PRIVMSG #omarchy :one\r\n\
         @batch=ml :mira!u@h PRIVMSG #omarchy :two\r\n",
    );
}

#[test]
fn an_unclosed_batch_is_flushed_when_the_connection_drops_and_not_again_on_reconnect() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    open_unclosed(&fx);
    assert!(since(&fx, start).is_empty());

    fx.transport.remote_close();
    assert_eq!(bodies(&since(&fx, start)), vec!["one\ntwo"]);

    fx.fire_reconnect();
    fx.transport.complete_connect();
    assert_eq!(
        bodies(&since(&fx, start)),
        vec!["one\ntwo"],
        "the reset has nothing left to flush"
    );
}

#[test]
fn an_unclosed_batch_is_flushed_when_the_connection_state_resets() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    open_unclosed(&fx);
    fx.session.locked(|core| core.reset_for_connection());
    assert_eq!(bodies(&since(&fx, start)), vec!["one\ntwo"]);
    fx.session.locked(|core| core.reset_for_connection());
    assert_eq!(since(&fx, start).len(), 1, "flushing twice emits once");
}

#[test]
fn a_manual_stop_still_shows_the_unclosed_message() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    open_unclosed(&fx);
    fx.session.stop();
    fx.transport.remote_close();
    assert_eq!(bodies(&since(&fx, start)), vec!["one\ntwo"]);
}

#[test]
fn an_unclosed_batch_nested_in_an_unclosed_replay_is_dropped_with_it() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    fx.inject(
        ":irc.host BATCH +hx chathistory #omarchy\r\n\
         @batch=hx :mira!u@h BATCH +ml draft/multiline #omarchy\r\n\
         @batch=ml :mira!u@h PRIVMSG #omarchy :one\r\n",
    );
    fx.transport.remote_close();
    assert!(since(&fx, start).is_empty(), "history is never replayed live");
    assert!(fx.batches().is_empty());
}

// Caps on what one batch buffers.

fn numbered(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("l{index}")).collect()
}

fn inject_lines(fx: &Fixture, lines: &[String]) {
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    fx.inject(&batch("ml", &refs));
}

#[test]
fn exactly_the_line_cap_is_still_one_message() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    inject_lines(&fx, &numbered(100));
    let messages = since(&fx, start);
    assert_eq!(messages.len(), 1);
    assert_eq!(body(&messages[0]).matches('\n').count(), 99);
}

#[test]
fn past_the_line_cap_the_rest_arrives_one_by_one() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    inject_lines(&fx, &numbered(103));
    let messages = since(&fx, start);
    assert_eq!(messages.len(), 4, "one assembled message, then lines 101..103");
    let first = body(&messages[0]);
    assert_eq!(first.lines().count(), 100);
    assert!(first.starts_with("l0\n") && first.ends_with("\nl99"));
    assert_eq!(
        bodies(&messages[1..]),
        vec!["l100", "l101", "l102"],
        "the rest keep their order"
    );
}

#[test]
fn past_the_byte_cap_the_rest_arrives_one_by_one() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    // 16 lines of 4000 bytes are 64000 bytes, a 17th would pass 64 KiB.
    let big = "x".repeat(4000);
    let lines: Vec<String> = (0..18).map(|index| format!("{index}{big}")).collect();
    inject_lines(&fx, &lines);
    let messages = since(&fx, start);
    assert_eq!(messages.len(), 3, "one assembled message, then two lines");
    assert_eq!(body(&messages[0]).lines().count(), 16);
    assert!(body(&messages[1]).starts_with("16x"));
    assert!(body(&messages[2]).starts_with("17x"));
}

#[test]
fn a_nested_batch_past_the_cap_flushes_its_pieces_into_the_replay() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    let lines = numbered(101);
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    fx.inject(&history_with(&nested_batch(&refs)));
    assert!(since(&fx, start).is_empty(), "nothing leaks as live traffic");
    let batches = fx.batches();
    assert_eq!(batches.len(), 1);
    // before, the 100-line message, the 101st line, after.
    assert_eq!(batches[0].lines.len(), 4);
    assert_eq!(body(&batches[0].lines[1]).lines().count(), 100);
    assert_eq!(body(&batches[0].lines[2]), "l100");
}

// Not tracked.

#[test]
fn without_message_tags_the_lines_arrive_one_by_one() {
    let fx = Fixture::new(history_config());
    register_with(&fx, "batch draft/multiline");
    let start = fx.messages().len();
    fx.inject(&batch("ml", &["one", "two"]));
    assert_eq!(bodies(&since(&fx, start)), vec!["one", "two"]);
}

#[test]
fn without_the_batch_capability_the_lines_arrive_one_by_one() {
    let fx = Fixture::new(history_config());
    register_with(&fx, "message-tags draft/multiline");
    let start = fx.messages().len();
    fx.inject(&batch("ml", &["one", "two"]));
    assert_eq!(bodies(&since(&fx, start)), vec!["one", "two"]);
}

#[test]
fn without_the_multiline_capability_the_lines_arrive_one_by_one() {
    let fx = Fixture::new(history_config());
    register_with(&fx, "batch message-tags");
    let start = fx.messages().len();
    fx.inject(&batch("ml", &["one", "two"]));
    assert_eq!(bodies(&since(&fx, start)), vec!["one", "two"]);
}

#[test]
fn over_the_open_batch_budget_the_lines_arrive_one_by_one() {
    let fx = Fixture::new(history_config());
    register(&fx);
    fx.inject(&open_unknown_batches(16));
    let start = fx.messages().len();
    fx.inject(&batch("ml", &["one", "two"]));
    assert_eq!(bodies(&since(&fx, start)), vec!["one", "two"]);
    assert_eq!(fx.session.state(), SessionState::Registered);
}

#[test]
fn a_batch_inside_a_multiline_is_ignored() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    fx.inject(
        ":mira!u@h BATCH +ml draft/multiline #omarchy\r\n\
         @batch=ml :mira!u@h PRIVMSG #omarchy :one\r\n\
         @batch=ml :mira!u@h BATCH +kid netjoin irc.a irc.b\r\n\
         @batch=kid :bob!u@h PRIVMSG #omarchy :swallowed\r\n\
         @batch=ml :mira!u@h BATCH -kid\r\n\
         @batch=ml :mira!u@h PRIVMSG #omarchy :two\r\n\
         :mira!u@h BATCH -ml\r\n",
    );
    assert_eq!(bodies(&since(&fx, start)), vec!["one\ntwo"]);
}

#[test]
fn a_line_that_is_not_text_is_not_buffered() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    fx.inject(
        ":mira!u@h BATCH +ml draft/multiline #omarchy\r\n\
         @batch=ml :mira!u@h PRIVMSG #omarchy :one\r\n\
         @batch=ml :mira!u@h TOPIC #omarchy :nope\r\n\
         @batch=ml :mira!u@h PRIVMSG #omarchy :two\r\n\
         :mira!u@h BATCH -ml\r\n",
    );
    assert_eq!(bodies(&since(&fx, start)), vec!["one\ntwo"]);
}

#[test]
fn an_empty_batch_emits_nothing() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    fx.inject(&batch("ml", &[]));
    assert!(since(&fx, start).is_empty());
}

#[test]
fn our_own_echoed_batch_is_one_message_from_us() {
    let fx = Fixture::new(history_config());
    register(&fx);
    let start = fx.messages().len();
    fx.inject(
        ":omairc!u@h BATCH +ml draft/multiline #omarchy\r\n\
         @batch=ml :omairc!u@h PRIVMSG #omarchy :one\r\n\
         @batch=ml :omairc!u@h PRIVMSG #omarchy :two\r\n\
         :omairc!u@h BATCH -ml\r\n",
    );
    let messages = since(&fx, start);
    assert_eq!(messages.len(), 1);
    assert_eq!(body(&messages[0]), "one\ntwo");
    assert_eq!(messages[0].prefix.as_ref().unwrap().nick, b"omairc");
}
