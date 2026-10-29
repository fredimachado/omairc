//! Outbound writers, metadata limits, and CTCP answers.
//! Status-entry assertions (`handler.status`, `hasLabel`, `anyFieldContains`)
//! live with the Status recorder elsewhere. The CTCP reply text is this app's
//! `https://omairc.app` (no build number), as in `ctcp_version_reply_text`.

use std::time::Duration;

use super::fixture::{test_config, Fixture};
use crate::irc::message::MAX_CLASSIC_FRAME_BYTES;
use crate::irc::presence::{clamped_to, effective_max_value_bytes, MAXIMUM_VALUE_BYTES};
use crate::session::events::SessionState;

fn ctcp_version_reply(nick: &str) -> String {
    format!("NOTICE {nick} :\u{1}VERSION https://omairc.app\u{1}\r\n")
}

fn count_frame(frames: &[String], frame: &str) -> usize {
    frames
        .iter()
        .filter(|candidate| *candidate == frame)
        .count()
}

fn frames_contain(frames: &[String], needle: &str) -> bool {
    frames.iter().any(|frame| frame.contains(needle))
}

/// A registered fixture with `multi-prefix` negotiated away and 001 received.
fn registered() -> Fixture {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :multi-prefix\r\n:server 001 omairc :Welcome\r\n");
    assert_eq!(fx.session.state(), SessionState::Registered);
    fx
}

#[test]
fn send_privmsg_validates_target() {
    let fx = registered();
    assert!(
        fx.session.send_privmsg("#omarchy", "hello"),
        "channel PRIVMSG must succeed"
    );
    assert!(
        fx.session.send_privmsg("lena", "hello"),
        "direct PRIVMSG must succeed"
    );
    assert_eq!(fx.last_frame(), "PRIVMSG lena :hello\r\n");
    assert!(fx.wrote("PRIVMSG #omarchy :hello\r\n"));

    let before = fx.frames().len();
    for target in [
        "",
        " ",
        "lena smith",
        "lena\t",
        "lena\n",
        "lena\r",
        "\u{1}lena",
        ":lena",
    ] {
        assert!(
            !fx.session.send_privmsg(target, "hello"),
            "send_privmsg({target:?}) must fail"
        );
    }
    assert_eq!(
        fx.frames().len(),
        before,
        "invalid targets must not write frames"
    );
}

#[test]
fn send_ctcp_writes_query() {
    let fx = registered();
    assert!(fx.session.send_ctcp("lena", "VERSION", "", ""));
    assert_eq!(fx.last_frame(), "PRIVMSG lena :\u{1}VERSION\u{1}\r\n");
    assert!(fx.session.send_ctcp("lena", "time", "", ""));
    assert_eq!(fx.last_frame(), "PRIVMSG lena :\u{1}TIME\u{1}\r\n");
    assert!(fx.session.send_ctcp("lena", "PING", "42", ""));
    assert_eq!(fx.last_frame(), "PRIVMSG lena :\u{1}PING 42\u{1}\r\n");

    let before = fx.frames().len();
    assert!(!fx.session.send_ctcp("", "VERSION", "", ""));
    assert!(!fx.session.send_ctcp("lena smith", "VERSION", "", ""));
    assert!(!fx.session.send_ctcp("lena", "", "", ""));
    assert!(!fx.session.send_ctcp("lena", "PING PONG", "", ""));
    assert_eq!(
        fx.frames().len(),
        before,
        "invalid CTCP queries must not write frames"
    );
}

#[test]
fn send_privmsg_splits_near_two_frames() {
    let fx = Fixture::new(test_config());
    fx.register_with_welcome();
    let before = fx.frames().len();
    let first = "a".repeat(400);
    let second = "b".repeat(200);
    assert!(fx
        .session
        .send_privmsg("#omarchy", &format!("{first} {second}")));
    let frames = fx.written_since(before);
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0], format!("PRIVMSG #omarchy :{first}\r\n"));
    assert_eq!(frames[1], format!("PRIVMSG #omarchy :{second}\r\n"));
    for frame in &frames {
        assert!(frame.ends_with("\r\n") && frame.len() <= MAX_CLASSIC_FRAME_BYTES);
    }
}

