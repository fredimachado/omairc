//! Conversation creation under its cause, drops, closed marks, the cap, clearing,
//! and the network-wide forget. Ports of `reducer_test.go`.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use super::*;
use crate::irc::casemapping::IrcCaseMapping;
use crate::irc::event::{HistoryKind, IrcConversationCause, MsgId};
use crate::irc::reducer::ClosedStore;

/// An in-memory closed list. It keeps the `(network, normalized target)` pairs the way
/// the controller's list does, without reaching into `storage/`.
#[derive(Clone, Default)]
struct MemoryClosed(Rc<RefCell<BTreeSet<(String, String)>>>);

impl ClosedStore for MemoryClosed {
    fn contains(&self, network_id: &str, target: &str, mapping: IrcCaseMapping) -> bool {
        self.0
            .borrow()
            .contains(&(network_id.to_owned(), mapping.normalize(target)))
    }

    fn add(&self, network_id: &str, target: &str, mapping: IrcCaseMapping) {
        self.0
            .borrow_mut()
            .insert((network_id.to_owned(), mapping.normalize(target)));
    }

    fn remove(&self, network_id: &str, target: &str, mapping: IrcCaseMapping) {
        self.0
            .borrow_mut()
            .remove(&(network_id.to_owned(), mapping.normalize(target)));
    }
}

#[test]
fn direct_messages_use_composite_keys() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    welcome_omairc(&mut reducer, NETWORK_B);
    let alice_a = reducer.conversation_key(NETWORK_A, "Alice");
    let alice_b = reducer.conversation_key(NETWORK_B, "Alice");

    fold(&mut reducer, message(&alice_a, "ALICE", "hello", "Alice"));
    {
        let direct = state_of(&reducer, &alice_a);
        assert!(!direct.is_channel(), "isChannel");
        assert_eq!(direct.target, "Alice", "target");
        assert_eq!(direct.messages.len(), 1, "messages");
    }
    assert!(reducer.find(&alice_b).is_none(), "networkB direct");

    fold(&mut reducer, nick_change(NETWORK_A, "alice", "Alicia"));
    assert!(reducer.find(&alice_a).is_none(), "old key dropped");
    let renamed = state_of(&reducer, &reducer.conversation_key(NETWORK_A, "alicia"));
    assert_eq!(renamed.target, "Alicia", "renamed target");
}

#[test]
fn nick_appends_event_to_direct_message() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let alice = reducer.conversation_key(NETWORK_A, "Alice");
    fold(&mut reducer, message(&alice, "Alice", "hello", "Alice"));
    fold(&mut reducer, nick_change(NETWORK_A, "Alice", "Alicia"));

    let renamed = state_of(&reducer, &reducer.conversation_key(NETWORK_A, "Alicia"));
    assert_eq!(renamed.target, "Alicia", "target");
    assert_eq!(renamed.messages.len(), 2, "messages");
    let last = renamed.messages.last().expect("a row");
    assert_eq!(last.kind, crate::irc::world::MessageKind::Event, "last kind");
    assert_eq!(last.body, "Alice is now Alicia", "last body");
}

#[test]
fn nick_case_only_updates_direct_display_nick() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let alice = reducer.conversation_key(NETWORK_A, "Alice");
    fold(&mut reducer, message(&alice, "Alice", "hello", "Alice"));
    fold(&mut reducer, nick_change(NETWORK_A, "Alice", "ALICE"));

    let same = state_of(&reducer, &alice);
    assert_eq!(same.target, "ALICE", "target");
    assert_eq!(same.messages.len(), 2, "messages");
    assert_eq!(same.messages.last().expect("a row").body, "Alice is now ALICE", "last body");
}

