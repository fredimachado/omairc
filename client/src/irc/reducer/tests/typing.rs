//! Typing hints: the stored clock, the 30-second paused hold, and the nick and
//! conversation remaps. Ports of the reducer tests in `typing_test.go`.

use super::*;
use crate::irc::event::TypingEvent;
use crate::irc::typing::TypingPhase;
use chrono::TimeDelta;

fn at(milliseconds: i64) -> DateTime<Utc> {
    typing_t0() + TimeDelta::milliseconds(milliseconds)
}

fn typing_event(key: &ConversationKey, nick: &str, phase: TypingPhase, received_at: DateTime<Utc>) -> TypingEvent {
    TypingEvent {
        conversation: key.clone(),
        nick: nick.to_owned(),
        phase,
        received_at,
    }
}

#[test]
fn reducer_stores_typing_clocks_and_expires_at_read() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold_at(&mut reducer, join(NETWORK_A, "#omarchy", "omairc"), typing_t0());
    fold_at(&mut reducer, join(NETWORK_A, "#omarchy", "alice"), typing_t0());
    let key = reducer.conversation_key(NETWORK_A, "#omarchy");

    fold_at(&mut reducer, typing_event(&key, "alice", TypingPhase::Active, typing_t0()), typing_t0());
    assert_eq!(reducer.typing_nicks(&key, at(5999)), vec!["alice"], "active at 5999");
    assert!(reducer.typing_nicks(&key, at(6000)).is_empty(), "active at 6000");
    assert_eq!(state_of(&reducer, &key).typing.len(), 1, "hint still stored");

    fold_at(&mut reducer, typing_event(&key, "alice", TypingPhase::Active, at(4000)), at(4000));
    assert_eq!(reducer.typing_nicks(&key, at(9000)), vec!["alice"], "refreshed at 9000");

    fold_at(&mut reducer, typing_event(&key, "bob", TypingPhase::Paused, typing_t0()), typing_t0());
    assert_eq!(reducer.typing_nicks(&key, at(5000)), vec!["alice", "bob"], "paused at 5000");
    assert_eq!(reducer.typing_nicks(&key, at(10000)), vec!["bob"], "paused at 10000");
    assert_eq!(reducer.typing_nicks(&key, at(29999)), vec!["bob"], "paused at 29999");
    assert!(reducer.typing_nicks(&key, at(30000)).is_empty(), "paused at 30000");

    fold_at(&mut reducer, typing_event(&key, "alice", TypingPhase::Done, at(5000)), at(5000));
    fold_at(&mut reducer, typing_event(&key, "bob", TypingPhase::Done, at(5000)), at(5000));
    assert!(reducer.typing_nicks(&key, at(5000)).is_empty(), "done clears");
}

#[test]
fn reducer_paused_typing_hint_holds_until_paused_deadline() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold_at(&mut reducer, join(NETWORK_A, "#omarchy", "omairc"), typing_t0());
    let key = reducer.conversation_key(NETWORK_A, "#omarchy");
    let alice_dm = reducer.conversation_key(NETWORK_A, "Alice");

    fold_at(&mut reducer, typing_event(&key, "bob", TypingPhase::Paused, typing_t0()), typing_t0());
    assert_eq!(reducer.typing_nicks(&key, at(0)), vec!["bob"], "paused at t0");
    assert_eq!(reducer.typing_nicks(&key, at(1000)), vec!["bob"], "paused at 1000");
    assert_eq!(reducer.typing_nicks(&key, at(29000)), vec!["bob"], "paused at 29000");
    assert!(reducer.typing_nicks(&key, at(30000)).is_empty(), "paused at 30000");
    assert!(
        state_of(&reducer, &key).typing.contains_key("bob"),
        "an expired hint must stay stored until pruned by a new hint"
    );

    fold_at(&mut reducer, typing_event(&key, "alice", TypingPhase::Active, at(29000)), at(29000));
    assert!(state_of(&reducer, &key).typing.contains_key("bob"), "bob must survive a prune at 29000");
    fold_at(&mut reducer, typing_event(&key, "alice", TypingPhase::Active, at(30001)), at(30001));
    assert!(!state_of(&reducer, &key).typing.contains_key("bob"), "bob must be pruned at 30001");

    fold(&mut reducer, message(&alice_dm, "Alice", "hi", "Alice"));
    fold_at(&mut reducer, typing_event(&alice_dm, "Alice", TypingPhase::Paused, typing_t0()), typing_t0());
    assert!(reducer.direct_peer_is_typing(&alice_dm, at(0)), "direct paused at t0");
    assert!(reducer.direct_peer_is_typing(&alice_dm, at(29999)), "direct paused at 29999");
    assert!(!reducer.direct_peer_is_typing(&alice_dm, at(30000)), "direct paused at 30000");

    fold_at(&mut reducer, typing_event(&alice_dm, "Alice", TypingPhase::Active, typing_t0()), typing_t0());
    assert!(reducer.direct_peer_is_typing(&alice_dm, at(0)), "direct active at t0");
    assert!(reducer.direct_peer_is_typing(&alice_dm, at(5999)), "direct active at 5999");
    assert!(!reducer.direct_peer_is_typing(&alice_dm, at(6000)), "direct active at 6000");
}

