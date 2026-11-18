//! WHO, WHOIS, METADATA and join-failure numerics.

use super::*;

#[test]
fn away_numerics_set_clear_and_fill_the_facts_each_one_owns() {
    match translate_line(":server 306 omairc :You have been marked as being away").first() {
        Some(Event::SelfAway(self_away)) => assert!(self_away.away),
        other => panic!("want SelfAwayEvent, got {other:?}"),
    }
    match translate_line(":server 305 omairc :You are no longer away").first() {
        Some(Event::SelfAway(self_away)) => assert!(!self_away.away),
        other => panic!("want SelfAwayEvent, got {other:?}"),
    }

    // 352 with G: away, realname "Alice", and no operator flag on the away event.
    let events = translate_line(":server 352 omairc #room user host server Alice G :0 Alice");
    match &events[0] {
        Event::Away(away) => {
            assert_eq!(away.nick, "Alice");
            assert!(away.away.is_some());
            assert_eq!(away.realname.as_deref(), Some("Alice"));
            assert!(!away.realname_only);
        }
        other => panic!("want AwayEvent, got {other:?}"),
    }

    // 352 with H: here, so no away value.
    match translate_line(":server 352 omairc #room user host server Alice H :0 Alice").first() {
        Some(Event::Away(away)) => assert!(away.away.is_none()),
        other => panic!("want AwayEvent, got {other:?}"),
    }

    // 352 with H*: operator, and the realname stays on the away event.
    let events = translate_line(":server 352 omairc #room user host server Alice H* :0 Alice Vale");
    assert_eq!(events.len(), 2);
    match (&events[0], &events[1]) {
        (Event::Away(away), Event::NickFacts(facts)) => {
            assert_eq!(away.realname.as_deref(), Some("Alice Vale"));
            assert!(away.away.is_none());
            assert!(
                facts.realname.is_none(),
                "the realname stays on the away event"
            );
            assert_eq!(facts.server_operator, Some(true));
        }
        other => panic!("want AwayEvent then NickFactsEvent, got {other:?}"),
    }

    // 311 is a WHOIS real name only: it must not change away.
    match translate_line(":server 311 omairc Alice user host * :Alice Example").first() {
        Some(Event::Away(away)) => {
            assert!(away.realname_only);
            assert!(away.away.is_none());
            assert_eq!(away.realname.as_deref(), Some("Alice Example"));
        }
        other => panic!("want AwayEvent, got {other:?}"),
    }

    // 313 sets operator true and leaves the realname alone.
    let events = translate_line(":server 313 omairc Alice :is an IRC operator");
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::NickFacts(facts) => {
            assert!(facts.realname.is_none());
            assert_eq!(facts.server_operator, Some(true));
        }
        other => panic!("want NickFactsEvent, got {other:?}"),
    }

    match translate_line(":alice!u@h AWAY :lunch").first() {
        Some(Event::Away(away)) => {
            assert_eq!(away.away.as_ref().map(|a| a.reason.as_str()), Some("lunch"))
        }
        other => panic!("want AwayEvent, got {other:?}"),
    }
    match translate_line(":alice!u@h AWAY").first() {
        Some(Event::Away(away)) => assert!(away.away.is_none(), "a bare AWAY clears away"),
        other => panic!("want AwayEvent, got {other:?}"),
    }
}

#[test]
fn who_reply_real_names_follow_the_hop_count_rules() {
    // A signed hop count is still a hop count, as Go's Atoi reads it.
    match translate_line(":server 352 omairc #room user host server Alice H :+1 Alice Vale").first()
    {
        Some(Event::Away(away)) => assert_eq!(away.realname.as_deref(), Some("Alice Vale")),
        other => panic!("want AwayEvent, got {other:?}"),
    }
    // A lone hop count is no real name at all.
    match translate_line(":server 352 omairc #room user host server Alice H :0").first() {
        Some(Event::Away(away)) => assert!(away.realname.is_none()),
        other => panic!("want AwayEvent, got {other:?}"),
    }
    // A trailing value with no hop count is the name itself.
    match translate_line(":server 352 omairc #room user host server Alice H :Alice Vale").first() {
        Some(Event::Away(away)) => assert_eq!(away.realname.as_deref(), Some("Alice Vale")),
        other => panic!("want AwayEvent, got {other:?}"),
    }
}

