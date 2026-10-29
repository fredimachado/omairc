//! NAMES, joins, parts, quits, kicks, nicks and the membership collapse.
//! `reducer_test.go` and the membership-folding cases of the same file.

use super::*;
use crate::irc::event::HistoryKind;
use crate::irc::membershipnoise::MembershipNoise;
use crate::irc::world::MessageKind;

#[test]
fn names_fill_and_complete_without_duplicates() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, names(NETWORK_A, "#room", &["@Alice", "Bob"], false));
    fold(&mut reducer, names(NETWORK_A, "#room", &["@ALICE", "+Carol"], true));
    fold(&mut reducer, join(NETWORK_A, "#room", "carol"));

    let key = reducer.conversation_key(NETWORK_A, "#room");
    let conversation = state_of(&reducer, &key);
    assert!(conversation.is_channel(), "isChannel");
    assert_eq!(conversation.people_count(), 3, "peopleCount");
    assert!(!conversation.channel.as_ref().expect("a channel").names_syncing, "namesSyncing");
    let alice = &conversation.channel.as_ref().expect("a channel").members["alice"];
    assert_eq!(alice.ranks, parsed_name("@Alice").ranks, "alice ranks");
    let view = reducer.member_view(&key, "alice").expect("alice is a member");
    assert_eq!(view.label, "@ALICE", "alice label");
    assert_eq!(view.status, "", "alice status");
}

#[test]
fn nick_and_quit_stay_network_scoped() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    welcome_omairc(&mut reducer, NETWORK_B);
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, join(NETWORK_B, "#room", "Alice"));
    fold(&mut reducer, nick_change(NETWORK_A, "ALICE", "Alicia"));

    let channel_a = reducer.conversation_key(NETWORK_A, "#room");
    let channel_b = reducer.conversation_key(NETWORK_B, "#room");
    let members_a = &state_of(&reducer, &channel_a).channel.as_ref().expect("a channel").members;
    assert!(members_a.contains_key("alicia"), "channelA must hold alicia");
    assert!(!members_a.contains_key("alice"), "channelA must not hold alice");
    assert!(
        state_of(&reducer, &channel_b).channel.as_ref().expect("a channel").members.contains_key("alice"),
        "channelB must still hold alice"
    );

    fold(&mut reducer, QuitEvent {
        reason: "gone".to_owned(),
        ..quit(NETWORK_A, "ALICIA")
    });
    assert_eq!(state_of(&reducer, &channel_a).people_count(), 0, "channelA peopleCount");
    assert_eq!(state_of(&reducer, &channel_b).people_count(), 1, "channelB peopleCount");
}

#[test]
fn self_membership_controls_channel_lifecycle() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#one", "OMAIRC"));

    let one = reducer.conversation_key(NETWORK_A, "#one");
    assert!(state_of(&reducer, &one).channel.as_ref().expect("a channel").joined, "joined");

    fold(&mut reducer, part(NETWORK_A, "#one", "omairc"));
    assert!(!state_of(&reducer, &one).channel.as_ref().expect("a channel").joined, "joined after part");
    assert_eq!(state_of(&reducer, &one).people_count(), 0, "peopleCount after part");

    fold(&mut reducer, join(NETWORK_A, "#two", "omairc"));
    let two = reducer.conversation_key(NETWORK_A, "#two");
    fold(&mut reducer, kick(NETWORK_A, "#two", "OMAIRC", "operator", "bye"));
    assert!(!state_of(&reducer, &two).channel.as_ref().expect("a channel").joined, "joined after kick");
    assert_eq!(state_of(&reducer, &two).people_count(), 0, "peopleCount after kick");
}

#[test]
fn identical_channels_stay_isolated() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    welcome_omairc(&mut reducer, NETWORK_B);
    let key_a = reducer.conversation_key(NETWORK_A, "#same");
    let key_b = reducer.conversation_key(NETWORK_B, "#same");

    fold(&mut reducer, join(NETWORK_A, "#same", "Alice"));
    fold(&mut reducer, join(NETWORK_B, "#same", "Bob"));
    fold(&mut reducer, message(&key_a, "Alice", "only a", "#same"));
    fold(&mut reducer, message(&key_b, "Bob", "only b", "#same"));

    let channel_a = state_of(&reducer, &key_a);
    let channel_b = state_of(&reducer, &key_b);
    assert_eq!(channel_a.people_count(), 1, "channelA peopleCount");
    assert_eq!(channel_b.people_count(), 1, "channelB peopleCount");
    assert_eq!(channel_a.messages.last().expect("a row").body, "only a");
    assert_eq!(channel_b.messages.last().expect("a row").body, "only b");
}

