//! The reducer's read-marker rules: the newest server-stamped line stands in for
//! a marker, and a line the marker covers plants no mention.

use super::*;
use crate::irc::event::{ConversationKey, MsgId};
use crate::irc::reducer::latest_server_message_time;
use crate::irc::reducer::ConversationState;
use crate::irc::reducer::transcript::{note_chat_arrival, ChatArrival};
use crate::irc::world::{Message, MessageKind, Origin};
use crate::irc::event::IrcConversationCause;
use chrono::TimeZone;

#[test]
fn latest_server_message_time_ignores_wall_clock_fallback() {
    let wall = Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap();
    let server = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
    let mut conv = ConversationState::new(ConversationKey::new(NETWORK_A, "peer"), "peer");
    conv.messages.push(Message {
        kind: MessageKind::Message,
        author: "peer".to_owned(),
        body: "hi".to_owned(),
        time: wall,
        server_time: None,
        msgid: MsgId::default(),
        origin: Origin::Live,
        mentioned: false,
        collapsible: false,
        sequence: 0,
        only_visible_to_you: false,
    });
    assert!(
        latest_server_message_time(&conv).is_none(),
        "line without server-time tag must not publish a marker time"
    );
    conv.messages[0].server_time = Some(server);
    assert_eq!(latest_server_message_time(&conv), Some(server), "latest server message time");
}

#[test]
fn covered_read_marker_skips_mention_arrival() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let key = reducer.conversation_key(NETWORK_A, "#room");
    reducer.ensure_conversation(&key, "#room", IrcConversationCause::ChannelState);
    let marker = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
    reducer.conversations.get_mut(&key).expect("room").read_marker = Some(marker);
    let msgid = MsgId::default();
    let conv = reducer.conversations.get_mut(&key).expect("room");
    note_chat_arrival(
        &mut reducer.facts,
        conv,
        &key,
        ChatArrival {
            author: "peer",
            body: "omairc: ping",
            kind: MessageKind::Message,
            msgid: &msgid,
            sequence: 1,
            server_time: Some(marker),
            origin: Origin::Live,
            history: None,
        },
    );
    assert!(reducer.take_mention_arrival().is_none(), "covered mention must not plant MentionArrival");
}

#[test]
fn apply_server_read_marker_recounts_covered_badges() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    let early = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
    let late = Utc.with_ymd_and_hms(2024, 6, 1, 12, 5, 0).unwrap();
    fold(&mut reducer, MessageEvent {
        server_time: Some(early),
        ..message(&room, "peer", "omairc: first", "#room")
    });
    fold(&mut reducer, MessageEvent {
        server_time: Some(late),
        ..message(&room, "peer", "omairc: second", "#room")
    });
    assert_eq!(state_of(&reducer, &room).unread, 2, "unread before marker (2 chats)");
    assert_eq!(state_of(&reducer, &room).mentions, 2, "mentions before marker");

    assert!(!reducer.apply_server_read_marker(&room, None), "the * sentinel changes nothing");
    assert!(reducer.apply_server_read_marker(&room, Some(early)), "the marker is newer than none");
    assert_eq!(state_of(&reducer, &room).mentions, 1, "mentions after marker covers the early line");
    assert!(!reducer.apply_server_read_marker(&room, Some(early)), "an equal marker is not newer");
}
