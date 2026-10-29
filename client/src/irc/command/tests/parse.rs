//! The parser: each verb's argument shape, unknown words, and the plain-text and scope rules.

use super::*;

#[test]
fn parse_command_table() {
    let cases: Vec<(&str, Command)> = vec![
        ("", Command::Empty),
        ("   ", Command::Empty),
        ("hello world", say("hello world")),
        ("  spaced  ", say("spaced")),
        ("//away", say("/away")),
        ("///x", say("//x")),
        ("//", say("/")),
        ("/j #help", Command::Join("#help".into())),
        ("/join", Command::Join(String::new())),
        ("/JOIN  #help  ", Command::Join("#help".into())),
        ("/back extra", Command::Back),
        ("/bogus", Command::Unknown("/bogus".into())),
        ("/", Command::Unknown("/".into())),
    ];
    for (input, want) in cases {
        assert_eq!(parse(input), want, "parse({input:?})");
    }
}

#[test]
fn verbs_keep_their_argument_and_ignore_case() {
    assert_eq!(parse("/ME waves"), Command::Action("waves".into()));
    assert!(parse("/ME waves").is_live_message());
    assert_eq!(
        parse("/LEAVE #omarchy"),
        Command::Part(words("#omarchy", ""))
    );
    assert_eq!(parse("/part"), Command::Part(Words::default()));
    assert_eq!(parse("/leave"), Command::Part(Words::default()));
    assert_eq!(parse("/join"), Command::Join(String::new()));
    assert_eq!(
        parse("/disconnect later"),
        Command::Disconnect("later".into())
    );
    assert_eq!(parse("/quit later"), Command::Disconnect("later".into()));
}

#[test]
fn unknown_words_keep_their_slash_and_their_argument_is_dropped() {
    assert_eq!(parse("/nope"), Command::Unknown("/nope".into()));
    assert_eq!(parse("/nope extra"), Command::Unknown("/nope".into()));
    assert_eq!(parse("/q"), Command::Unknown("/q".into()));
    assert_eq!(parse("/t"), Command::Unknown("/t".into()));
}

#[test]
fn close_and_back_drop_their_arguments() {
    assert_eq!(parse("/close"), Command::Close);
    assert!(!parse("/close").is_live_message());
    assert_eq!(parse("//close"), say("/close"));
    assert_eq!(parse("/back leftover"), Command::Back);
    assert_eq!(parse("/BACK leftover"), Command::Back);
    assert_eq!(parse("//away lunch"), say("/away lunch"));
}

#[test]
fn query_msg_and_notice_split_the_first_word() {
    assert_eq!(parse("/query lena hi"), Command::Query(words("lena", "hi")));
    assert_eq!(parse("/MSG lena"), Command::Msg(words("lena", "")));
    assert_eq!(parse("/msg lena hi"), Command::Msg(words("lena", "hi")));
    assert_eq!(parse("//query"), say("/query"));
    assert_eq!(parse("//msg hi"), say("/msg hi"));
    assert_eq!(
        parse("/notice lena later"),
        Command::Notice(words("lena", "later"))
    );
    assert_eq!(
        parse("/NOTICE #omarchy hi"),
        Command::Notice(words("#omarchy", "hi"))
    );
    assert_eq!(parse("//notice hi"), say("/notice hi"));
}

#[test]
fn topic_and_away_keep_the_whole_text() {
    assert_eq!(parse("/topic"), Command::Topic(String::new()));
    assert_eq!(
        parse("/TOPIC Hello world"),
        Command::Topic("Hello world".into())
    );
    assert_eq!(
        parse("/topic #omarchy is the place"),
        Command::Topic("#omarchy is the place".into())
    );
    assert_eq!(parse("/away lunch"), Command::Away("lunch".into()));
    assert_eq!(parse("/AWAY lunch"), Command::Away("lunch".into()));
    assert_eq!(parse("/away"), Command::Away(String::new()));
}

#[test]
fn autoaway_keeps_its_raw_argument() {
    assert_eq!(parse("/autoaway"), Command::Autoaway(String::new()));
    assert_eq!(parse("/autoaway 15"), Command::Autoaway("15".into()));
    assert_eq!(parse("/AUTOAWAY off"), Command::Autoaway("off".into()));
    assert_eq!(parse("//autoaway 15"), say("/autoaway 15"));
}

