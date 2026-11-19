//! Replies and reactions: a reply keeps its parent, a reaction lands on a line the
//! transcript holds and on nothing else, and neither opens a conversation, marks
//! unread or raises an arrival. Go twin: `tui/internal/irc/reactions_test.go`.

use super::*;
use crate::irc::event::ReactionEvent;
use crate::irc::reaction::{MAX_REACTION_EMOJIS, MAX_REACTION_NICKS};

fn react(key: &ConversationKey, nick: &str, target: &str, emoji: &str, removed: bool) -> ReactionEvent {
    ReactionEvent {
        conversation: key.clone(),
        nick: nick.to_owned(),
        target_msgid: MsgId::new(target),
        emoji: emoji.to_owned(),
        removed,
        received_at: reducer_timestamp(),
    }
}

fn with_id(mut event: MessageEvent, msgid: &str) -> MessageEvent {
    event.msgid = MsgId::new(msgid);
    event
}

/// A welcomed network with `#room` joined and one line `m1` from alice.
fn room_with_line() -> (EventReducer, ConversationKey) {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    let key = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, with_id(message(&key, "alice", "ship it", "#room"), "m1"));
    (reducer, key)
}

fn chips(reducer: &EventReducer, key: &ConversationKey, msgid: &str) -> Vec<(String, usize, Vec<String>)> {
    state_of(reducer, key)
        .reactions
        .get(msgid)
        .map(|set| {
            set.chips()
                .into_iter()
                .map(|chip| (chip.emoji, chip.count, chip.nicks))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_reply_keeps_the_parent_msgid_and_a_missing_parent_is_no_error() {
    let (mut reducer, key) = room_with_line();
    let mut loaded = with_id(message(&key, "bob", "agreed", "#room"), "m2");
    loaded.reply_to = MsgId::new("m1");
    fold(&mut reducer, loaded);
    let mut orphan = with_id(message(&key, "carol", "late", "#room"), "m3");
    orphan.reply_to = MsgId::new("gone");
    fold(&mut reducer, orphan);

    let state = state_of(&reducer, &key);
    let reply = message_by_body(&state.messages, "agreed");
    assert_eq!(reply.reply_to, "m1");
    assert_eq!(message_by_body(&state.messages, "late").reply_to, "gone");
    assert_eq!(message_by_body(&state.messages, "ship it").reply_to, "");
}

#[test]
fn a_reaction_adds_once_per_nick_across_case_mapping_and_unreact_removes_it() {
    let (mut reducer, key) = room_with_line();
    fold(&mut reducer, react(&key, "Bob[", "m1", "👍", false));
    fold(&mut reducer, react(&key, "bob{", "m1", "👍", false));
    assert_eq!(
        chips(&reducer, &key, "m1"),
        vec![("👍".to_owned(), 1, vec!["Bob[".to_owned()])],
        "the same nick under the case mapping counts once"
    );

    fold(&mut reducer, react(&key, "carol", "m1", "👍", false));
    fold(&mut reducer, react(&key, "carol", "m1", "🎉", false));
    let seen = chips(&reducer, &key, "m1");
    assert_eq!(seen[0].1, 2);
    assert_eq!(seen[0].2, vec!["Bob[", "carol"]);
    assert_eq!(seen[1].0, "🎉");

    // An unreact from an absent nick or for an absent emoji is a no-op.
    fold(&mut reducer, react(&key, "dave", "m1", "👍", true));
    fold(&mut reducer, react(&key, "carol", "m1", "🔥", true));
    assert_eq!(chips(&reducer, &key, "m1").len(), 2);

    fold(&mut reducer, react(&key, "BOB{", "m1", "👍", true));
    assert_eq!(chips(&reducer, &key, "m1")[0].2, vec!["carol"]);
    fold(&mut reducer, react(&key, "carol", "m1", "🎉", true));
    fold(&mut reducer, react(&key, "carol", "m1", "👍", true));
    assert!(
        state_of(&reducer, &key).reactions.is_empty(),
        "the last nick takes the chip and the line's entry with it"
    );
}

#[test]
fn a_reaction_to_an_unknown_msgid_is_dropped_without_a_row() {
    let (mut reducer, key) = room_with_line();
    let rows = state_of(&reducer, &key).messages.len();
    fold(&mut reducer, react(&key, "bob", "nope", "👍", false));
    let state = state_of(&reducer, &key);
    assert!(state.reactions.is_empty());
    assert_eq!(state.messages.len(), rows, "no placeholder row");
}

#[test]
fn a_reaction_to_a_membership_line_is_dropped() {
    // A join line may carry a msgid, but only chat lines show chips.
    let (mut reducer, key) = room_with_line();
    let mut membership = history_event(
        &key,
        "#room",
        HistoryKind::Chat,
        false,
        vec![ReplayLine {
            kind: MessageKindTag::Event,
            ..replay_line("", "bob joined", "e1")
        }],
    );
    membership.lines[0].author = String::new();
    fold(&mut reducer, membership);
    fold(&mut reducer, react(&key, "bob", "e1", "👍", false));
    assert!(state_of(&reducer, &key).reactions.is_empty());
}

#[test]
fn a_reaction_never_opens_a_conversation() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let direct = reducer.conversation_key(NETWORK_A, "alice");
    let channel = reducer.conversation_key(NETWORK_A, "#nowhere");
    let before = reducer.conversations().len();
    fold(&mut reducer, react(&direct, "alice", "m1", "👍", false));
    fold(&mut reducer, react(&channel, "alice", "m1", "👍", false));
    assert_eq!(reducer.conversations().len(), before);
    assert!(reducer.find(&direct).is_none());
    assert!(reducer.find(&channel).is_none());
}

#[test]
fn a_reaction_changes_no_unread_mention_inbox_or_log() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let log = RecordingLog::default();
    reducer.set_conversation_log(Some(Box::new(log.clone())));
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    let key = reducer.conversation_key(NETWORK_A, "#room");
    let other = reducer.conversation_key(NETWORK_A, "#other");
    reducer.mark_selected(&other);
    fold(&mut reducer, join(NETWORK_A, "#other", "omairc"));
    fold(&mut reducer, with_id(message(&key, "alice", "omairc: look", "#room"), "m1"));
    let _ = reducer.take_mention_arrival();
    let _ = reducer.take_inbox_arrival();
    let (unread, mentions, mark) = {
        let state = state_of(&reducer, &key);
        (state.unread, state.mentions, state.unread_mark)
    };
    assert_eq!(unread, 1);
    let logged = log.snapshot().len();

    let notify = fold(&mut reducer, react(&key, "bob", "m1", "👍", false));
    // A reaction to the user's own line or a mention-bearing line stays quiet.
    fold(&mut reducer, react(&key, "omairc", "m1", "❤", false));

    let state = state_of(&reducer, &key);
    assert_eq!((state.unread, state.mentions, state.unread_mark), (unread, mentions, mark));
    assert!(reducer.take_mention_arrival().is_none());
    assert!(reducer.take_inbox_arrival().is_none());
    assert_eq!(log.snapshot().len(), logged, "reactions are in memory only");
    assert_eq!(notify, vec![ViewNotify::Messages], "only the transcript repaints");
    assert_eq!(chips(&reducer, &key, "m1").len(), 2, "a reaction from the user applies like any other");
}

#[test]
fn a_dropped_reaction_repaints_nothing() {
    let (mut reducer, key) = room_with_line();
    assert!(fold(&mut reducer, react(&key, "bob", "nope", "👍", false)).is_empty());
    let missing = reducer.conversation_key(NETWORK_A, "#nowhere");
    assert!(fold(&mut reducer, react(&missing, "bob", "m1", "👍", false)).is_empty());
}

#[test]
fn reactions_are_bounded_per_line() {
    let (mut reducer, key) = room_with_line();
    for index in 0..MAX_REACTION_EMOJIS + 4 {
        fold(&mut reducer, react(&key, "bob", "m1", &format!("e{index}"), false));
    }
    assert_eq!(chips(&reducer, &key, "m1").len(), MAX_REACTION_EMOJIS);
    for index in 0..MAX_REACTION_NICKS + 4 {
        fold(&mut reducer, react(&key, &format!("n{index}"), "m1", "e0", false));
    }
    assert_eq!(chips(&reducer, &key, "m1")[0].1, MAX_REACTION_NICKS);
}

#[test]
fn a_trimmed_line_takes_its_reactions_with_it() {
    let (mut reducer, key) = room_with_line();
    fold(&mut reducer, react(&key, "bob", "m1", "👍", false));
    assert_eq!(chips(&reducer, &key, "m1").len(), 1);
    for index in 0..super::super::MAX_MESSAGES {
        fold(&mut reducer, message(&key, "alice", &format!("filler {index}"), "#room"));
    }
    let state = state_of(&reducer, &key);
    assert!(!state.message_ids.contains(&MsgId::new("m1")), "m1 was trimmed");
    assert!(state.reactions.is_empty(), "its reactions went with it");
}

#[test]
fn merged_direct_messages_keep_the_reactions_of_the_moved_lines() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let old = reducer.conversation_key(NETWORK_A, "alice");
    let new = reducer.conversation_key(NETWORK_A, "alicia");
    fold(&mut reducer, with_id(message(&new, "alicia", "already here", "alicia"), "m2"));
    fold(&mut reducer, with_id(message(&old, "alice", "hi", "alice"), "m1"));
    fold(&mut reducer, react(&old, "alice", "m1", "👍", false));
    fold(&mut reducer, nick_change(NETWORK_A, "alice", "alicia"));
    assert!(reducer.find(&old).is_none(), "the two direct messages merged");
    assert_eq!(chips(&reducer, &new, "m1").len(), 1, "the moved line keeps its chip");
}

