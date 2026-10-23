use super::incoming::{incoming, severity_for};
use super::keep::KEEP_ROWS;
use super::outgoing::format_outgoing_cap_req;
use super::*;
use crate::irc::message::IrcMessage;
use crate::irc::parser::parse;
use chrono::TimeZone;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap()
}

fn msg(line: &str) -> IrcMessage {
    parse(line.as_bytes()).expect("parses")
}

/// The Status text of each entry `incoming_all` yields.
fn texts(line: &str) -> Vec<String> {
    incoming_all("net", &msg(line), "#&", now())
        .into_iter()
        .map(|entry| entry.text)
        .collect()
}

fn one(line: &str) -> StatusEntry {
    incoming("net", &msg(line), "#&", now())
}

#[test]
fn severities_follow_the_command() {
    assert_eq!(severity_for("PING"), LogSeverity::Trace);
    assert_eq!(severity_for("AUTHENTICATE"), LogSeverity::Trace);
    assert_eq!(severity_for("FAIL"), LogSeverity::Alert);
    assert_eq!(severity_for("WARN"), LogSeverity::Info);
    assert_eq!(severity_for("ERROR"), LogSeverity::Alert);
    assert_eq!(severity_for("433"), LogSeverity::Alert);
    assert_eq!(severity_for("512"), LogSeverity::Alert);
    assert_eq!(severity_for("001"), LogSeverity::Info);
    assert_eq!(severity_for("NOTICE"), LogSeverity::Info);
}

#[test]
fn a_004_splits_into_one_entry_per_advertised_field() {
    let line = ":srv 004 me demo.omairc OmaircDemo iw abc def";
    assert_eq!(
        texts(line),
        [
            "Host: demo.omairc",
            "IRCd: OmaircDemo",
            "User modes: iw",
            "Channel modes: abc",
            "Parametric channel modes: def",
        ]
    );
    assert!(incoming_all("net", &msg(line), "#&", now())
        .iter()
        .all(|entry| entry.label == "004"));
}

#[test]
fn a_short_004_keeps_only_the_fields_it_has() {
    assert_eq!(
        texts(":srv 004 me demo.omairc OmaircDemo iw"),
        ["Host: demo.omairc", "IRCd: OmaircDemo", "User modes: iw"]
    );
}

#[test]
fn cap_lines_name_the_subcommand_and_its_tokens() {
    assert_eq!(
        texts(":srv CAP me LS :echo-message message-tags batch"),
        ["Server supports: echo-message | message-tags | batch"]
    );
    assert_eq!(texts(":srv CAP me ACK :batch"), ["Acknowledged: batch"]);
    assert_eq!(texts(":srv CAP me NAK :"), ["Rejected:"]);
    assert_eq!(texts(":srv CAP me NEW :sasl"), ["Server added: sasl"]);
    assert_eq!(texts(":srv CAP me DEL :sasl"), ["Server removed: sasl"]);
}

#[test]
fn lusers_numerics_keep_their_text_and_nothing_else() {
    assert_eq!(
        texts(":srv 251 me :There are 9 users"),
        ["There are 9 users"]
    );
    assert_eq!(
        texts(":srv 252 me 3 :operators online"),
        ["3 operators online"]
    );
    assert_eq!(
        texts(":srv 265 me 9 12 :Current local users 9, max 12"),
        ["Current local users 9, max 12"]
    );
}

#[test]
fn whois_lines_format_their_known_numerics_and_mark_the_end() {
    let user = one(":srv 311 me kai ~kai host * :Kai Tanaka");
    assert_eq!(user.label, "whois");
    assert_eq!(user.text, "kai is ~kai@host (Kai Tanaka)");
    let line = user.whois_line.expect("whois line");
    assert_eq!(line.nick, "kai");
    assert!(!line.is_terminal());

    let channels = one(":srv 319 me kai :@#omarchy +#ricing");
    assert_eq!(channels.text, "kai is on @#omarchy +#ricing");

    let idle = one(":srv 317 me kai 42 1700000000 :seconds idle, signon time");
    assert_eq!(idle.text, "kai idle 42s, signon 1700000000");

    let end = one(":srv 318 me kai :End of /WHOIS list.");
    assert_eq!(end.text, "End of WHOIS for kai");
    assert!(end.whois_line.expect("line").is_terminal());

    let missing = one(":srv 401 me ghost :No such nick/channel");
    assert_eq!(missing.label, "401", "401 has no label, so the command");
    assert_eq!(missing.text, "No such nick: ghost");
    assert_eq!(
        missing.whois_line.expect("line").progress,
        WhoisProgress::Failed
    );
}