#[test]
fn metadata_numerics_read_the_known_keys_and_drop_the_rest() {
    match translate_line(":server 761 omairc Alice status priv :writing docs").first() {
        Some(Event::MemberMetadata(metadata)) => {
            assert_eq!(metadata.nick, "Alice");
            assert_eq!(metadata.key, "status");
            assert_eq!(metadata.value, "writing docs");
        }
        other => panic!("want MemberMetadataEvent, got {other:?}"),
    }
    match translate_line(":server 766 omairc Alice status :Key not set").first() {
        Some(Event::MemberMetadata(metadata)) => {
            assert_eq!(metadata.nick, "Alice");
            assert_eq!(metadata.key, "status");
            assert_eq!(metadata.value, "", "766 clears the key");
        }
        other => panic!("want MemberMetadataEvent, got {other:?}"),
    }
    assert!(translate_line(":server 761 omairc Alice unknown priv :value").is_empty());
    assert!(translate_line(":server 761 omairc #room status priv :value").is_empty());
    assert!(translate_line(":server 766 omairc Alice unknown :gone").is_empty());
}

#[test]
fn join_failure_numerics_need_a_channel_when_they_are_ambiguous() {
    match translate_line(":server 471 omairc #room :Cannot join channel (+l)").first() {
        Some(Event::ChannelError(error)) => {
            assert_eq!(error.channel, "#room");
            assert_eq!(error.body, "Cannot join channel (+l)");
        }
        other => panic!("want ChannelErrorEvent, got {other:?}"),
    }
    match translate_line(":server 437 omairc #room :unavailable").first() {
        Some(Event::ChannelError(error)) => assert_eq!(error.channel, "#room"),
        other => panic!("want ChannelErrorEvent, got {other:?}"),
    }
    assert!(translate_line(":server 437 omairc someNick :unavailable").is_empty());
}

#[test]
fn setname_translates_to_realname_fact() {
    let events = translate_line(":alice!u@h SETNAME :Alice Lee");
    assert_eq!(events.len(), 1, "{events:?}");
    match &events[0] {
        Event::NickFacts(facts) => {
            assert_eq!(facts.nick, "alice");
            assert_eq!(facts.realname.as_deref(), Some("Alice Lee"));
            assert_eq!(facts.server_operator, None);
        }
        other => panic!("want NickFactsEvent, got {other:?}"),
    }
}

#[test]
fn setname_with_empty_param_clears_realname() {
    for line in [":alice!u@h SETNAME :", ":alice!u@h SETNAME"] {
        match translate_line(line).as_slice() {
            [Event::NickFacts(facts)] => assert_eq!(facts.realname.as_deref(), Some(""), "{line}"),
            other => panic!("{line}: want one NickFactsEvent, got {other:?}"),
        }
    }
}

#[test]
fn setname_without_prefix_is_ignored() {
    assert!(translate_line("SETNAME :Alice Lee").is_empty());
}

fn userhost_facts(events: &[Event]) -> Vec<(String, String)> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::NickFacts(facts) => facts
                .userhost
                .clone()
                .map(|userhost| (facts.nick.clone(), userhost)),
            _ => None,
        })
        .collect()
}

#[test]
fn names_with_userhost_yield_correct_nicks_and_userhost_facts() {
    let events = translate_line(":server 353 omairc = #room :@+alice!a@h.example bob!b@h2 carol");
    match &events[0] {
        Event::Names(names) => {
            let nicks: Vec<_> = names.names.iter().map(|name| name.nick.as_str()).collect();
            assert_eq!(nicks, ["alice", "bob", "carol"], "the userhost never leaks into a nick");
            assert_eq!(names.names[0], parsed_name("@+alice"));
        }
        other => panic!("want NamesEvent first, got {other:?}"),
    }
    assert_eq!(
        userhost_facts(&events),
        [("alice".to_owned(), "a@h.example".to_owned()), ("bob".to_owned(), "b@h2".to_owned())],
        "a token without a userhost adds no fact"
    );
    let plain = translate_line(":server 353 omairc = #room :@Alice +Bob");
    assert_eq!(plain.len(), 1, "no userhost, no extra event");
}

#[test]
fn join_carries_the_prefix_userhost_as_a_fact() {
    let events = translate_line(":alice!u@h.example JOIN #room");
    assert!(matches!(events[0], Event::Join(_)));
    assert_eq!(userhost_facts(&events), [("alice".to_owned(), "u@h.example".to_owned())]);
    assert!(userhost_facts(&translate_line(":server JOIN #room")).is_empty(), "a server prefix has no userhost");
}

#[test]
fn who_352_sets_userhost() {
    let events = translate_line(":server 352 omairc #room user host.example server Alice H :0 Alice");
    assert_eq!(userhost_facts(&events), [("Alice".to_owned(), "user@host.example".to_owned())]);
}

#[test]
fn chghost_translates_to_a_userhost_fact() {
    let events = translate_line(":alice!u@h CHGHOST newuser new.host");
    assert_eq!(events.len(), 1);
    assert_eq!(userhost_facts(&events), [("alice".to_owned(), "newuser@new.host".to_owned())]);
    assert!(translate_line(":alice!u@h CHGHOST onlyone").is_empty());
    assert!(translate_line("CHGHOST a b").is_empty(), "no sender");
}