#[test]
fn status_and_avatar_keep_the_whole_argument() {
    assert_eq!(parse("/status"), Command::Status(String::new()));
    assert_eq!(
        parse("/status writing docs"),
        Command::Status("writing docs".into())
    );
    assert_eq!(parse("/STATUS clear"), Command::Status("clear".into()));
    assert_eq!(
        parse("/status clear the table"),
        Command::Status("clear the table".into())
    );
    assert_eq!(parse("//status lunch"), say("/status lunch"));
    assert_eq!(parse("/avatar"), Command::Avatar(String::new()));
    assert_eq!(
        parse("/avatar https://example.com/a.png"),
        Command::Avatar("https://example.com/a.png".into())
    );
    assert_eq!(
        parse("/avatar me@example.com"),
        Command::Avatar("me@example.com".into())
    );
    assert_eq!(parse("/AVATAR clear"), Command::Avatar("clear".into()));
}

#[test]
fn whois_and_the_ctcp_queries_split_the_first_word() {
    assert_eq!(parse("/whois lena"), Command::Whois(words("lena", "")));
    assert_eq!(parse("/WHOIS"), Command::Whois(Words::default()));
    assert_eq!(parse("//whois lena"), say("/whois lena"));
    assert_eq!(parse("/ping lena"), Command::Ping(words("lena", "")));
    assert_eq!(parse("/PING"), Command::Ping(Words::default()));
    assert_eq!(parse("/TIME lena"), Command::Time(words("lena", "")));
    assert_eq!(parse("/version lena"), Command::Version(words("lena", "")));
    assert_eq!(parse("/version"), Command::Version(Words::default()));
    assert_eq!(parse("//ping lena"), say("/ping lena"));
}

#[test]
fn mode_kick_and_invite() {
    assert_eq!(
        parse("/mode #omarchy +o lena"),
        Command::Mode("#omarchy +o lena".into())
    );
    assert_eq!(parse("/MODE #omarchy"), Command::Mode("#omarchy".into()));
    assert_eq!(parse("/mode"), Command::Mode(String::new()));
    assert_eq!(parse("//mode #omarchy"), say("/mode #omarchy"));
    assert_eq!(parse("/kick bob spam"), Command::Kick(words("bob", "spam")));
    assert_eq!(
        parse("/KICK #omarchy bob"),
        Command::Kick(words("#omarchy", "bob"))
    );
    assert_eq!(parse("/kick"), Command::Kick(Words::default()));
    assert_eq!(parse("//kick bob"), say("/kick bob"));
    assert_eq!(
        parse("/invite bob #lab"),
        Command::Invite(words("bob", "#lab"))
    );
    assert_eq!(parse("/INVITE bob"), Command::Invite(words("bob", "")));
    assert_eq!(parse("/invite"), Command::Invite(Words::default()));
    assert_eq!(parse("//invite bob"), say("/invite bob"));
}

#[test]
fn op_family_services_raw_help_and_list() {
    assert_eq!(parse("/op alice"), Command::Op(words("alice", "")));
    assert!(!parse("/op alice").is_live_message());
    assert_eq!(parse("/DEOP alice"), Command::Deop(words("alice", "")));
    assert_eq!(parse("/voice bob"), Command::Voice(words("bob", "")));
    assert_eq!(parse("/devoice bob"), Command::Devoice(words("bob", "")));
    assert_eq!(parse("/ban eve"), Command::Ban(words("eve", "")));
    assert_eq!(
        parse("/kickban #room bob spam"),
        Command::Kickban(words("#room", "bob spam"))
    );
    assert_eq!(parse("/kb bob"), Command::Kickban(words("bob", "")));
    assert_eq!(parse("/unban bob!*@*"), Command::Unban(words("bob!*@*", "")));
    assert_eq!(parse("/quiet eve"), Command::Quiet(words("eve", "")));
    assert_eq!(parse("/unquiet eve"), Command::Unquiet(words("eve", "")));
    assert_eq!(parse("/bans"), Command::Bans(String::new()));
    assert_eq!(parse("/bans extra"), Command::Bans("extra".into()));
    assert_eq!(parse("/quiets"), Command::Quiets(String::new()));
    assert_eq!(
        parse("/ns identify hunter2"),
        Command::Ns("identify hunter2".into())
    );
    assert_eq!(parse("/CS help"), Command::Cs("help".into()));
    assert_eq!(parse("/znc ListMods"), Command::Znc("ListMods".into()));
    assert_eq!(parse("/raw PING :x"), Command::Raw("PING :x".into()));
    assert_eq!(parse("/quote PING :x"), Command::Raw("PING :x".into()));
    assert_eq!(parse("/help"), Command::Help);
    assert_eq!(parse("/help me"), Command::Help);
    assert_eq!(parse("/list"), Command::List(String::new()));
    assert_eq!(parse("/LIST #foo*"), Command::List("#foo*".into()));
    assert_eq!(parse("//list"), say("/list"));
    assert_eq!(parse("//op alice"), say("/op alice"));
}

