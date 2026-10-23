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