#[test]
fn a_reply_keeps_its_parent_across_a_restart_and_reactions_are_not_logged() {
    let log = RecordingLog::default();
    let mut first = EventReducer::new();
    first.set_conversation_log(Some(Box::new(log.clone())));
    welcome_omairc(&mut first, NETWORK_A);
    fold(&mut first, join(NETWORK_A, "#room", "omairc"));
    apply_wire(&mut first, "@msgid=m1 :alice!u@h PRIVMSG #room :ship it");
    apply_wire(&mut first, "@msgid=m2;+draft/reply=m1 :bob!u@h PRIVMSG #room :agreed");
    apply_wire(&mut first, "@+draft/reply=m1;+draft/react=👍 :carol!u@h TAGMSG #room");
    assert_eq!(log.snapshot().len(), 3, "join, two lines, no reaction");

    let mut again = EventReducer::new();
    again.set_conversation_log(Some(Box::new(log.clone())));
    welcome_omairc(&mut again, NETWORK_A);
    fold(&mut again, join(NETWORK_A, "#room", "omairc"));
    let room = again.conversation_key(NETWORK_A, "#room");
    let state = state_of(&again, &room);
    assert_eq!(message_by_body(&state.messages, "agreed").reply_to, "m1");
    assert!(state.reactions.is_empty(), "reactions are in memory only");
}
