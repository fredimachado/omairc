//! Away, self-away, member metadata, bots, accounts and realnames, WHO and WHOIS
//! facts, and the query-header labels. Ports of `reducer_test.go`.

use super::*;
use crate::irc::event::SelfAwayEvent;
use crate::irc::presence::{Away, AVATAR_KEY, BOT_KEY, DISPLAY_NAME_KEY};
use crate::irc::reducer::PeerPresence;

fn self_away_event(network_id: &str, away: bool) -> SelfAwayEvent {
    SelfAwayEvent {
        network_id: network_id.to_owned(),
        away,
    }
}

/// Applies one wire line and returns nothing; the reducer state carries the result.
fn wire(reducer: &mut EventReducer, line: &str) {
    apply_wire(reducer, line);
}

#[test]
fn away_is_one_fact_visible_in_every_channel() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let omarchy = reducer.conversation_key(NETWORK_A, "#omarchy");
    let desktop = reducer.conversation_key(NETWORK_A, "#desktop");
    fold(&mut reducer, join(NETWORK_A, "#omarchy", "Alice"));
    fold(&mut reducer, join(NETWORK_A, "#desktop", "Alice"));

    fold(&mut reducer, away_event(NETWORK_A, "alice", away("lunch")));
    fold(&mut reducer, away_event(NETWORK_A, "alice", away("lunch")));
    assert!(reducer.member_view(&omarchy, "alice").expect("alice").is_away(), "omarchy away");
    assert!(reducer.member_view(&desktop, "alice").expect("alice").is_away(), "desktop away");
    let reason = reducer.member_view(&omarchy, "alice").expect("alice").away.expect("away");
    assert_eq!(reason.reason, "lunch", "away reason");

    fold(&mut reducer, nick_change(NETWORK_A, "Alice", "Alicia"));
    assert!(reducer.member_view(&omarchy, "alicia").expect("alicia").is_away(), "omarchy away after nick");
    assert!(reducer.member_view(&desktop, "alicia").expect("alicia").is_away(), "desktop away after nick");

    fold(&mut reducer, away_event(NETWORK_A, "Alicia", None));
    assert!(!reducer.member_view(&omarchy, "alicia").expect("alicia").is_away(), "omarchy back");
    assert!(!reducer.member_view(&desktop, "alicia").expect("alicia").is_away(), "desktop back");
}

#[test]
fn metadata_status_is_separate_from_prefix_modes() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, names(NETWORK_A, "#room", &["@Alice"], true));
    fold(&mut reducer, metadata(NETWORK_A, "Alice", "status", "writing docs"));

    let member = reducer.member_view(&room, "alice").expect("alice");
    assert_eq!(member.status, "writing docs", "status");
    assert_eq!(member.label, "@Alice", "label");
    assert_eq!(member.ranks, parsed_name("@Alice").ranks, "ranks");
    assert!(!member.is_away(), "away");

    fold(&mut reducer, metadata(NETWORK_A, "Alice", "status", ""));
    assert_eq!(reducer.member_view(&room, "alice").expect("alice").status, "", "cleared status");
}

#[test]
fn metadata_keys_store_independently_of_away_and_prefix() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, names(NETWORK_A, "#room", &["@Alice"], true));
    fold(&mut reducer, away_event(NETWORK_A, "Alice", away("lunch")));
    fold(&mut reducer, metadata(NETWORK_A, "Alice", AVATAR_KEY, "https://example.com/a.png"));
    fold(&mut reducer, metadata(NETWORK_A, "Alice", BOT_KEY, "PacketBot"));
    fold(&mut reducer, metadata(NETWORK_A, "Alice", DISPLAY_NAME_KEY, "Anna Docs"));
    fold(&mut reducer, metadata(NETWORK_A, "Alice", "status", "writing docs"));

    let member = reducer.member_view(&room, "alice").expect("alice");
    assert!(member.is_away(), "away");
    assert_eq!(member.label, "@Alice", "label");
    assert_eq!(member.status, "writing docs", "status");
    assert_eq!(member.avatar, "https://example.com/a.png", "avatar");
    assert!(member.bot, "bot");
    assert_eq!(member.display_name, "Anna Docs", "displayName");

    let facts = reducer.nick_presence(NETWORK_A, "ALICE");
    assert!(facts.away.is_some(), "facts away");
    assert_eq!(facts.avatar(), "https://example.com/a.png", "facts avatar");
    assert!(facts.is_bot(), "facts bot");
    assert_eq!(facts.metadata(DISPLAY_NAME_KEY), "Anna Docs", "facts display-name");

    fold(&mut reducer, metadata(NETWORK_A, "Alice", BOT_KEY, ""));
    let after_clear = reducer.member_view(&room, "alice").expect("alice");
    assert!(!after_clear.bot, "bot after clear");
    assert_eq!(after_clear.status, "writing docs", "status after clear");
    assert_eq!(after_clear.avatar, "https://example.com/a.png", "avatar after clear");
    assert_eq!(after_clear.display_name, "Anna Docs", "displayName after clear");
    assert!(after_clear.is_away(), "away after clear");
    assert_eq!(after_clear.label, "@Alice", "label after clear");
}