#[test]
fn send_privmsg_splits_enormous_token() {
    let fx = Fixture::new(test_config());
    fx.register_with_welcome();
    let before = fx.frames().len();
    let prefix = "PRIVMSG #omarchy :";
    let max_body = MAX_CLASSIC_FRAME_BYTES - prefix.len() - 2;
    let token = "x".repeat(max_body * 2 + 16);
    assert!(fx.session.send_privmsg("#omarchy", &token));
    let frames = fx.written_since(before);
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[0], format!("{prefix}{}\r\n", &token[..max_body]));
    assert_eq!(
        frames[1],
        format!("{prefix}{}\r\n", &token[max_body..max_body * 2])
    );
    assert_eq!(frames[2], format!("{prefix}{}\r\n", &token[max_body * 2..]));
    for frame in &frames {
        assert!(frame.ends_with("\r\n") && frame.len() <= MAX_CLASSIC_FRAME_BYTES);
    }
}

#[test]
fn set_topic_is_set_only() {
    let fx = registered();
    let before = fx.frames().len();
    assert!(!fx.session.set_topic("#omarchy", ""));
    assert!(!fx.session.set_topic("", "hello"));
    assert_eq!(
        fx.frames().len(),
        before,
        "invalid TOPIC must not write a frame"
    );
    assert!(fx.session.set_topic("#omarchy", "hello"));
    assert_eq!(fx.last_frame(), "TOPIC #omarchy :hello\r\n");
}

#[test]
fn kick_writes_optional_reason() {
    let fx = registered();
    let before = fx.frames().len();
    assert!(!fx.session.kick("", "alice", "spam"));
    assert!(!fx.session.kick("#omarchy", "", "spam"));
    assert_eq!(fx.frames().len(), before);
    assert!(fx.session.kick("#omarchy", "alice", "spam"));
    assert_eq!(fx.last_frame(), "KICK #omarchy alice :spam\r\n");
    assert!(fx.session.kick("#omarchy", "alice", ""));
    assert_eq!(fx.last_frame(), "KICK #omarchy alice\r\n");
}

#[test]
fn kickban_writes_mode_then_kick_without_waiting() {
    let fx = registered();
    let before = fx.frames().len();
    assert!(!fx.session.kickban("", "alice!*@*", "alice", "spam"));
    assert!(!fx.session.kickban("#omarchy", "", "alice", "spam"));
    assert!(!fx.session.kickban("#omarchy", "alice!*@*", "", "spam"));
    assert_eq!(fx.frames().len(), before, "invalid kickban must not write");

    assert!(fx.session.kickban("#omarchy", "alice!*@*", "alice", "spam"));
    let frames = fx.frames();
    assert_eq!(
        &frames[frames.len() - 2..],
        [
            "MODE #omarchy +b alice!*@*\r\n".to_owned(),
            "KICK #omarchy alice :spam\r\n".to_owned(),
        ]
    );

    assert!(fx.session.kickban("#omarchy", "bob!u@h", "bob", ""));
    let frames = fx.frames();
    assert_eq!(
        &frames[frames.len() - 2..],
        [
            "MODE #omarchy +b bob!u@h\r\n".to_owned(),
            "KICK #omarchy bob\r\n".to_owned(),
        ]
    );
}

#[test]
fn invite_writes_nick_then_channel() {
    let fx = registered();
    let before = fx.frames().len();
    assert!(!fx.session.invite("", "#lab"));
    assert!(!fx.session.invite("bob", ""));
    assert_eq!(fx.frames().len(), before);
    assert!(fx.session.invite("bob", "#lab"));
    assert_eq!(fx.last_frame(), "INVITE bob #lab\r\n");
}

#[test]
fn set_away_encodes_optional_reason() {
    let fx = registered();
    let steps: Vec<(Box<dyn Fn(&Fixture) -> bool>, &str)> = vec![
        (
            Box::new(|fx| fx.session.set_away("lunch")),
            "AWAY :lunch\r\n",
        ),
        (Box::new(|fx| fx.session.set_away("")), "AWAY\r\n"),
        (Box::new(|fx| fx.session.set_away("   ")), "AWAY\r\n"),
        (Box::new(|fx| fx.session.clear_away()), "AWAY\r\n"),
        (Box::new(|fx| fx.session.mark_away("")), "AWAY :\r\n"),
        (Box::new(|fx| fx.session.mark_away("   ")), "AWAY :\r\n"),
        (
            Box::new(|fx| fx.session.mark_away("lunch")),
            "AWAY :lunch\r\n",
        ),
    ];
    for (index, (call, want)) in steps.iter().enumerate() {
        assert!(call(&fx), "step {index} failed");
        assert_eq!(fx.last_frame(), *want, "step {index}");
    }
}