#[test]
fn advertised_channel_types_create_channels() {
    let mut reducer = EventReducer::new();
    let mut features = IrcServerFeatures::new();
    features.apply_token("CHANTYPES=&");
    reducer.set_server_features(NETWORK_A, features);
    welcome_omairc(&mut reducer, NETWORK_A);

    fold(&mut reducer, join(NETWORK_A, "&local", "omairc"));
    let channel = state_of(&reducer, &reducer.conversation_key(NETWORK_A, "&local"));
    assert!(channel.is_channel(), "isChannel");
}

#[test]
fn welcome_resets_membership() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));

    let key = reducer.conversation_key(NETWORK_A, "#room");
    {
        let conversation = state_of(&reducer, &key);
        assert_eq!(conversation.people_count(), 2, "peopleCount");
        assert!(conversation.channel.as_ref().expect("a channel").joined, "joined");
        assert!(conversation.channel.as_ref().expect("a channel").history_anchor.is_some(), "historyAnchor");
    }

    welcome_omairc(&mut reducer, NETWORK_A);
    {
        let conversation = state_of(&reducer, &key);
        assert_eq!(conversation.people_count(), 0, "peopleCount after welcome");
        assert!(!conversation.channel.as_ref().expect("a channel").joined, "joined after welcome");
        assert!(conversation.channel.as_ref().expect("a channel").history_anchor.is_none(), "historyAnchor after welcome");
    }

    // A stale history line must not splice into the reset (unjoined) channel.
    fold(
        &mut reducer,
        history_event(&key, "#room", HistoryKind::Chat, false, vec![replay_line("alice", "stale", "id-welcome")]),
    );
    let conversation = state_of(&reducer, &key);
    assert_eq!(conversation.messages.len(), 2, "messages");
    assert_eq!(conversation.messages[0].body, "omairc joined");
    assert_eq!(conversation.messages[1].body, "Alice joined");
}

#[test]
fn mode_edits_existing_rows_only() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, names(NETWORK_A, "#room", &["Alice"], true));

    let label = |reducer: &EventReducer| reducer.member_view(&room, "alice").expect("alice").label;
    fold(&mut reducer, mode(NETWORK_A, "#room", "op", "+o", &["alice"]));
    assert_eq!(label(&reducer), "@Alice", "after +o");
    fold(&mut reducer, mode(NETWORK_A, "#room", "op", "+o", &["alice"]));
    assert_eq!(label(&reducer), "@Alice", "after repeat +o");
    fold(&mut reducer, mode(NETWORK_A, "#room", "op", "+v", &["alice"]));
    assert_eq!(label(&reducer), "@Alice", "after +v");
    fold(&mut reducer, mode(NETWORK_A, "#room", "op", "-o", &["alice"]));
    assert_eq!(label(&reducer), "+Alice", "after -o");
    fold(&mut reducer, mode(NETWORK_A, "#room", "op", "-v", &["alice"]));
    assert_eq!(label(&reducer), "Alice", "after -v");
    fold(&mut reducer, mode(NETWORK_A, "#room", "op", "+o", &["ghost"]));
    assert!(reducer.member_view(&room, "ghost").is_none(), "mode must not invent a member");
    assert_eq!(state_of(&reducer, &room).people_count(), 1, "peopleCount");
}

#[test]
fn join_of_listed_nick_keeps_ranks() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, names(NETWORK_A, "#room", &["@+Alice"], true));
    fold(&mut reducer, join(NETWORK_A, "#room", "alice"));

    let member = reducer.member_view(&room, "alice").expect("alice");
    assert_eq!(member.label, "@alice", "label");
    assert_eq!(member.nick, "alice", "nick");
}

#[test]
fn stale_names_sync_releases_after_thirty_seconds() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let started = reducer_timestamp();
    fold_at(&mut reducer, names(NETWORK_A, "#room", &["Alice"], false), started);
    let key = reducer.conversation_key(NETWORK_A, "#room");
    {
        let channel = state_of(&reducer, &key).channel.as_ref().expect("a channel");
        assert!(channel.names_syncing, "namesSyncing");
        assert!(channel.names_sync_started.is_some(), "namesSyncStarted");
    }

    assert!(
        !reducer.release_stale_names_sync(Some(&key), started + chrono::TimeDelta::seconds(29)),
        "at 29s"
    );
    assert!(state_of(&reducer, &key).channel.as_ref().expect("a channel").names_syncing, "still syncing");

    assert!(
        reducer.release_stale_names_sync(Some(&key), started + chrono::TimeDelta::seconds(31)),
        "at 31s"
    );
    assert!(!state_of(&reducer, &key).channel.as_ref().expect("a channel").names_syncing, "released");
}