#[test]
fn drop_direct_message_erases_only_direct_rows() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let lena = reducer.conversation_key(NETWORK_A, "lena");
    let channel = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, message(&lena, "lena", "hi", "lena"));
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    reducer.mark_selected(&lena);

    assert_eq!(reducer.conversations().len(), 2, "conversations");
    assert!(reducer.drop_direct_message(&lena), "drop direct");
    assert!(reducer.find(&lena).is_none(), "direct gone");
    assert!(reducer.find(&channel).expect("channel kept").is_channel(), "channel is channel");
    assert_eq!(reducer.conversations().len(), 1, "conversations after drop");
    assert!(!reducer.drop_direct_message(&lena), "second drop");
    assert!(!reducer.drop_direct_message(&channel), "channel is not direct");
    assert!(reducer.find(&channel).is_some(), "channel still kept");
    assert_eq!(reducer.conversations().len(), 1, "conversations after rejected drop");

    fold(&mut reducer, message(&lena, "lena", "again", "lena"));
    assert_eq!(state_of(&reducer, &lena).unread, 1, "recreated unread");
}

#[test]
fn drop_channel_erases_only_channel_rows() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let lena = reducer.conversation_key(NETWORK_A, "lena");
    let channel = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, message(&lena, "lena", "hi", "lena"));
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    reducer.mark_selected(&channel);

    assert_eq!(reducer.conversations().len(), 2, "conversations");
    assert!(reducer.drop_channel(&channel), "drop channel");
    assert!(reducer.find(&channel).is_none(), "channel gone");
    assert!(!state_of(&reducer, &lena).is_channel(), "direct kept and not a channel");
    assert_eq!(reducer.conversations().len(), 1, "conversations after drop");
    assert!(reducer.selected().is_none(), "selection cleared");
    assert!(!reducer.drop_channel(&channel), "second drop");
    assert!(!reducer.drop_channel(&lena), "direct is not channel");
    assert!(reducer.find(&lena).is_some(), "direct still kept");
    assert_eq!(reducer.conversations().len(), 1, "conversations after rejected drop");

    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    let recreated = state_of(&reducer, &channel);
    assert!(recreated.is_channel(), "recreated is channel");
    assert!(recreated.channel.as_ref().expect("a channel").joined, "recreated joined");
}

#[test]
fn closed_channel_stays_closed_until_join() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let channel = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));
    assert!(reducer.mark_channel_left(&channel), "mark left");
    {
        let left = state_of(&reducer, &channel);
        assert!(!left.channel.as_ref().expect("a channel").joined, "left is not joined");
        assert!(left.channel.as_ref().expect("a channel").members.is_empty(), "left has no members");
    }

    reducer.note_closed(&channel);
    assert!(reducer.drop_channel(&channel), "drop closed");
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, message(&channel, "Alice", "back", "#room"));
    fold(
        &mut reducer,
        history_event(&channel, "#room", HistoryKind::Chat, false, Vec::new()),
    );
    assert!(reducer.find(&channel).is_none(), "catch-up stayed closed");

    reducer.clear_closed(&channel);
    assert!(reducer.ensure_conversation(&channel, "#room", IrcConversationCause::ChannelState).is_some(), "explicit join inserts");

    reducer.note_closed(&channel);
    assert!(reducer.drop_channel(&channel), "drop again");
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    assert!(reducer.find(&channel).is_some(), "welcome lets autojoin back");
}

#[test]
fn persisted_refusal_survives_welcome() {
    let mut reducer = EventReducer::new();
    reducer.set_closed_persistence(Box::new(MemoryClosed::default()));
    welcome_omairc(&mut reducer, NETWORK_A);
    let channel = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    reducer.note_closed(&channel);
    assert!(reducer.drop_channel(&channel), "drop closed");
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, message(&channel, "Alice", "back", "#room"));
    assert!(
        reducer.find(&channel).is_none(),
        "catch-up stayed closed after welcome dropped memory"
    );
}