#[test]
fn presence_is_dropped_with_the_last_channel_and_on_welcome() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let omarchy = reducer.conversation_key(NETWORK_A, "#omarchy");
    let desktop = reducer.conversation_key(NETWORK_A, "#desktop");
    fold(&mut reducer, join(NETWORK_A, "#omarchy", "Alice"));
    fold(&mut reducer, join(NETWORK_A, "#desktop", "Alice"));
    fold(&mut reducer, away_event(NETWORK_A, "Alice", away("lunch")));

    fold(&mut reducer, part(NETWORK_A, "#desktop", "Alice"));
    assert!(reducer.member_view(&desktop, "alice").is_none(), "desktop must not hold alice after the part");
    assert!(reducer.member_view(&omarchy, "alice").expect("alice").is_away(), "omarchy away");

    fold(&mut reducer, quit(NETWORK_A, "Alice"));
    fold(&mut reducer, join(NETWORK_A, "#omarchy", "Alice"));
    assert!(!reducer.member_view(&omarchy, "alice").expect("alice").is_away(), "away forgotten on quit");

    fold(&mut reducer, away_event(NETWORK_A, "Alice", away("lunch")));
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#omarchy", "Alice"));
    assert!(!reducer.member_view(&omarchy, "alice").expect("alice").is_away(), "away cleared on welcome");
}

#[test]
fn losing_a_capability_clears_the_facts_it_fed() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, away_event(NETWORK_A, "Alice", away("lunch")));
    fold(&mut reducer, metadata(NETWORK_A, "Alice", "status", "writing docs"));

    reducer.clear_presence_facts(NETWORK_A, true, false);
    assert!(!reducer.member_view(&room, "alice").expect("alice").is_away(), "away cleared");
    assert_eq!(reducer.member_view(&room, "alice").expect("alice").status, "writing docs", "status kept");

    reducer.clear_presence_facts(NETWORK_A, false, true);
    assert_eq!(reducer.member_view(&room, "alice").expect("alice").status, "", "status cleared");
}

#[test]
fn self_away_is_network_membership_not_member_presence() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let omarchy = reducer.conversation_key(NETWORK_A, "#omarchy");
    fold(&mut reducer, join(NETWORK_A, "#omarchy", "omairc"));
    fold(&mut reducer, join(NETWORK_A, "#desktop", "omairc"));

    fold(&mut reducer, away_event(NETWORK_A, "omairc", Some(Away::default())));
    assert!(!reducer.self_away(NETWORK_A), "selfAway from away-notify");
    assert!(reducer.member_view(&omarchy, "omairc").expect("omairc").is_away(), "member row away");

    fold(&mut reducer, self_away_event(NETWORK_A, true));
    fold(&mut reducer, self_away_event(NETWORK_A, true));
    assert!(reducer.self_away(NETWORK_A), "selfAway set");

    fold(&mut reducer, part(NETWORK_A, "#omarchy", "omairc"));
    fold(&mut reducer, part(NETWORK_A, "#desktop", "omairc"));
    assert!(reducer.self_away(NETWORK_A), "selfAway survives parts");

    reducer.clear_presence_facts(NETWORK_A, true, true);
    assert!(reducer.self_away(NETWORK_A), "selfAway survives presence clear");

    fold(&mut reducer, self_away_event(NETWORK_A, false));
    fold(&mut reducer, self_away_event(NETWORK_A, false));
    assert!(!reducer.self_away(NETWORK_A), "selfAway cleared");

    fold(&mut reducer, self_away_event(NETWORK_A, true));
    welcome_omairc(&mut reducer, NETWORK_A);
    assert!(!reducer.self_away(NETWORK_A), "selfAway reset on welcome");
    assert!(!reducer.self_away(NETWORK_B), "other network selfAway");
}

