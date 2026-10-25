//! The sidebar's conversation order: networks in the order the caller gives (or
//! sorted), then within a network the channels, then the direct messages, each
//! group by case-insensitive target and finally by key.
//!
//! Port of `tui/internal/irc/conversation.go` `SidebarOrder` /
//! `SidebarOrderWithNetworks` and its helpers (Go: `networkRank`, `groupRank`,
//! `compareCaseInsensitive`), and of `ircSidebarOrder` in
//! `src/irc/irceventreducer.cpp` and `conversationlistmodel.cpp` (C++).

use std::cmp::Ordering;

use crate::irc::event::ConversationKey;

use super::state::ConversationState;
use super::EventReducer;

/// A network's place in the caller's order. A network the order does not name
/// sorts after every one it does (`networkRank`).
fn network_rank(network_id: &str, order: &[String]) -> usize {
    order
        .iter()
        .position(|candidate| candidate == network_id)
        .unwrap_or(order.len())
}

/// Channels first (0), then direct messages (2). A channel the user has left stays
/// with the other channels so the name sort keeps its place (`groupRank`).
fn group_rank(conversation: Option<&ConversationState>) -> u8 {
    match conversation {
        Some(conversation) if conversation.is_channel() => 0,
        _ => 2,
    }
}

impl EventReducer {
    /// Every conversation in sidebar order, with the networks sorted by id
    /// (`SidebarOrder`).
    #[cfg(test)]
    pub fn sidebar_order(&self) -> Vec<ConversationKey> {
        self.sidebar_order_with_networks(&[])
    }

    /// Every conversation in sidebar order. An empty `network_order` builds one
    /// from the conversation keys, sorted; otherwise the given order wins
    /// (`SidebarOrderWithNetworks`).
    pub fn sidebar_order_with_networks(&self, network_order: &[String]) -> Vec<ConversationKey> {
        let mut order: Vec<String> = network_order.to_vec();
        if order.is_empty() {
            for key in self.conversations.keys() {
                if !order.contains(&key.network_id) {
                    order.push(key.network_id.clone());
                }
            }
            order.sort();
        }
        let mut keys: Vec<ConversationKey> = self.conversations.keys().cloned().collect();
        keys.sort_by(|left, right| {
            let left_state = self.conversations.get(left);
            let right_state = self.conversations.get(right);
            network_rank(&left.network_id, &order)
                .cmp(&network_rank(&right.network_id, &order))
                .then_with(|| left.network_id.cmp(&right.network_id))
                .then_with(|| group_rank(left_state).cmp(&group_rank(right_state)))
                .then_with(|| {
                    let left_target = left_state.map_or(left.normalized_target.as_str(), |state| {
                        state.target.as_str()
                    });
                    let right_target = right_state.map_or(right.normalized_target.as_str(), |state| {
                        state.target.as_str()
                    });
                    compare_case_insensitive(left_target, right_target)
                })
                .then_with(|| left.cmp(right))
        });
        keys
    }
}

/// Orders two display targets the way `QString::compare(CaseInsensitive)` does
/// for ASCII: folded, then compared (`compareCaseInsensitive`).
fn compare_case_insensitive(left: &str, right: &str) -> Ordering {
    left.to_lowercase().cmp(&right.to_lowercase())
}
