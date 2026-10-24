//! The command table and the lines the translator does not model.

use super::*;

#[test]
fn numeric_and_command_table_yields_the_expected_first_kind() {
    let cases: [(&str, &str, EventKind); 22] = [
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
    // CHGHOST is a no-op, and an unknown numeric has no handler.
    assert!(translate_line(":alice!u@h CHGHOST new host").is_empty());
    assert!(translate_line(":server 999 omairc :nope").is_empty());
}