#[test]
fn a_whois_with_too_few_parameters_falls_back_to_the_plain_line() {
    let short = one(":srv 311 me kai ~kai");
    assert_eq!(short.label, "311");
    assert!(short.whois_line.is_none());
}

#[test]
fn a_notice_wraps_its_speaker_and_a_bare_notice_does_not() {
    let wrapped = one(":anna!a@host NOTICE me :hello there");
    assert_eq!(wrapped.label, "NOTICE");
    assert_eq!(wrapped.text, "-anna- hello there");

    let bare = one("NOTICE * :server says hi");
    assert_eq!(bare.text, "server says hi");
}

#[test]
fn a_ctcp_reply_notice_is_a_ctcp_line_that_names_its_reply() {
    let entry = one(":anna!a@host NOTICE me :\u{1}VERSION omairc-test\u{1}");
    assert_eq!(entry.label, "CTCP");
    assert_eq!(entry.text, "VERSION reply from anna: omairc-test");
    let reply = entry.ctcp_reply.expect("ctcp metadata");
    assert_eq!(reply.nick, "anna");
    assert_eq!(reply.command, "VERSION");
}

#[test]
fn a_ctcp_request_in_a_privmsg_reads_as_its_command_from_the_sender() {
    let entry = one(":anna!a@host PRIVMSG me :\u{1}PING 1234\u{1}");
    assert_eq!(entry.label, "CTCP");
    assert_eq!(entry.text, "PING 1234 from anna");
}

#[test]
fn an_invite_names_the_nick_and_the_channel() {
    let entry = one(":rio!r@host INVITE me #debian");
    assert_eq!(entry.label, "INVITE");
    assert_eq!(entry.text, "rio invited you to #debian");
}

#[test]
fn a_standard_reply_shows_its_description_on_status() {
    assert_eq!(
        texts(":srv FAIL AUTH BAD_ACCOUNT :Account is not set up"),
        ["Account is not set up"]
    );
}

#[test]
fn a_nick_refusal_names_the_reason_or_the_numeric() {
    assert_eq!(
        texts(":srv 433 * taken :Nickname is already in use"),
        ["taken Nickname is already in use"]
    );
    assert_eq!(
        texts(":srv 433 *"),
        ["433"],
        "a refusal with no sentence keeps the numeric"
    );
}

#[test]
fn a_chat_line_to_a_channel_stays_in_the_channel_not_status() {
    let line = msg(":anna!a@host PRIVMSG #omarchy :hi all");
    assert!(!keeps_incoming(&line, "fred", "#&"));
}

#[test]
fn a_direct_message_to_the_user_is_engaged_and_stays_out_of_status() {
    let line = msg(":anna!a@host PRIVMSG fred :hi");
    assert!(!keeps_incoming(&line, "fred", "#&"));
}

#[test]
fn a_services_privmsg_goes_to_status() {
    let line = msg(":NickServ!services@services.example PRIVMSG fred :You are identified");
    assert!(keeps_incoming(&line, "fred", "#&"));
}

#[test]
fn the_keep_table_drops_protocol_chatter_and_keeps_replies() {
    assert!(!keeps_incoming(&msg(":a!b@c JOIN #omarchy"), "fred", "#&"));
    assert!(!keeps_incoming(&msg("PING :srv"), "fred", "#&"));
    assert!(keeps_incoming(&msg(":srv 001 fred :Welcome"), "fred", "#&"));
    assert!(keeps_incoming(
        &msg(":srv ERROR :Closing link"),
        "fred",
        "#&"
    ));
    assert!(
        !keeps_incoming(&msg(":srv 303 fred :online"), "fred", "#&"),
        "303 is not a Status numeric"
    );
}

#[test]
fn the_keep_table_is_uppercase_and_unique() {
    for (index, row) in KEEP_ROWS.iter().enumerate() {
        assert!(!row.command.is_empty());
        assert!(
            row.command.bytes().all(|byte| !byte.is_ascii_lowercase()),
            "{} must be uppercase",
            row.command
        );
        assert!(
            KEEP_ROWS[..index]
                .iter()
                .all(|earlier| earlier.command != row.command),
            "{} appears twice",
            row.command
        );
    }
}

#[test]
fn an_outgoing_cap_req_is_kept_and_reads_as_requesting() {
    assert!(keeps_outgoing(b"CAP REQ :message-tags batch\r\n"));
    assert!(!keeps_outgoing(b"CAP END\r\n"));
    let entry = outgoing("net", b"CAP REQ :message-tags batch\r\n", "#&", now());
    assert_eq!(entry.text, "Requesting: message-tags | batch");
    assert_eq!(entry.source, LogSource::Client);
}

