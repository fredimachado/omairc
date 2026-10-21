//! The sidebar order: networks first, then channels, then direct messages, each
//! group by case-insensitive target. Its own check, since `reducer_test.go` has
//! none for the order.

use super::*;

#[test]
fn sidebar_order_groups_networks_channels_then_direct_messages() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, "b");
    welcome_omairc(&mut reducer, "a");
    fold(&mut reducer, join("b", "#x", "omairc"));
    fold(&mut reducer, join("a", "#zz", "omairc"));
    fold(&mut reducer, join("a", "#one", "omairc"));
    fold(&mut reducer, part("a", "#zz", "omairc"));
    let alice = reducer.conversation_key("a", "alice");
    fold(&mut reducer, message(&alice, "alice", "hi", "alice"));

    let expected = vec![
        ConversationKey::new("a", "#one"),
        ConversationKey::new("a", "#zz"),
        ConversationKey::new("a", "alice"),
        ConversationKey::new("b", "#x"),
    ];
    assert_eq!(reducer.sidebar_order(), expected, "networks sorted, channels before directs");

    let reversed = reducer.sidebar_order_with_networks(&["b".to_owned(), "a".to_owned()]);
    assert_eq!(reversed.first(), Some(&ConversationKey::new("b", "#x")), "the given network order wins");
}
