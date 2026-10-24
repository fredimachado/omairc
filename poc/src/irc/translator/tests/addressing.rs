//! Which conversation a line belongs to, the time readers, and the helper edges.

use super::*;

#[test]
fn conversation_for_classifies_every_kind_of_target() {
    let features = features();
    let from_alice = must_parse(":alice!u@h PRIVMSG #room :hi");

    let channel = conversation_for(NETWORK_A, "#room", &from_alice, "omairc", &features);
    assert_eq!(channel, Some(Key::new(NETWORK_A, "#room")));

    let inbound = conversation_for(NETWORK_A, "omairc", &from_alice, "omairc", &features);
    assert_eq!(inbound, Some(Key::new(NETWORK_A, "alice")));

    let from_self = must_parse(":omairc!u@h PRIVMSG alice :hi");
    assert_eq!(
        conversation_for(NETWORK_A, "alice", &from_self, "omairc", &features),
        Some(Key::new(NETWORK_A, "alice"))
    );

    let service = must_parse(":NickServ!s@services PRIVMSG omairc :hi");
    assert!(
        conversation_for(NETWORK_A, "omairc", &service, "omairc", &features).is_none(),
        "a service identity must not resolve to a conversation"
    );

    let no_user = must_parse(":alice PRIVMSG omairc :hi");
    assert!(
        conversation_for(NETWORK_A, "omairc", &no_user, "omairc", &features).is_none(),
        "a user-less prefix must not resolve to a conversation"
    );

    let notice_target = must_parse(":alice!u@h PRIVMSG * :hi");
    assert!(
        conversation_for(NETWORK_A, "*", &notice_target, "omairc", &features).is_none(),
        "a network notice target must not resolve to a conversation"
    );

    let bouncer = must_parse(":*status!u@h PRIVMSG omairc :hi");
    assert!(
        conversation_for(NETWORK_A, "omairc", &bouncer, "omairc", &features).is_none(),
        "a non-routable bouncer nick must not resolve to a conversation"
    );
}

#[test]
fn the_time_tag_reader_takes_only_a_parseable_value() {
    assert_eq!(server_time_of(&must_parse(":a!u@h PRIVMSG #c :x")), None);
    assert_eq!(
        server_time_of(&must_parse("@time= :a!u@h PRIVMSG #c :x")),
        None
    );
    assert_eq!(
        server_time_of(&must_parse("@time=not-a-time :a!u@h PRIVMSG #c :x")),
        None
    );
    let want = Utc.with_ymd_and_hms(2024, 3, 9, 16, 0, 0).unwrap();
    assert_eq!(
        server_time_of(&must_parse(
            "@time=2024-03-09T16:00:00Z :a!u@h PRIVMSG #c :x"
        )),
        Some(want)
    );
    assert_eq!(
        server_time_of(&must_parse(
            "@time=2024-03-09T16:00:00.620Z :a!u@h PRIVMSG #c :x"
        )),
        Some(want + chrono::Duration::milliseconds(620))
    );
    assert_eq!(
        server_time_of(&must_parse(
            "@time=2024-03-09T16:00:00+0000 :a!u@h PRIVMSG #c :x"
        )),
        Some(want),
        "Qt accepts offsets without a colon; RFC3339 alone does not"
    );
}

#[test]
fn chathistory_targets_times_accept_the_prefix_or_the_bare_value() {
    let want = Some(Utc.with_ymd_and_hms(2024, 3, 9, 16, 0, 0).unwrap());
    assert_eq!(
        parse_history_target_time("timestamp=2024-03-09T16:00:00Z"),
        want
    );
    assert_eq!(parse_history_target_time("2024-03-09T16:00:00Z"), want);
    assert_eq!(
        parse_history_target_time("timestamp=2024-03-09T16:00:00+0000"),
        want
    );
    assert_eq!(parse_history_target_time("timestamp="), None);
    assert_eq!(parse_history_target_time(""), None);
    assert_eq!(parse_history_target_time("yesterday"), None);
}

#[test]
fn helper_edges_match_the_go_readers() {
    assert_eq!(
        features().case_mapping().normalize("#Room"),
        "#room",
        "channels key on the folded name"
    );
    assert!(!is_go_int(""));
    assert!(is_go_int("-3"));
    assert!(!all_digits("+1"));
    assert_eq!(action_text("\u{1}ACTION \u{1}"), Some(""));
    assert_eq!(action_text("\u{1}ACTION"), None);
}