#[test]
fn an_outgoing_pass_is_kept_with_its_secret_masked() {
    assert!(keeps_outgoing(b"PASS hunter2\r\n"));
    let entry = outgoing("net", b"PASS hunter2\r\n", "#&", now());
    assert!(
        !entry.text.contains("hunter2"),
        "a password never reaches Status"
    );
    assert_eq!(entry.label, "PASS");
}

#[test]
fn an_outgoing_line_strips_either_line_ending() {
    let crlf = outgoing("net", b"PRIVMSG #omarchy :hi\r\n", "#&", now());
    let lf = outgoing("net", b"PRIVMSG #omarchy :hi\n", "#&", now());
    assert_eq!(crlf.text, "PRIVMSG #omarchy :hi");
    assert_eq!(crlf.text, lf.text);
    assert_eq!(crlf.label, "PRIVMSG");
}

#[test]
fn local_lines_carry_the_local_source_and_their_label() {
    let outcome_line = outcome("net", "Command was refused", now());
    assert_eq!(outcome_line.label, "command");
    assert_eq!(outcome_line.source, LogSource::Local);
    assert_eq!(outcome_line.severity, LogSeverity::Info);

    let life = lifecycle(
        "net",
        LogSeverity::Alert,
        "connect",
        "Connection lost",
        now(),
    );
    assert_eq!(life.source, LogSource::Local);
    assert_eq!(life.severity, LogSeverity::Alert);
    assert_eq!(life.timestamp, now());
}

#[test]
fn the_request_label_is_recorded_on_the_entry() {
    let mut entry = outcome("net", "ok", now());
    assert!(entry.request_label.is_empty());
    entry.set_request_label("req-7");
    assert_eq!(entry.request_label, "req-7");
}

/// Ported from `TestSessionConfiguredPasswordNeverAppearsInStatusEntries`: the
/// server password reaches Status only as `PASS ***`.
#[test]
fn a_configured_password_reads_as_pass_stars() {
    let entry = outgoing("net", b"PASS hunter2\r\n", "", now());
    assert_eq!(entry.label, "PASS");
    assert_eq!(entry.text, "PASS ***");
}

/// Ported from `TestSessionKeyedJoinIsRedactedInStatusEntries`.
#[test]
fn a_keyed_join_is_masked_and_an_unkeyed_one_is_not() {
    let keyed = outgoing("net", b"JOIN #secret hunter2\r\n", "", now());
    assert_eq!(keyed.text, "JOIN #secret ***");
    assert!(!keyed.text.contains("hunter2"));
    assert_eq!(
        outgoing("net", b"JOIN #omarchy\r\n", "", now()).text,
        "JOIN #omarchy"
    );
}

/// Ported from `TestSessionSelfEchoToServiceIsRedacted`: a self-echoed
/// services command never shows its secret, and a harmless `set` line does.
#[test]
fn a_self_echo_to_a_service_masks_its_secret() {
    let cases = [
        (
            ":me!u@h PRIVMSG nickserv :identify my_nick s3cret",
            "IDENTIFY ***",
        ),
        (
            ":myserv!u@h PRIVMSG nickserv :identify my_nick s3cret",
            "IDENTIFY ***",
        ),
        (
            ":me!u@h PRIVMSG nickserv IDENTIFY my_nick s3cret",
            "IDENTIFY ***",
        ),
        (
            ":me!u@h PRIVMSG nickserv :\u{1}IDENTIFY my_nick s3cret\u{1}",
            "IDENTIFY ***",
        ),
        (
            ":me!u@h PRIVMSG nickserv :set password s3cret",
            "SET PASSWORD ***",
        ),
        (
            ":me!u@h PRIVMSG nickserv :set email user@example.net",
            "set email user@example.net",
        ),
    ];
    for (line, want) in cases {
        let entry = incoming("freenode", &msg(line), "", now());
        assert_eq!(entry.text, want, "{line:?}");
        assert!(!entry.text.contains("s3cret"), "{line:?} leaked");
    }
    let ctcp = incoming(
        "freenode",
        &msg(":me!u@h PRIVMSG nickserv :\u{1}IDENTIFY my_nick s3cret\u{1}"),
        "",
        now(),
    );
    assert_eq!(ctcp.label, "PRIVMSG");
}

#[test]
fn a_cap_req_check_never_splits_a_character() {
    // A multi-byte first character must not panic the byte-prefix test.
    assert!(!keeps_outgoing("é\r\n".as_bytes()));
    assert!(format_outgoing_cap_req("éCAP").is_none());
}