#[test]
fn consecutive_joins_collapse_into_one_event() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, join(NETWORK_A, "#room", "Bob"));

    let conversation = state_of(&reducer, &reducer.conversation_key(NETWORK_A, "#room"));
    assert_eq!(conversation.messages.len(), 1, "messages");
    assert_eq!(conversation.messages[0].kind, MessageKind::Event, "kind");
    assert_eq!(conversation.messages[0].body, "Alice, Bob joined", "body");
    assert_eq!(conversation.trimmed, 0, "trimmed");
}

#[test]
fn privmsg_breaks_join_collapse() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, message(&room, "Alice", "hello", "#room"));
    fold(&mut reducer, join(NETWORK_A, "#room", "Bob"));

    let conversation = state_of(&reducer, &room);
    assert_eq!(conversation.messages.len(), 3, "messages");
    assert_eq!(conversation.messages[0].body, "Alice joined");
    assert_eq!(conversation.messages[1].kind, MessageKind::Message, "messages[1] kind");
    assert_eq!(conversation.messages[1].body, "hello");
    assert_eq!(conversation.messages[2].body, "Bob joined");
}

#[test]
fn kick_and_mode_stay_separate_from_join_line() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, kick(NETWORK_A, "#room", "Alice", "op", "bye"));
    fold(&mut reducer, join(NETWORK_A, "#room", "Bob"));
    fold(&mut reducer, mode(NETWORK_A, "#room", "op", "+v", &["Bob"]));
    fold(&mut reducer, join(NETWORK_A, "#room", "Carol"));

    let conversation = state_of(&reducer, &reducer.conversation_key(NETWORK_A, "#room"));
    assert_eq!(
        bodies(conversation),
        vec![
            "Alice joined",
            "Alice was kicked",
            "Bob joined",
            "op set mode +v Bob",
            "Carol joined",
        ]
    );
}

#[test]
fn mixed_join_part_quit_nick_collapse() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, join(NETWORK_A, "#room", "Bob"));
    fold(&mut reducer, part(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, QuitEvent {
        reason: "gone".to_owned(),
        ..quit(NETWORK_A, "Bob")
    });
    fold(&mut reducer, join(NETWORK_A, "#room", "Carol"));
    fold(&mut reducer, nick_change(NETWORK_A, "Carol", "Caroline"));

    let conversation = state_of(&reducer, &reducer.conversation_key(NETWORK_A, "#room"));
    assert_eq!(conversation.messages.len(), 1, "messages");
    assert_eq!(
        conversation.messages[0].body,
        "Alice, Bob joined, Alice left, Bob quit, Carol joined, Carol is now Caroline"
    );
}

#[test]
fn part_then_join_of_same_nick_is_dropped() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    fold(&mut reducer, part(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));

    {
        let conversation = state_of(&reducer, &room);
        assert_eq!(conversation.messages.len(), 1, "messages");
        assert_eq!(conversation.messages[0].body, "omairc joined", "body");
        assert_eq!(conversation.people_count(), 2, "people");
    }

    fold(&mut reducer, join(NETWORK_A, "#room", "Bob"));
    fold(&mut reducer, quit(NETWORK_A, "Bob"));
    fold(&mut reducer, join(NETWORK_A, "#room", "Bob"));
    let conversation = state_of(&reducer, &room);
    assert_eq!(conversation.messages.len(), 2, "messages after quit");
    assert_eq!(conversation.messages[1].body, "Bob joined", "bob");
    assert_eq!(conversation.people_count(), 3, "people after rejoin");
}

#[test]
fn line_between_part_and_join_keeps_both() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    fold(&mut reducer, part(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, message(&room, "omairc", "hello", "#room"));
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));

    let conversation = state_of(&reducer, &room);
    assert_eq!(conversation.messages.len(), 4, "messages");
    assert_eq!(conversation.messages[1].body, "Alice left", "left");
    assert_eq!(conversation.messages[2].body, "hello", "chat");
    assert_eq!(conversation.messages[3].body, "Alice joined", "joined");
}

#[test]
fn nick_chain_keeps_final_name() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, nick_change(NETWORK_A, "Alice", "Bob"));
    fold(&mut reducer, nick_change(NETWORK_A, "Bob", "Carol"));

    let conversation = state_of(&reducer, &reducer.conversation_key(NETWORK_A, "#room"));
    assert_eq!(conversation.messages.len(), 1, "messages");
    assert_eq!(conversation.messages[0].body, "Alice joined, Alice is now Carol", "body");
}