#[test]
fn reducer_clears_typing_on_chat_leave_and_quit() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold_at(&mut reducer, join(NETWORK_A, "#omarchy", "omairc"), typing_t0());
    fold_at(&mut reducer, join(NETWORK_A, "#desktop", "omairc"), typing_t0());
    let omarchy = reducer.conversation_key(NETWORK_A, "#omarchy");
    let desktop = reducer.conversation_key(NETWORK_A, "#desktop");
    fold_at(&mut reducer, typing_event(&omarchy, "alice", TypingPhase::Active, typing_t0()), typing_t0());
    fold_at(&mut reducer, typing_event(&desktop, "alice", TypingPhase::Active, typing_t0()), typing_t0());

    fold_at(&mut reducer, message(&omarchy, "alice", "here", "#omarchy"), typing_t0());
    assert!(reducer.typing_nicks(&omarchy, typing_t0()).is_empty(), "chat clears typing");
    assert_eq!(reducer.typing_nicks(&desktop, typing_t0()), vec!["alice"], "other channel keeps typing");

    fold_at(&mut reducer, typing_event(&omarchy, "alice", TypingPhase::Active, typing_t0()), typing_t0());
    fold_at(&mut reducer, part(NETWORK_A, "#omarchy", "alice"), typing_t0());
    assert!(reducer.typing_nicks(&omarchy, typing_t0()).is_empty(), "part clears typing");

    fold_at(&mut reducer, typing_event(&desktop, "bob", TypingPhase::Active, typing_t0()), typing_t0());
    fold_at(&mut reducer, quit(NETWORK_A, "alice"), typing_t0());
    assert_eq!(reducer.typing_nicks(&desktop, typing_t0()), vec!["bob"], "quit clears only alice");

    fold_at(&mut reducer, WelcomeEvent {
        network_id: NETWORK_A.to_owned(),
        current_nick: "omairc".to_owned(),
    }, typing_t0());
    assert!(reducer.typing_nicks(&desktop, typing_t0()).is_empty(), "welcome clears typing");
}

#[test]
fn reducer_remaps_typing_nick_including_direct_message() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold_at(&mut reducer, join(NETWORK_A, "#omarchy", "omairc"), typing_t0());
    let channel = reducer.conversation_key(NETWORK_A, "#omarchy");
    let alice_dm = reducer.conversation_key(NETWORK_A, "Alice");
    fold_at(&mut reducer, message(&alice_dm, "Alice", "hi", "Alice"), typing_t0());
    fold_at(&mut reducer, typing_event(&channel, "Alice", TypingPhase::Active, typing_t0()), typing_t0());
    fold_at(&mut reducer, typing_event(&alice_dm, "Alice", TypingPhase::Active, typing_t0()), typing_t0());

    fold_at(&mut reducer, nick_change(NETWORK_A, "Alice", "Alicia"), typing_t0());

    assert_eq!(reducer.typing_nicks(&channel, typing_t0()), vec!["Alicia"], "channel remapped");
    assert!(reducer.find(&alice_dm).is_none(), "old direct dropped");
    let alicia_dm = reducer.conversation_key(NETWORK_A, "Alicia");
    assert_eq!(reducer.typing_nicks(&alicia_dm, typing_t0()), vec!["Alicia"], "direct remapped");
}