#[test]
fn set_own_metadata_writes_set_and_clear() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :batch draft/metadata-2\r\n\
         :server CAP omairc ACK :batch draft/metadata-2\r\n\
         :server 001 omairc :Welcome\r\n",
    );
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert_eq!(
        fx.session.set_own_metadata("status", "writing").as_deref(),
        Some("writing")
    );
    assert_eq!(fx.last_frame(), "METADATA * SET status :writing\r\n");
    assert!(fx.session.clear_own_metadata("status"));
    assert_eq!(fx.last_frame(), "METADATA * SET status\r\n");

    let after_clear = fx.frames().len();
    assert!(fx.session.set_own_metadata("unknown", "x").is_none());
    assert!(fx.session.set_own_metadata("", "x").is_none());
    assert_eq!(
        fx.frames().len(),
        after_clear,
        "invalid metadata must not write a frame"
    );

    let without_cap = Fixture::new(test_config());
    without_cap.connect_tls();
    without_cap.inject(":server CAP omairc LS :multi-prefix\r\n:server 001 omairc :Welcome\r\n");
    let before = without_cap.frames().len();
    assert!(without_cap
        .session
        .set_own_metadata("status", "writing")
        .is_none());
    assert!(!without_cap.session.clear_own_metadata("status"));
    assert_eq!(
        without_cap.frames().len(),
        before,
        "metadata without the capability must not write"
    );
}

#[test]
fn metadata_capability_limits_subscriptions_and_values() {
    let limited = Fixture::new(test_config());
    limited.connect_tls();
    limited.inject(
        ":server CAP omairc LS :batch draft/metadata-2=max-subs=2,max-value-bytes=8\r\n\
         :server CAP omairc ACK :batch draft/metadata-2\r\n\
         :server 001 omairc :Welcome\r\n",
    );
    assert_eq!(limited.session.state(), SessionState::Registered);
    let metadata = limited.session.metadata_capability();
    assert_eq!(metadata.max_subs, Some(2));
    assert_eq!(metadata.max_value_bytes, Some(8));
    assert!(frames_contain(
        &limited.frames(),
        "METADATA * SUB status avatar\r\n"
    ));
    assert!(!frames_contain(
        &limited.frames(),
        "METADATA * SUB status avatar bot"
    ));

    assert!(limited
        .session
        .set_own_metadata("status", "abcdefghijk")
        .is_some());
    assert_eq!(limited.last_frame(), "METADATA * SET status :abcdefgh\r\n");

    assert!(limited
        .session
        .set_own_metadata("status", "café!!!!")
        .is_some());
    let last = limited.last_frame();
    let prefix = "METADATA * SET status :";
    assert!(
        last.starts_with(prefix) && last.ends_with("\r\n"),
        "{last:?}"
    );
    assert_eq!(last[prefix.len()..].trim_end_matches("\r\n").len(), 8);

    let zero_subs = Fixture::new(test_config());
    zero_subs.connect_tls();
    zero_subs.inject(
        ":server CAP omairc LS :batch draft/metadata-2=max-subs=0\r\n\
         :server CAP omairc ACK :batch draft/metadata-2\r\n\
         :server 001 omairc :Welcome\r\n",
    );
    assert!(
        !frames_contain(&zero_subs.frames(), "METADATA * SUB"),
        "max-subs=0 must not subscribe"
    );

    let malformed = Fixture::new(test_config());
    malformed.connect_tls();
    malformed.inject(
        ":server CAP omairc LS :batch \
         draft/metadata-2=max-subs=-3,max-value-bytes=nope,max-value-bytes=99999\r\n\
         :server CAP omairc ACK :batch draft/metadata-2\r\n\
         :server 001 omairc :Welcome\r\n",
    );
    let metadata = malformed.session.metadata_capability();
    assert_eq!(metadata.max_subs, None);
    assert_eq!(metadata.max_value_bytes, Some(99999));
    assert!(frames_contain(
        &malformed.frames(),
        "METADATA * SUB status avatar bot display-name pronouns homepage color\r\n"
    ));
    assert_eq!(
        effective_max_value_bytes(metadata.max_value_bytes),
        MAXIMUM_VALUE_BYTES
    );

    let huge = "x".repeat(600);
    assert_eq!(
        clamped_to(&huge, MAXIMUM_VALUE_BYTES).len(),
        MAXIMUM_VALUE_BYTES
    );
    assert!(malformed
        .session
        .set_own_metadata("status", &huge)
        .is_some());
    let huge_frame = malformed.last_frame();
    let set_prefix = "METADATA * SET status :";
    let wire_budget = MAX_CLASSIC_FRAME_BYTES - set_prefix.len() - 2;
    let want = MAXIMUM_VALUE_BYTES.min(wire_budget);
    assert_eq!(
        huge_frame[set_prefix.len()..]
            .trim_end_matches("\r\n")
            .len(),
        want
    );
    assert_eq!(huge_frame.len(), MAX_CLASSIC_FRAME_BYTES);

    let legacy = Fixture::new(test_config());
    legacy.connect_tls();
    legacy.inject(
        ":server CAP omairc LS :batch draft/metadata-2\r\n\
         :server CAP omairc ACK :batch draft/metadata-2\r\n\
         :server 001 omairc :Welcome\r\n",
    );
    let metadata = legacy.session.metadata_capability();
    assert!(metadata.max_subs.is_none() && metadata.max_value_bytes.is_none());
    assert!(frames_contain(
        &legacy.frames(),
        "METADATA * SUB status avatar bot display-name pronouns homepage color\r\n"
    ));

    let zero_value = Fixture::new(test_config());
    zero_value.connect_tls();
    zero_value.inject(
        ":server CAP omairc LS :batch draft/metadata-2=max-value-bytes=0\r\n\
         :server CAP omairc ACK :batch draft/metadata-2\r\n\
         :server 001 omairc :Welcome\r\n",
    );
    let metadata = zero_value.session.metadata_capability();
    assert_eq!(metadata.max_value_bytes, Some(0));
    assert_eq!(effective_max_value_bytes(metadata.max_value_bytes), 0);
    let before_zero_set = zero_value.frames().len();
    assert!(zero_value
        .session
        .set_own_metadata("status", "blocked")
        .is_none());
    assert_eq!(zero_value.frames().len(), before_zero_set);
    assert!(zero_value.session.clear_own_metadata("status"));
    assert_eq!(zero_value.last_frame(), "METADATA * SET status\r\n");
}