#[test]
fn membership_noise_shows_every_or_hides_including_history() {
    // Every: each membership line stays its own row.
    let mut every = EventReducer::new();
    welcome_omairc(&mut every, NETWORK_A);
    every.set_membership_noise(MembershipNoise::Every);
    fold(&mut every, join(NETWORK_A, "#room", "Alice"));
    fold(&mut every, join(NETWORK_A, "#room", "Bob"));
    let room = every.conversation_key(NETWORK_A, "#room");
    {
        let shown = state_of(&every, &room);
        assert_eq!(shown.messages.len(), 2, "every messages");
        assert_eq!(shown.messages[0].body, "Alice joined", "alice");
        assert_eq!(shown.messages[1].body, "Bob joined", "bob");
        assert!(!shown.messages[0].collapsible, "not collapsible");
    }

    // Hidden: membership lines disappear, and playback too.
    let mut hidden = EventReducer::new();
    welcome_omairc(&mut hidden, NETWORK_A);
    hidden.set_membership_noise(MembershipNoise::Hidden);
    fold(&mut hidden, join(NETWORK_A, "#room", "omairc"));
    fold(&mut hidden, join(NETWORK_A, "#room", "Alice"));
    fold(&mut hidden, part(NETWORK_A, "#room", "Alice"));
    fold(&mut hidden, kick(NETWORK_A, "#room", "Bob", "op", ""));
    fold(&mut hidden, mode(NETWORK_A, "#room", "op", "+v", &[]));
    {
        let quiet = state_of(&hidden, &room);
        assert_eq!(quiet.people_count(), 1, "people");
        assert_eq!(quiet.messages.len(), 2, "hidden messages");
        assert_eq!(quiet.messages[0].body, "Bob was kicked", "kick");
        assert_eq!(quiet.messages[1].body, "op set mode +v", "mode");
    }

    let playback = batch(
        "#room",
        HistoryKind::BouncerPlayback,
        &[
            "@time=2026-09-04T00:00:00.000Z :alice!u@h JOIN :#room",
            "@time=2026-09-04T00:00:01.000Z :bob!u@h JOIN :#room",
            "@time=2026-09-04T00:00:02.000Z :alice!u@h PRIVMSG #room :from history",
        ],
    );
    let played = history_of(&hidden, &playback);
    assert_eq!(played.lines.len(), 3, "played lines");
    fold(&mut hidden, played.clone());
    {
        let quiet = state_of(&hidden, &room);
        assert_eq!(quiet.people_count(), 1, "people after history");
        assert_eq!(quiet.messages.len(), 3, "hidden history messages");
        assert_eq!(quiet.messages[0].body, "from history", "history chat");
    }
    assert!(hidden.playback_batch_kept(NETWORK_A, "#room"), "playback kept");

    // Folded (the default): playback joins fold into one caption above the chat.
    let mut folded = EventReducer::new();
    welcome_omairc(&mut folded, NETWORK_A);
    fold(&mut folded, join(NETWORK_A, "#room", "omairc"));
    fold(&mut folded, played.clone());
    {
        let folded_room = state_of(&folded, &room);
        assert_eq!(folded_room.people_count(), 1, "folded people");
        assert_eq!(folded_room.messages.len(), 3, "folded messages");
        assert_eq!(folded_room.messages[0].body, "alice, bob joined", "folded joins");
        assert_eq!(folded_room.messages[1].body, "from history", "folded chat");
        assert_eq!(folded_room.messages[2].body, "omairc joined", "self join");
    }

    // Every, with the self join and the same playback.
    fold(&mut every, join(NETWORK_A, "#room", "omairc"));
    fold(&mut every, played);
    let shown = state_of(&every, &room);
    assert_eq!(shown.people_count(), 3, "every people");
    assert_eq!(shown.messages[2].body, "alice joined", "historic alice");
    assert_eq!(shown.messages[3].body, "bob joined", "historic bob");
    assert_eq!(shown.messages[4].body, "from history", "historic chat");
}

#[test]
fn lone_membership_line_keeps_today_wording() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, message(&room, "Alice", "hello", "#room"));
    fold(&mut reducer, part(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, message(&room, "Alice", "gap", "#room"));
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, message(&room, "Alice", "back", "#room"));
    fold(&mut reducer, QuitEvent {
        reason: "bye".to_owned(),
        ..quit(NETWORK_A, "Alice")
    });
    fold(&mut reducer, message(&room, "Alice", "next", "#room"));
    fold(&mut reducer, join(NETWORK_A, "#room", "Bob"));
    fold(&mut reducer, message(&room, "Bob", "hi", "#room"));
    fold(&mut reducer, nick_change(NETWORK_A, "Bob", "Bobby"));

    let conversation = state_of(&reducer, &room);
    assert_eq!(conversation.messages[0].body, "Alice joined", "join");
    assert_eq!(conversation.messages[2].body, "Alice left", "part");
    assert_eq!(conversation.messages[4].body, "Alice joined", "rejoin");
    assert_eq!(conversation.messages[6].body, "Alice quit", "quit");
    assert_eq!(conversation.messages[8].body, "Bob joined", "bob");
    assert_eq!(conversation.messages[10].body, "Bob is now Bobby", "nick");
}