#[test]
fn self_away_shows_on_our_own_row_in_every_channel() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let omarchy = reducer.conversation_key(NETWORK_A, "#omarchy");
    let desktop = reducer.conversation_key(NETWORK_A, "#desktop");
    fold(&mut reducer, join(NETWORK_A, "#omarchy", "omairc"));
    fold(&mut reducer, join(NETWORK_A, "#desktop", "omairc"));
    fold(&mut reducer, join(NETWORK_A, "#omarchy", "Alice"));

    assert!(!reducer.member_view(&omarchy, "omairc").expect("omairc").is_away(), "omarchy self away");
    assert!(!reducer.member_view(&desktop, "omairc").expect("omairc").is_away(), "desktop self away");

    fold(&mut reducer, self_away_event(NETWORK_A, true));
    assert!(reducer.self_away(NETWORK_A), "selfAway");
    assert!(reducer.member_view(&omarchy, "omairc").expect("omairc").is_away(), "omarchy self away after");
    assert!(reducer.member_view(&desktop, "omairc").expect("omairc").is_away(), "desktop self away after");
    assert!(!reducer.member_view(&omarchy, "alice").expect("alice").is_away(), "peer away");

    // The overlay reads the network-level fact, so losing member presence keeps it.
    reducer.clear_presence_facts(NETWORK_A, true, true);
    assert!(reducer.member_view(&omarchy, "omairc").expect("omairc").is_away(), "self away after presence clear");

    fold(&mut reducer, self_away_event(NETWORK_A, false));
    assert!(!reducer.member_view(&omarchy, "omairc").expect("omairc").is_away(), "omarchy self back");
    assert!(!reducer.member_view(&desktop, "omairc").expect("omairc").is_away(), "desktop self back");
}

#[test]
fn peer_presence_follows_shared_channel_and_away_facts() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);

    assert_eq!(reducer.peer_presence(NETWORK_A, "alice"), PeerPresence::Unknown, "no shared channel");

    fold(&mut reducer, join(NETWORK_A, "#omarchy", "Alice"));
    assert_eq!(reducer.peer_presence(NETWORK_A, "alice"), PeerPresence::Online, "shared channel");

    fold(&mut reducer, away_event(NETWORK_A, "Alice", away("lunch")));
    assert_eq!(reducer.peer_presence(NETWORK_A, "alice"), PeerPresence::Away, "away");

    fold(&mut reducer, away_event(NETWORK_A, "Alice", None));
    assert_eq!(reducer.peer_presence(NETWORK_A, "alice"), PeerPresence::Online, "back");

    fold(&mut reducer, quit(NETWORK_A, "Alice"));
    assert_eq!(reducer.peer_presence(NETWORK_A, "alice"), PeerPresence::Unknown, "after quit");
    assert_eq!(reducer.peer_presence(NETWORK_B, "alice"), PeerPresence::Unknown, "other network");
}

#[test]
fn falsey_bot_values_are_not_bots() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, names(NETWORK_A, "#room", &["Alice"], true));

    for falsey in ["0", "false", "no"] {
        fold(&mut reducer, metadata(NETWORK_A, "Alice", BOT_KEY, falsey));
        assert!(!reducer.member_view(&room, "alice").expect("alice").bot, "bot {falsey}");
        assert!(!reducer.nick_presence(NETWORK_A, "Alice").is_bot(), "facts bot {falsey}");
        fold(&mut reducer, metadata(NETWORK_A, "Alice", BOT_KEY, ""));
    }

    fold(&mut reducer, metadata(NETWORK_A, "Alice", BOT_KEY, "PacketBot"));
    assert!(reducer.member_view(&room, "alice").expect("alice").bot, "real bot");
}