#[test]
fn whois_writes_doubled_nick() {
    let fx = registered();
    assert!(fx.session.whois("lena", ""));
    assert_eq!(fx.last_frame(), "WHOIS lena lena\r\n");
    let before = fx.frames().len();
    assert!(!fx.session.whois("", ""));
    assert!(!fx.session.whois("   ", ""));
    assert_eq!(fx.frames().len(), before);
}

#[test]
fn ctcp_from_server_prefix_gets_no_reply() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server 001 omairc :Welcome\r\n");
    fx.inject(
        ":services.example.net PRIVMSG omairc :\u{1}VERSION\u{1}\r\n\
         :services PRIVMSG omairc :\u{1}VERSION\u{1}\r\n\
         :NickServ!NickServ@services PRIVMSG omairc :\u{1}VERSION\u{1}\r\n",
    );
    for nick in ["services.example.net", "services", "NickServ"] {
        assert!(
            !fx.wrote(&ctcp_version_reply(nick)),
            "server-prefixed CTCP answered for {nick}"
        );
    }
}

#[test]
fn ctcp_to_folded_self_nick_is_answered() {
    let mut config = test_config();
    config.nick = "omairc[m]".to_owned();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(
        ":server 001 omairc[m] :Welcome\r\n\
         :server 005 omairc[m] CASEMAPPING=rfc1459 :are supported\r\n",
    );
    fx.inject(":alice!u@h PRIVMSG omairc{m} :\u{1}PING one\u{1}\r\n");
    assert!(
        fx.wrote("NOTICE alice :\u{1}PING one\u{1}\r\n"),
        "a folded self nick must be answered"
    );
}

