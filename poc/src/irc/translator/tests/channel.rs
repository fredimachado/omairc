//! Join, account and NAMES facts on a channel.

use super::*;

#[test]
fn extended_join_carries_account_and_realname_and_classic_join_does_not() {
    let events = translate_line(":alice!u@h JOIN #room account :Alice Example");
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::Join(join) => {
            assert_eq!(join.channel, "#room");
            assert_eq!(join.nick, "alice");
            assert_eq!(join.account.as_deref(), Some("account"));
            assert_eq!(join.realname.as_deref(), Some("Alice Example"));
        }
        other => panic!("want JoinEvent, got {other:?}"),
    }

    match translate_line(":alice!u@h JOIN #room").first() {
        Some(Event::Join(join)) => {
            assert!(
                join.account.is_none(),
                "a classic join must not clear an account"
            );
            assert!(join.realname.is_none());
        }
        other => panic!("want JoinEvent, got {other:?}"),
    }
}

#[test]
fn names_tokens_split_ranks_and_end_of_names_completes() {
    let events = translate_line(":server 353 omairc = #room :@Alice +Bob");
    match &events[0] {
        Event::Names(names) => {
            assert_eq!(names.channel, "#room");
            assert!(!names.complete);
            assert_eq!(names.names.len(), 2);
            assert_eq!(names.names[0].nick, "Alice");
            assert_eq!(names.names[0].ranks, parsed_name("@Alice").ranks);
            assert_eq!(names.names[1].nick, "Bob");
            assert_eq!(names.names[1].ranks, parsed_name("+Bob").ranks);
        }
        other => panic!("want NamesEvent, got {other:?}"),
    }

    match translate_line(":server 366 omairc #room :End of /NAMES list.").first() {
        Some(Event::Names(end)) => {
            assert_eq!(end.channel, "#room");
            assert!(end.complete);
            assert!(end.names.is_empty());
        }
        other => panic!("want NamesEvent, got {other:?}"),
    }
}

#[test]
fn account_tag_is_present_for_a_logout_and_a_valueless_tag_but_not_absent() {
    let events = translate_line(":Alice!u@h PRIVMSG #room :hi");
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], Event::Message(_)));

    let events = translate_line("@account=services :Alice!u@h PRIVMSG #room :hi");
    assert_eq!(events.len(), 2);
    match &events[0] {
        Event::Account(account) => assert_eq!(account.account, "services"),
        other => panic!("want AccountEvent, got {other:?}"),
    }
    assert!(matches!(events[1], Event::Message(_)));

    match translate_line("@account=* :Alice!u@h PRIVMSG #room :hi").first() {
        Some(Event::Account(account)) => assert_eq!(account.account, "*", "a logout"),
        other => panic!("want AccountEvent, got {other:?}"),
    }

    let events = translate_line("@account :Alice!u@h PRIVMSG #room :hi");
    assert_eq!(events.len(), 2);
    match &events[0] {
        Event::Account(account) => assert_eq!(account.account, "", "a valueless account"),
        other => panic!("want AccountEvent, got {other:?}"),
    }
}