#[test]
fn extended_join_records_account_on_one_line() {
    // The wire form is enough. These caps do not have to be enabled.
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    wire(&mut reducer, ":Alice!a@h JOIN #room services :Alice Example");

    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").account, "services", "presence account");
    assert_eq!(reducer.display_account(NETWORK_A, "Alice"), "services", "display account");
    let room = room_of(&reducer, "#room").expect("room present");
    assert_eq!(room.messages.len(), 1, "messages");
    assert_eq!(room.messages[0].body, "Alice (services) joined", "join body");
    let key = room.key.clone();
    assert_eq!(reducer.member_view(&key, "alice").expect("alice").account, "services", "member account");

    wire(&mut reducer, ":Alice!a@h PRIVMSG #room :hi");
    wire(&mut reducer, ":Alice!a@h JOIN :#room");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").account, "services", "account kept");
    assert_eq!(last_body(&reducer, "#room"), "Alice joined", "classic join body");

    wire(&mut reducer, ":Alice!a@h PRIVMSG #room :again");
    wire(&mut reducer, ":Alice!a@h JOIN #room * :Alice Example");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").account, "", "account cleared");
    assert_eq!(reducer.display_account(NETWORK_A, "Alice"), "", "display cleared");
    assert_eq!(last_body(&reducer, "#room"), "Alice joined", "star join body");
}

#[test]
fn account_command_and_tag_share_one_field() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    wire(&mut reducer, ":Alice!a@h ACCOUNT services");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").account, "services", "account command");
    assert!(reducer.find(&reducer.conversation_key(NETWORK_A, "#room")).is_none(), "no channel invented");

    wire(&mut reducer, ":Alice!a@h ACCOUNT *");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").account, "", "account logout");
    assert!(reducer.nick_presence(NETWORK_A, "Alice").account_known, "logout is known");

    wire(&mut reducer, "@account=services :Alice!a@h PRIVMSG #room :one");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").account, "services", "account tag");
    wire(&mut reducer, ":Alice!a@h PRIVMSG #room :two");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").account, "services", "account tag sticky");
    wire(&mut reducer, "@account=* :Alice!a@h PRIVMSG #room :three");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").account, "", "account tag logout");

    wire(&mut reducer, "@account=services :Alice!a@h NOTICE #room :psst");
    let mut via_command = EventReducer::new();
    welcome_omairc(&mut via_command, NETWORK_A);
    wire(&mut via_command, ":Alice!a@h ACCOUNT services");
    assert_eq!(
        reducer.nick_presence(NETWORK_A, "Alice").account,
        via_command.nick_presence(NETWORK_A, "Alice").account,
        "notice account tag"
    );

    wire(&mut reducer, ":server 330 omairc Alice whoisacct :is logged in as");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").account, "whoisacct", "whois account");
    wire(&mut reducer, ":server 311 omairc Alice user host * :Alice Example");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").account, "whoisacct", "whois account kept");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").realname, "Alice Example", "whois realname");
    assert_eq!(reducer.meaningful_realname(NETWORK_A, "Alice"), "Alice Example", "whois meaningful");
}