#[test]
fn clear_messages_wipes_transcript_keeps_row() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let lena = reducer.conversation_key(NETWORK_A, "lena");
    let channel = reducer.conversation_key(NETWORK_A, "#room");
    let missing = reducer.conversation_key(NETWORK_A, "ghost");

    fold(&mut reducer, message(&lena, "lena", "hi", "lena"));
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));
    fold(&mut reducer, topic(NETWORK_A, "#room", "topic", "op"));
    fold(&mut reducer, message(&channel, "Alice", "hello", "#room"));

    assert_eq!(reducer.conversations().len(), 2, "conversations");
    assert!(!state_of(&reducer, &lena).is_channel(), "direct not channel");
    assert_eq!(state_of(&reducer, &lena).messages.len(), 1, "direct messages");

    reducer.clear_messages(&lena);
    assert!(!state_of(&reducer, &lena).is_channel(), "direct still not channel");
    assert_eq!(state_of(&reducer, &lena).messages.len(), 0, "direct messages after clear");
    assert_eq!(reducer.conversations().len(), 2, "conversations after clear");

    reducer.clear_messages(&lena);
    assert_eq!(state_of(&reducer, &lena).messages.len(), 0, "direct messages after second clear");
    assert!(reducer.find(&lena).is_some(), "direct still present");

    {
        let room = state_of(&reducer, &channel);
        assert!(room.is_channel(), "room is channel");
        assert_eq!(room.people_count(), 2, "room peopleCount");
        assert_eq!(room.channel.as_ref().expect("a channel").topic, "topic", "room topic");
        assert!(!room.messages.is_empty(), "room has messages");
    }
    reducer.clear_messages(&channel);
    {
        let room = state_of(&reducer, &channel);
        assert_eq!(room.messages.len(), 0, "room messages after clear");
        assert_eq!(room.people_count(), 2, "room peopleCount after clear");
        assert_eq!(room.channel.as_ref().expect("a channel").topic, "topic", "room topic after clear");
        assert!(room.is_channel(), "room still channel");
    }

    assert!(reducer.find(&missing).is_none(), "missing absent");
    reducer.clear_messages(&missing);
    assert!(reducer.find(&missing).is_none(), "missing still absent");
    assert_eq!(reducer.conversations().len(), 2, "conversations after missing clear");

    fold(&mut reducer, MessageEvent {
        msgid: MsgId::new("seen-id"),
        ..message(&lena, "lena", "seen", "lena")
    });
    reducer.clear_messages(&lena);
    assert_eq!(state_of(&reducer, &lena).messages.len(), 0, "direct messages after clear");
    assert!(
        state_of(&reducer, &lena).message_ids.contains(&MsgId::new("seen-id")),
        "clearMessages must keep the msgid set"
    );
    fold(
        &mut reducer,
        history_event(
            &lena,
            "lena",
            HistoryKind::Chat,
            false,
            vec![replay_line("lena", "seen", "seen-id"), replay_line("lena", "later", "later-id")],
        ),
    );
    let direct = state_of(&reducer, &lena);
    assert_eq!(direct.messages.len(), 1, "direct messages after replay");
    assert_eq!(direct.messages[0].body, "later", "replay body");
    assert_eq!(direct.messages[0].msgid.value, "later-id", "replay msgid");

    assert!(reducer.drop_direct_message(&lena), "drop direct");
    assert!(reducer.find(&lena).is_none(), "direct gone");
    assert!(reducer.find(&channel).is_some(), "channel kept");
    assert!(!reducer.drop_direct_message(&channel), "channel not direct");
    assert!(reducer.find(&channel).is_some(), "channel still kept");
    assert_eq!(reducer.conversations().len(), 1, "conversations");
}