#[test]
fn answers_ctcp_requests() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server 001 omairc :Welcome\r\n");
    fx.inject(
        ":MetaNova!u@h PRIVMSG omairc :\u{1}PING token\u{1}\r\n\
         :alice!u@h PRIVMSG omairc :\u{1}TIME\u{1}\r\n\
         :bob!u@h PRIVMSG omairc :\u{1}VERSION\u{1}\r\n",
    );
    assert!(fx.wrote("NOTICE MetaNova :\u{1}PING token\u{1}\r\n"));
    assert!(fx.wrote(&ctcp_version_reply("bob")));
    assert!(
        fx.frames()
            .iter()
            .any(|frame| frame.starts_with("NOTICE alice :\u{1}TIME ")),
        "the TIME request must be answered"
    );
}

#[test]
fn answers_nick_only_ctcp_requests() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server 001 omairc :Welcome\r\n");
    fx.inject(":lena PRIVMSG omairc :\u{1}VERSION\u{1}\r\n");
    assert!(
        fx.wrote(&ctcp_version_reply("lena")),
        "a nick-only prefix must be answered"
    );
}

#[test]
fn rate_limits_ctcp_version_replies_per_nick() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server 001 omairc :Welcome\r\n");
    let probe = ":MetaNova!u@h PRIVMSG omairc :\u{1}VERSION\u{1}\r\n";
    let reply = ctcp_version_reply("MetaNova");
    let other_probe = ":alice!u@h PRIVMSG omairc :\u{1}VERSION\u{1}\r\n";
    let other_reply = ctcp_version_reply("alice");

    fx.inject(&format!("{probe}{probe}"));
    assert_eq!(count_frame(&fx.frames(), &reply), 1, "MetaNova replies");
    fx.inject(other_probe);
    assert_eq!(count_frame(&fx.frames(), &other_reply), 1, "alice replies");

 // The C++ test sleeps 5500 ms; the fake clock is advanced instead.
    fx.clock.advance(Duration::from_millis(5500));
    fx.inject(probe);
    assert_eq!(
        count_frame(&fx.frames(), &reply),
        2,
        "MetaNova replies after the interval"
    );
}

#[test]
fn drops_oversized_ctcp_ping_payload() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server 001 omairc :Welcome\r\n");
    let too_long = "x".repeat(33);
    let exact = "y".repeat(32);
    fx.inject(&format!(
        ":MetaNova!u@h PRIVMSG omairc :\u{1}PING {too_long}\u{1}\r\n"
    ));
    assert!(!fx.wrote(&format!("NOTICE MetaNova :\u{1}PING {too_long}\u{1}\r\n")));
    assert!(!fx.wrote(&format!(
        "NOTICE MetaNova :\u{1}PING {}\u{1}\r\n",
        "x".repeat(32)
    )));

    fx.inject(&format!(
        ":MetaNova!u@h PRIVMSG omairc :\u{1}PING {exact}\u{1}\r\n"
    ));
    assert!(fx.wrote(&format!("NOTICE MetaNova :\u{1}PING {exact}\u{1}\r\n")));
}

#[test]
fn does_not_answer_channel_ctcp_requests() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server 001 omairc :Welcome\r\n");
    let before = fx.frames().len();
    fx.inject(":MetaNova!u@h PRIVMSG #omarchy :\u{1}PING token\u{1}\r\n");
    assert_eq!(
        fx.frames().len(),
        before,
        "a channel CTCP must not be answered"
    );
}

#[test]
fn welcome_assigns_nick_from_001() {
    let mut config = test_config();
    config.nick = "omairc-very-long-name".to_owned();
    let fx = Fixture::new(config);
    assert_eq!(fx.session.nick(), "omairc-very-long-name");
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc-very-long-name LS :multi-prefix\r\n\
         :server 001 omairc-truncated :Welcome\r\n",
    );
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert_eq!(fx.session.nick(), "omairc-truncated");
}

#[test]
fn empty_welcome_keeps_config_nick() {
    let fx = Fixture::new(test_config());
    assert_eq!(fx.session.nick(), "omairc");
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :multi-prefix\r\n:server 001\r\n");
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert_eq!(fx.session.nick(), "omairc");
}

#[test]
fn self_nick_updates_session_nick() {
    let mut config = test_config();
    config.nick = "omairc-very-long-name".to_owned();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc-very-long-name LS :multi-prefix\r\n\
         :server 001 omairc-truncated :Welcome\r\n\
         :Alice!u@h NICK :Alicia\r\n",
    );
    assert_eq!(fx.session.nick(), "omairc-truncated");
    fx.inject(":omairc-truncated!u@h NICK :fred\r\n");
    assert_eq!(fx.session.nick(), "fred");
}
