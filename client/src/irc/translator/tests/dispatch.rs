//! The command table and the lines the translator does not model.

use super::*;

#[test]
fn numeric_and_command_table_yields_the_expected_first_kind() {
    let cases: [(&str, &str, EventKind); 23] = [
        (
            "self away",
            ":server 306 omairc :marked away",
            EventKind::SelfAway,
        ),
        ("self back", ":server 305 omairc :back", EventKind::SelfAway),
        (
            "whois away",
            ":server 352 omairc #room user host server Alice G :0 Alice",
            EventKind::Away,
        ),
        (
            "whois account",
            ":server 330 omairc Alice whoisacct :is logged in as",
            EventKind::Account,
        ),
        (
            "channel topic reply",
            ":server 332 omairc #room :topic",
            EventKind::Topic,
        ),
        (
            "topic who time",
            ":server 333 omairc #room alice!u@h 1710000000",
            EventKind::Topic,
        ),
        (
            "names reply",
            ":server 353 omairc = #room :@Alice +Bob",
            EventKind::Names,
        ),
        (
            "end of names",
            ":server 366 omairc #room :End of /NAMES list.",
            EventKind::Names,
        ),
        ("mode", ":op!u@h MODE #room +o alice", EventKind::Mode),
        ("topic", ":op!u@h TOPIC #room :topic", EventKind::Topic),
        ("part", ":alice!u@h PART #room :bye", EventKind::Part),
        ("quit", ":alice!u@h QUIT :gone", EventKind::Quit),
        ("nick", ":alice!u@h NICK Alicia", EventKind::Nick),
        ("kick", ":op!u@h KICK #room bob :bye", EventKind::Kick),
        ("account", ":alice!u@h ACCOUNT services", EventKind::Account),
        ("away", ":alice!u@h AWAY :lunch", EventKind::Away),
        (
            "metadata 761",
            ":server 761 omairc Alice status priv :writing docs",
            EventKind::MemberMetadata,
        ),
        (
            "metadata 766",
            ":server 766 omairc Alice status :Key not set",
            EventKind::MemberMetadata,
        ),
        (
            "join failure 471",
            ":server 471 omairc #room :Cannot join channel (+l)",
            EventKind::ChannelError,
        ),
        (
            "join failure 473",
            ":server 473 omairc #room :Invite only",
            EventKind::ChannelError,
        ),
        (
            "ambiguous join failure 437",
            ":server 437 omairc #room :unavailable",
            EventKind::ChannelError,
        ),
        (
            "whois user",
            ":server 311 omairc Alice user host * :Alice Example",
            EventKind::Away,
        ),
        (
            "setname",
            ":alice!u@h SETNAME :Alice Lee",
            EventKind::NickFacts,
        ),
    ];
    for (name, line, want) in cases {
        let events = translate_line(line);
        assert!(
            !events.is_empty(),
            "{name}: Translate({line:?}) produced no events"
        );
        assert_eq!(events[0].kind(), want, "{name}");
    }
}

#[test]
fn lines_the_translator_does_not_model_produce_nothing() {
    // An unknown numeric has no handler.
    assert!(translate_line(":server 999 omairc :nope").is_empty());
}

#[test]
fn invite_to_self_emits_no_event() {
    // The invite to us stays a Status and inbox matter, not a transcript event.
    assert!(translate_line(":op!u@h INVITE omairc #room").is_empty());
    assert!(translate_line(":op!u@h INVITE omairc :#room").is_empty());
}

#[test]
fn invite_to_other_emits_invite_event() {
    match translate_line(":op!u@h INVITE bob #room").as_slice() {
        [Event::Invite(invite)] => {
            assert_eq!(invite.network_id, NETWORK_A);
            assert_eq!(invite.channel, "#room");
            assert_eq!(invite.inviter, "op");
            assert_eq!(invite.target, "bob");
        }
        other => panic!("want one InviteEvent, got {other:?}"),
    }
}

#[test]
fn invite_to_other_nick_case_mapped_self_is_self() {
    let translate_as = |nick: &str, line: &str| {
        translate(
            NETWORK_A,
            nick,
            &features(),
            &must_parse(line),
            reducer_timestamp(),
            None,
        )
    };
    assert!(translate_as("me", ":op!u@h INVITE ME #room").is_empty(), "ASCII case");
    assert!(
        translate_as("a[b", ":op!u@h INVITE A{B #room").is_empty(),
        "rfc1459 brackets fold to braces"
    );
    assert_eq!(translate_as("me", ":op!u@h INVITE mee #room").len(), 1);
}

#[test]
fn invite_without_channel_emits_nothing() {
    assert!(translate_line(":op!u@h INVITE bob").is_empty());
    assert!(translate_line(":op!u@h INVITE bob notachannel").is_empty());
    assert!(translate_line("INVITE bob #room").is_empty(), "no inviter");
}