#[test]
fn messages_cap_at_two_thousand_fifo() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    for index in 0..2001 {
        fold(&mut reducer, message(&room, "Alice", &index.to_string(), "#room"));
    }

    {
        let conversation = state_of(&reducer, &room);
        assert_eq!(conversation.messages.len(), 2000, "messages");
        assert_eq!(conversation.messages[0].body, "1", "front");
        assert_eq!(conversation.messages.last().expect("a row").body, "2000", "back");
        assert_eq!(conversation.trimmed, 1, "trimmed");
    }

    fold(&mut reducer, join(NETWORK_A, "#room", "Bob"));
    let conversation = state_of(&reducer, &room);
    assert_eq!(conversation.messages.len(), 2000, "messages after join");
    assert_eq!(conversation.messages[0].body, "2", "front after join");
    let last = conversation.messages.last().expect("a row");
    assert_eq!(last.body, "Bob joined", "back after join");
    assert_eq!(last.kind, crate::irc::world::MessageKind::Event, "back kind after join");
    assert_eq!(conversation.trimmed, 2, "trimmed after join");
}

#[test]
fn clear_messages_empties_after_cap() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    for index in 0..2001 {
        fold(&mut reducer, message(&room, "Alice", &index.to_string(), "#room"));
    }
    assert_eq!(state_of(&reducer, &room).messages.len(), 2000, "messages");

    reducer.clear_messages(&room);
    let conversation = state_of(&reducer, &room);
    assert_eq!(conversation.messages.len(), 0, "messages after clear");
    assert_eq!(conversation.trimmed, 2001, "trimmed after clear");
}

#[test]
fn forget_network_leaves_the_other_network() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    welcome_omairc(&mut reducer, NETWORK_B);
    fold(&mut reducer, join(NETWORK_A, "#alpha", "omairc"));
    fold(&mut reducer, join(NETWORK_B, "#lab", "omairc"));
    let rio = reducer.conversation_key(NETWORK_B, "rio");
    fold(&mut reducer, message(&rio, "rio", "ping", "rio"));
    let lab = reducer.conversation_key(NETWORK_B, "#lab");
    reducer.mark_selected(&lab);

    reducer.forget_network(NETWORK_B);
    assert!(reducer.find(&reducer.conversation_key(NETWORK_A, "#alpha")).is_some(), "networkA channel kept");
    assert!(reducer.find(&lab).is_none(), "networkB channel gone");
    assert!(reducer.find(&rio).is_none(), "networkB direct gone");
    assert!(reducer.selected().is_none(), "selection cleared");
    assert_eq!(reducer.conversations().len(), 1, "conversations");
}

#[test]
fn conversation_cause_insert_table() {
    let cases = [
        ("UserOpen direct", IrcConversationCause::UserOpen, false, false, true),
        ("UserOpen service", IrcConversationCause::UserOpen, false, true, true),
        ("UserOpen channel", IrcConversationCause::UserOpen, true, false, false),
        ("ChannelState channel", IrcConversationCause::ChannelState, true, false, true),
        ("ChannelState direct", IrcConversationCause::ChannelState, false, false, false),
        ("InboundOther channel", IrcConversationCause::InboundOther, true, false, true),
        ("InboundOther direct", IrcConversationCause::InboundOther, false, false, true),
        ("InboundOther service", IrcConversationCause::InboundOther, false, true, false),
        ("InboundSelf direct", IrcConversationCause::InboundSelf, false, false, false),
        ("InboundSelf channel", IrcConversationCause::InboundSelf, true, false, false),
        ("QuietSend direct", IrcConversationCause::QuietSend, false, false, false),
        ("QuietSend channel", IrcConversationCause::QuietSend, true, false, false),
        ("Restore direct", IrcConversationCause::Restore, false, false, true),
        ("Restore service", IrcConversationCause::Restore, false, true, false),
        ("Restore channel", IrcConversationCause::Restore, true, false, false),
    ];
    for (name, cause, channel, service, want) in cases {
        assert_eq!(cause.inserts(channel, service), want, "{name}");
    }
}