#[test]
fn reducer_merges_typing_when_direct_messages_collide() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let alice_dm = reducer.conversation_key(NETWORK_A, "Alice");
    let alicia_dm = reducer.conversation_key(NETWORK_A, "Alicia");
    fold_at(&mut reducer, message(&alice_dm, "Alice", "hi", "Alice"), typing_t0());
    fold_at(&mut reducer, message(&alicia_dm, "Alicia", "yo", "Alicia"), typing_t0());
    fold_at(&mut reducer, typing_event(&alice_dm, "Alice", TypingPhase::Active, typing_t0()), typing_t0());

    fold_at(&mut reducer, nick_change(NETWORK_A, "Alice", "Alicia"), typing_t0());

    assert!(reducer.find(&alice_dm).is_none(), "old direct dropped");
    assert_eq!(reducer.typing_nicks(&alicia_dm, typing_t0()), vec!["Alicia"], "merged typing");
}

#[test]
fn reducer_hides_self_typing_and_skips_missing_conversation() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let missing = reducer.conversation_key(NETWORK_A, "#omarchy");
    fold_at(&mut reducer, typing_event(&missing, "alice", TypingPhase::Active, typing_t0()), typing_t0());
    assert!(reducer.find(&missing).is_none(), "missing conversation not invented");
    assert!(reducer.typing_nicks(&missing, typing_t0()).is_empty(), "missing typing");

    fold_at(&mut reducer, join(NETWORK_A, "#omarchy", "omairc"), typing_t0());
    fold_at(&mut reducer, typing_event(&missing, "omairc", TypingPhase::Active, typing_t0()), typing_t0());
    assert!(reducer.typing_nicks(&missing, typing_t0()).is_empty(), "self typing hidden");
}

#[test]
fn reducer_direct_peer_is_typing_for_existing_direct_only() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold_at(&mut reducer, join(NETWORK_A, "#omarchy", "omairc"), typing_t0());
    let channel = reducer.conversation_key(NETWORK_A, "#omarchy");
    let alice_dm = reducer.conversation_key(NETWORK_A, "Alice");
    let ghost = reducer.conversation_key(NETWORK_A, "ghost");
    fold_at(&mut reducer, message(&alice_dm, "Alice", "hi", "Alice"), typing_t0());

    assert!(!reducer.direct_peer_is_typing(&channel, typing_t0()), "channel is not direct");
    assert!(!reducer.direct_peer_is_typing(&alice_dm, typing_t0()), "idle direct");
    assert!(!reducer.direct_peer_is_typing(&ghost, typing_t0()), "missing direct");
    assert!(reducer.find(&ghost).is_none(), "ghost not invented");

    fold_at(&mut reducer, typing_event(&alice_dm, "alice", TypingPhase::Active, typing_t0()), typing_t0());
    fold_at(&mut reducer, typing_event(&channel, "alice", TypingPhase::Active, typing_t0()), typing_t0());
    fold_at(&mut reducer, typing_event(&ghost, "ghost", TypingPhase::Active, typing_t0()), typing_t0());
    assert!(reducer.direct_peer_is_typing(&alice_dm, typing_t0()), "direct peer typing");
    assert!(!reducer.direct_peer_is_typing(&channel, typing_t0()), "channel peer is not a direct peer");
    assert!(!reducer.direct_peer_is_typing(&ghost, typing_t0()), "ghost peer typing");
    assert!(reducer.find(&ghost).is_none(), "ghost still absent");

    fold_at(&mut reducer, typing_event(&alice_dm, "Alice", TypingPhase::Paused, typing_t0()), typing_t0());
    assert!(reducer.direct_peer_is_typing(&alice_dm, typing_t0()), "paused peer typing");

    fold_at(&mut reducer, typing_event(&alice_dm, "Alice", TypingPhase::Done, typing_t0()), typing_t0());
    assert!(!reducer.direct_peer_is_typing(&alice_dm, typing_t0()), "done clears peer typing");

    fold_at(&mut reducer, typing_event(&alice_dm, "Alice", TypingPhase::Active, typing_t0()), typing_t0());
    assert!(reducer.direct_peer_is_typing(&alice_dm, typing_t0()), "active peer typing");
    fold_at(&mut reducer, message(&alice_dm, "Alice", "here", "Alice"), typing_t0());
    assert!(!reducer.direct_peer_is_typing(&alice_dm, typing_t0()), "chat clears peer typing");
}