#[test]
fn who_and_whois_store_realname_for_jump() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    wire(&mut reducer, ":Alice!a@h JOIN #room");
    wire(&mut reducer, ":Alice!a@h AWAY :lunch");
    wire(&mut reducer, ":server 352 omairc #room u h s Alice H :0 Alice");
    assert!(reducer.nick_presence(NETWORK_A, "Alice").away.is_none(), "an H WHO reply must clear away");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").realname, "Alice", "who realname");
    assert!(
        !crate::irc::presence::meaningful_realname(&reducer.nick_presence(NETWORK_A, "Alice").realname, "Alice"),
        "a real name equal to the nick must not be meaningful"
    );

    wire(&mut reducer, ":server 311 omairc Alice u h * :Alice Example");
    assert!(reducer.nick_presence(NETWORK_A, "Alice").away.is_none(), "WHOIS must not mark the nick away");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").realname, "Alice Example", "whois realname");

    wire(&mut reducer, ":Alice!a@h AWAY :back later");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").realname, "Alice Example", "away keeps realname");
    assert!(reducer.nick_presence(NETWORK_A, "Alice").away.is_some(), "AWAY must still record away");

    wire(&mut reducer, ":server 352 omairc #room u h s Alice G :2");
    assert!(reducer.nick_presence(NETWORK_A, "Alice").away.is_some(), "a G WHO reply must mark away");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").realname, "Alice Example", "hop-only who keeps realname");

    wire(&mut reducer, ":server 352 omairc #room u h s Alice G :0 Alice Example");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").realname, "Alice Example", "who stores example");
    wire(&mut reducer, ":server 311 omairc Alice u h * :unknown");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").realname, "Alice Example", "placeholder whois keeps example");

    wire(&mut reducer, ":Alice!a@h NICK Alicia");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alicia").realname, "Alice Example", "renamed realname");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").realname, "", "old nick realname");

    let mut joined = EventReducer::new();
    welcome_omairc(&mut joined, NETWORK_A);
    wire(&mut joined, ":Alice!a@h JOIN #room acct :Alice Example");
    assert_eq!(joined.nick_presence(NETWORK_A, "Alice").realname, "Alice Example", "join realname");
    assert_eq!(joined.nick_presence(NETWORK_A, "Alice").account, "acct", "join account");
    assert!(joined.nick_presence(NETWORK_A, "Alice").away.is_none(), "extended-join must not mark Alice away");
    assert!(crate::irc::presence::meaningful_realname("Alice Example", "Alice"), "Alice Example must stay meaningful");
    wire(&mut joined, ":Alice!a@h JOIN :#room");
    assert_eq!(joined.nick_presence(NETWORK_A, "Alice").realname, "Alice Example", "classic join keeps realname");
    assert_eq!(joined.nick_presence(NETWORK_A, "Alice").account, "acct", "classic join keeps account");
    assert!(joined.nick_presence(NETWORK_A, "Alice").away.is_none(), "a classic JOIN must not mark Alice away");
}

#[test]
fn nick_facts_feed_query_header() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    wire(&mut reducer, ":Alice!a@h JOIN #room");
    assert_eq!(reducer.peer_presence(NETWORK_A, "alice"), PeerPresence::Online, "alice online");

    wire(&mut reducer, ":server 352 omairc #room u h server Alice H* :0 Alice Example");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").realname, "Alice Example", "who realname");
    assert!(reducer.nick_presence(NETWORK_A, "Alice").server_operator, "who operator");
    assert_eq!(reducer.meaningful_realname(NETWORK_A, "Alice"), "Alice Example", "who meaningful");
    assert_eq!(reducer.peer_fact_labels(NETWORK_A, "Alice"), vec!["server operator"], "labels");

    // A gecos equal to the nick is not a name. It must not erase Alice Example. The
    // operator flag still follows this WHO reply.
    wire(&mut reducer, ":server 352 omairc #room u h server Alice G :0 Alice");
    assert_eq!(reducer.meaningful_realname(NETWORK_A, "Alice"), "Alice Example", "placeholder keeps name");
    assert!(!reducer.nick_presence(NETWORK_A, "Alice").server_operator, "operator cleared");
    assert_eq!(reducer.peer_presence(NETWORK_A, "alice"), PeerPresence::Away, "alice away");

    wire(&mut reducer, ":server 311 omairc Alice user host * :unknown");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").realname, "Alice Example", "placeholder whois keeps name");
    wire(&mut reducer, ":server 311 omairc Alice user host * :A Custom Name");
    assert_eq!(reducer.meaningful_realname(NETWORK_A, "Alice"), "A Custom Name", "custom name");

    wire(&mut reducer, ":server 313 omairc Alice :is an IRC operator");
    assert!(reducer.nick_presence(NETWORK_A, "Alice").server_operator, "whois operator");
    assert_eq!(reducer.meaningful_realname(NETWORK_A, "Alice"), "A Custom Name", "whois keeps name");

    wire(&mut reducer, ":Alice!a@h ACCOUNT *");
    assert!(reducer.nick_presence(NETWORK_A, "Alice").account_known, "logout known");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").account, "", "logout account");
    assert_eq!(reducer.peer_fact_labels(NETWORK_A, "Alice"), vec!["unauthenticated", "server operator"], "labels");

    wire(&mut reducer, ":server 761 omairc Alice bot * :Helper");
    assert_eq!(
        reducer.peer_fact_labels(NETWORK_A, "Alice"),
        vec!["unauthenticated", "server operator", "bot"],
        "labels"
    );

    wire(&mut reducer, ":Alice!a@h ACCOUNT Alice");
    assert_eq!(reducer.peer_fact_labels(NETWORK_A, "Alice"), vec!["server operator", "bot"], "labels");
    assert_eq!(reducer.display_account(NETWORK_A, "Alice"), "", "account matches nick");

    wire(&mut reducer, ":Bob!b@h JOIN #room");
    assert!(reducer.peer_fact_labels(NETWORK_A, "Bob").is_empty(), "bob has no labels");
    assert!(!reducer.nick_presence(NETWORK_A, "Bob").account_known, "bob account unknown");
    wire(&mut reducer, ":server 311 omairc Bob user host * :realname");
    assert_eq!(reducer.meaningful_realname(NETWORK_A, "Bob"), "", "placeholder realname");
    wire(&mut reducer, ":server 311 omairc Bob user host * :Unknown");
    assert_eq!(reducer.meaningful_realname(NETWORK_A, "Bob"), "", "placeholder unknown");
    wire(&mut reducer, ":server 311 omairc Bob user host * :FullName");
    assert_eq!(reducer.meaningful_realname(NETWORK_A, "Bob"), "", "placeholder fullname");

    wire(&mut reducer, ":server 311 omairc Carol u h * :Carol Example");
    assert_eq!(reducer.meaningful_realname(NETWORK_A, "Carol"), "Carol Example", "offline realname");
    assert_eq!(reducer.peer_presence(NETWORK_A, "carol"), PeerPresence::Unknown, "carol offline");
}