#[test]
fn ensure_conversation_honors_cause() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let lena = reducer.conversation_key(NETWORK_A, "lena");
    let nickserv = reducer.conversation_key(NETWORK_A, "NickServ");
    let room = reducer.conversation_key(NETWORK_A, "#room");
    let ghost = reducer.conversation_key(NETWORK_A, "ghost");

    assert!(reducer.ensure_conversation(&lena, "lena", IrcConversationCause::QuietSend).is_none(), "QuietSend must not invent a direct message");
    assert!(reducer.find(&lena).is_none(), "lena absent");
    assert!(reducer.ensure_conversation(&lena, "lena", IrcConversationCause::InboundSelf).is_none(), "InboundSelf must not invent a direct message");
    assert!(reducer.find(&lena).is_none(), "lena still absent");

    reducer.mark_selected(&ghost);
    assert!(reducer.ensure_conversation(&ghost, "ghost", IrcConversationCause::InboundSelf).is_none(), "InboundSelf must not invent a selected conversation");
    assert!(reducer.find(&ghost).is_none(), "ghost absent");

    let human = reducer.ensure_conversation(&lena, "lena", IrcConversationCause::InboundOther).is_some();
    assert!(human, "human present");
    assert!(!state_of(&reducer, &lena).is_channel(), "human not channel");

    assert!(reducer.ensure_conversation(&nickserv, "NickServ", IrcConversationCause::InboundOther).is_none(), "InboundOther must not invent a service query");
    assert!(reducer.find(&nickserv).is_none(), "nickserv absent");
    assert!(reducer.ensure_conversation(&nickserv, "NickServ", IrcConversationCause::UserOpen).is_some(), "queried present");
    assert!(!state_of(&reducer, &nickserv).is_channel(), "queried not channel");

    assert!(reducer.ensure_conversation(&room, "#room", IrcConversationCause::UserOpen).is_none(), "UserOpen must not invent a channel");
    assert!(reducer.find(&room).is_none(), "room absent");
    assert!(reducer.ensure_conversation(&room, "#room", IrcConversationCause::ChannelState).is_some(), "channel present");
    assert!(state_of(&reducer, &room).is_channel(), "channel is channel");

    assert!(reducer.ensure_conversation(&lena, "lena", IrcConversationCause::QuietSend).is_some(), "quiet send existing");
    assert!(reducer.ensure_conversation(&lena, "lena", IrcConversationCause::InboundSelf).is_some(), "inbound self existing");

    assert!(reducer.find(&ghost).is_none(), "ghost still absent");
    assert!(reducer.ensure_conversation(&ghost, "ghost", IrcConversationCause::Restore).is_some(), "restored present");
    assert!(!state_of(&reducer, &ghost).is_channel(), "restored not channel");

    let mut restore_only = EventReducer::new();
    welcome_omairc(&mut restore_only, NETWORK_A);
    assert!(restore_only.ensure_conversation(&nickserv, "NickServ", IrcConversationCause::Restore).is_none(), "Restore must not invent a service query");
    assert!(restore_only.find(&nickserv).is_none(), "restore nickserv absent");
    assert!(restore_only.ensure_conversation(&room, "#room", IrcConversationCause::Restore).is_none(), "Restore must not invent a channel");

    fold(&mut reducer, message(&lena, "omairc", "hello", "lena"));
    let human = state_of(&reducer, &lena);
    assert_eq!(human.messages.len(), 1, "human messages");
    assert_eq!(human.messages[0].body, "hello", "human last body");

    let mut inbound = EventReducer::new();
    welcome_omairc(&mut inbound, NETWORK_A);
    fold(&mut inbound, message(&nickserv, "NickServ", "identify", "NickServ"));
    assert!(inbound.find(&nickserv).is_none(), "inbound nickserv absent");
    fold(&mut inbound, message(&lena, "lena", "hi", "lena"));
    let opened = state_of(&inbound, &lena);
    assert!(!opened.is_channel(), "opened not channel");
    assert_eq!(opened.messages.len(), 1, "opened messages");
}

#[test]
fn nick_shaped_join_does_not_invent_direct() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "lena", "omairc"));
    assert!(reducer.find(&reducer.conversation_key(NETWORK_A, "lena")).is_none(), "no direct invented");
}