#[test]
fn ns_cs_and_znc_run_from_either_surface() {
    for line in ["/ns identify hunter2", "/cs help", "/znc ListMods"] {
        let command = parse(line);
        assert_eq!(command.rejection(CONVERSATION), None, "{line}");
        assert_eq!(command.rejection(STATUS), None, "{line}");
    }
}

#[test]
fn a_double_slash_and_triple_slash_stay_plain_text() {
    assert_eq!(parse("//away"), say("/away"));
    assert_eq!(parse("///x"), say("//x"));
    assert_eq!(parse("//join"), say("/join"));
    assert!(parse("//join").allowed_on(CONVERSATION));
    assert!(!parse("//join").allowed_on(STATUS));
}

#[test]
fn an_empty_me_is_refused_on_a_conversation_and_a_me_is_scoped_out_of_status() {
    assert_eq!(
        parse("/me").rejection(CONVERSATION),
        Some(CommandOutcome::Refused)
    );
    assert_eq!(
        command_outcome_text(CommandOutcome::Refused, &parse("/me")),
        "Command was refused"
    );
    assert_eq!(parse("/me hi").rejection(CONVERSATION), None);
    assert_eq!(
        command_outcome_text(CommandOutcome::WrongScope, &parse("/me hi")),
        "Select a connected conversation first"
    );
}

#[test]
fn plain_text_is_wrong_scope_on_status_only_and_blank_is_never_rejected() {
    assert_eq!(parse("hello").rejection(CONVERSATION), None);
    assert_eq!(
        parse("hello").rejection(STATUS),
        Some(CommandOutcome::WrongScope)
    );
    assert_eq!(parse("").rejection(STATUS), None);
    assert_eq!(parse("   ").rejection(CONVERSATION), None);
}

#[test]
fn unknown_words_are_rejected_on_both_surfaces_with_their_word() {
    let command = parse("/nope");
    for surface in [CONVERSATION, STATUS] {
        assert_eq!(
            command.rejection(surface),
            Some(CommandOutcome::Unsupported)
        );
    }
    assert_eq!(
        command_outcome_text(CommandOutcome::Unsupported, &command),
        "Unknown command: /nope"
    );
}

#[test]
fn me_is_conversation_scoped_and_the_rest_run_anywhere() {
    let me = parse("/me hi");
    assert_eq!(me.rejection(CONVERSATION), None);
    assert_eq!(me.rejection(STATUS), Some(CommandOutcome::WrongScope));
    for line in [
        "/clear", "/help", "/pref", "/version", "/list", "/join #x", "/part",
    ] {
        for surface in [CONVERSATION, STATUS] {
            assert_eq!(
                parse(line).rejection(surface),
                None,
                "{line} on {surface:?}"
            );
        }
    }
}

#[test]
fn words_split_at_the_first_space_only() {
    assert_eq!(Words::split(""), Words::default());
    assert_eq!(Words::split("a"), words("a", ""));
    assert_eq!(Words::split("a  b  c"), words("a", "b  c"));
    assert_eq!(Words::split("a\tb"), words("a\tb", ""));
}

#[test]
fn a_verb_argument_kind_follows_its_dispatch() {
    // The whole-argument verbs keep their text; the first-word verbs split it.
    assert!(matches!(parse("/away a b"), Command::Away(text) if text == "a b"));
    assert!(matches!(parse("/mute a b"), Command::Mute(w) if w == words("a", "b")));
}