#[test]
fn nick_change_keeps_services_account() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    wire(&mut reducer, ":Alice!a@h JOIN #room services :Alice");
    wire(&mut reducer, ":Alice!a@h NICK Alicia");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alicia").account, "services", "renamed account");
    assert_eq!(reducer.nick_presence(NETWORK_A, "Alice").account, "", "old nick account");
    assert_eq!(reducer.display_account(NETWORK_A, "Alicia"), "services", "renamed display account");

    let mut same = EventReducer::new();
    welcome_omairc(&mut same, NETWORK_A);
    wire(&mut same, ":Bob!b@h JOIN #room bob :Bob");
    assert_eq!(last_body(&same, "#room"), "Bob joined", "bob join body");
    assert_eq!(same.nick_presence(NETWORK_A, "Bob").account, "bob", "bob account");
    assert_eq!(same.display_account(NETWORK_A, "Bob"), "", "bob display account");
    let room = room_of(&same, "#room").expect("room present").key.clone();
    assert_eq!(same.member_view(&room, "bob").expect("bob").account, "", "bob member account");
    wire(&mut same, ":Bob!b@h NICK Robert");
    assert_eq!(same.nick_presence(NETWORK_A, "Robert").account, "bob", "robert account");
    assert_eq!(same.display_account(NETWORK_A, "Robert"), "bob", "robert display account");

    // Under the default RFC 1459 mapping, `a{b` is the same nick as `a[b`.
    let mut folded = EventReducer::new();
    welcome_omairc(&mut folded, NETWORK_A);
    wire(&mut folded, ":a[b!u@h JOIN #room a{b :name");
    assert_eq!(last_body(&folded, "#room"), "a[b joined", "folded join body");
    assert_eq!(folded.display_account(NETWORK_A, "a[b"), "", "folded display account");

    let mut ascii = EventReducer::new();
    welcome_omairc(&mut ascii, NETWORK_A);
    let mut features = IrcServerFeatures::new();
    features.apply_tokens(&["CASEMAPPING=ascii"]);
    ascii.set_server_features(NETWORK_A, features);
    wire(&mut ascii, ":a[b!u@h JOIN #room a{b :name");
    assert_eq!(last_body(&ascii, "#room"), "a[b (a{b) joined", "ascii join body");
    assert_eq!(ascii.display_account(NETWORK_A, "a[b"), "a{b", "ascii display account");
}
