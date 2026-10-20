//! Sidebar order and the per-network collapse, move, and focus state.
//!
//! Ports Qt's `ircSidebarOrder` (per network: channels, then direct messages,
//! each case-insensitively by name) and the `IrcConnection` collapse and move
//! setters. Pure data: the UI reads `sidebar_rows` and owns the scroll.

use std::cmp::Ordering;
use std::collections::HashSet;

use super::ids::ConversationRef;
use super::mock::{Conversation, MockWorld, Network};

/// What the sidebar remembers between chords. Focus is a network id, so it
/// survives the network being moved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarState {
    pub collapsed: HashSet<String>,
    pub focus: Option<String>,
    pub rail_visible: bool,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            collapsed: HashSet::new(),
            focus: None,
            rail_visible: true,
        }
    }
}

impl SidebarState {
    pub fn is_collapsed(&self, network_id: &str) -> bool {
        self.collapsed.contains(network_id)
    }
}

/// One conversation in sidebar order. `collapsed` is the owning network's
/// state at the time the rows were built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarRow {
    pub conversation: ConversationRef,
    pub name: String,
    pub channel: bool,
    pub collapsed: bool,
}

/// Every conversation in sidebar order, collapsed networks included. Callers
/// that walk only visible rows filter on `collapsed`.
pub fn sidebar_rows(world: &MockWorld, state: &SidebarState) -> Vec<SidebarRow> {
    let mut rows = Vec::new();
    for network in &world.networks {
        let collapsed = state.is_collapsed(&network.id);
        for conversation in ordered_conversations(network) {
            rows.push(SidebarRow {
                conversation: ConversationRef::new(network.id.clone(), conversation.key()),
                name: conversation.name.clone(),
                channel: conversation.channel,
                collapsed,
            });
        }
    }
    rows
}

/// One network's conversations in sidebar order: channels, then direct
/// messages, each case-insensitively by name (Qt `ircSidebarOrder`).
pub fn ordered_conversations(network: &Network) -> Vec<&Conversation> {
    let mut conversations: Vec<&Conversation> = network.conversations.iter().collect();
    conversations.sort_by(|left, right| conversation_order(left, right));
    conversations
}

fn conversation_order(left: &Conversation, right: &Conversation) -> Ordering {
    // Channels sort before direct messages (`false` < `true` after negation).
    (!left.channel)
        .cmp(&!right.channel)
        .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        .then_with(|| left.name.cmp(&right.name))
}

/// Qt `setNetworkCollapsed`. Unknown networks are ignored. Returns whether
/// the state changed.
pub fn set_collapsed(
    world: &MockWorld,
    state: &mut SidebarState,
    network_id: &str,
    collapsed: bool,
) -> bool {
    if world.network(network_id).is_none() {
        return false;
    }
    if collapsed {
        state.collapsed.insert(network_id.to_string())
    } else {
        state.collapsed.remove(network_id)
    }
}

/// Collapses or restores the focused network. Returns whether anything changed.
pub fn set_collapsed_focused(world: &MockWorld, state: &mut SidebarState, collapsed: bool) -> bool {
    match state.focus.clone() {
        Some(id) => set_collapsed(world, state, &id, collapsed),
        None => false,
    }
}

/// Qt `setAllNetworksCollapsed`: collapsing marks every network, restoring
/// clears the set. Returns whether anything changed.
pub fn collapse_all(world: &MockWorld, state: &mut SidebarState, collapsed: bool) -> bool {
    let next: HashSet<String> = if collapsed {
        world.networks.iter().map(|n| n.id.clone()).collect()
    } else {
        HashSet::new()
    };
    if next == state.collapsed {
        return false;
    }
    state.collapsed = next;
    true
}

/// Qt `moveNetwork`: swaps the network `delta` places in the stored order.
/// No wrap; returns false at either edge.
pub fn move_network(world: &mut MockWorld, network_id: &str, delta: isize) -> bool {
    if delta == 0 {
        return false;
    }
    let Some(index) = world.networks.iter().position(|n| n.id == network_id) else {
        return false;
    };
    let next = index as isize + delta;
    if next < 0 || next >= world.networks.len() as isize {
        return false;
    }
    let network = world.networks.remove(index);
    world.networks.insert(next as usize, network);
    true
}

/// Moves the focused network. Focus stays on it, because focus is stored by id.
pub fn move_focused(world: &mut MockWorld, state: &SidebarState, delta: isize) -> bool {
    match state.focus.as_deref() {
        Some(id) => move_network(world, id, delta),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mock::load_world;

    fn labels(rows: &[SidebarRow]) -> Vec<String> {
        rows.iter()
            .map(|row| format!("{}/{}", row.conversation.network_id, row.name))
            .collect()
    }

    #[test]
    fn order_is_channels_then_directs_case_insensitively_per_network() {
        let world = load_world();
        let rows = sidebar_rows(&world, &SidebarState::default());
        let omarchy: Vec<_> = rows
            .iter()
            .filter(|r| r.conversation.network_id == "omarchy")
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(
            omarchy,
            ["#desktop", "#help", "#omarchy", "#ricing", "anna", "dax"]
        );
    }

    #[test]
    fn networks_keep_world_order_and_rows_carry_collapse_state() {
        let world = load_world();
        let mut state = SidebarState::default();
        state.collapsed.insert("oftc".to_string());
        let rows = sidebar_rows(&world, &state);
        let first_oftc = rows.iter().position(|r| r.conversation.network_id == "oftc").unwrap();
        assert!(rows[..first_oftc].iter().all(|r| r.conversation.network_id == "omarchy"));
        assert!(rows[first_oftc..].iter().all(|r| r.collapsed));
        assert!(rows[..first_oftc].iter().all(|r| !r.collapsed));
        assert!(labels(&rows).contains(&"oftc/#build".to_string()));
    }

    #[test]
    fn rows_key_channels_by_lowercase_name() {
        let world = load_world();
        let rows = sidebar_rows(&world, &SidebarState::default());
        assert!(rows.iter().any(|r| r.conversation == ConversationRef::new("oftc", "#omarchy")));
    }

    #[test]
    fn collapse_ignores_unknown_networks_and_reports_change() {
        let world = load_world();
        let mut state = SidebarState::default();
        assert!(!set_collapsed(&world, &mut state, "nope", true));
        assert!(set_collapsed(&world, &mut state, "oftc", true));
        assert!(!set_collapsed(&world, &mut state, "oftc", true));
        assert!(state.is_collapsed("oftc"));
        assert!(set_collapsed(&world, &mut state, "oftc", false));
        assert!(!state.is_collapsed("oftc"));
    }

    #[test]
    fn collapse_focused_needs_focus() {
        let world = load_world();
        let mut state = SidebarState::default();
        assert!(!set_collapsed_focused(&world, &mut state, true));
        state.focus = Some("omarchy".to_string());
        assert!(set_collapsed_focused(&world, &mut state, true));
        assert!(state.is_collapsed("omarchy"));
    }

    #[test]
    fn collapse_all_marks_every_network_and_restores() {
        let world = load_world();
        let mut state = SidebarState::default();
        assert!(collapse_all(&world, &mut state, true));
        assert!(state.is_collapsed("omarchy") && state.is_collapsed("oftc"));
        assert!(!collapse_all(&world, &mut state, true));
        assert!(collapse_all(&world, &mut state, false));
        assert!(state.collapsed.is_empty());
    }

    #[test]
    fn move_swaps_neighbours_without_wrapping() {
        let mut world = load_world();
        assert!(!move_network(&mut world, "omarchy", -1));
        assert!(!move_network(&mut world, "omarchy", 0));
        assert!(move_network(&mut world, "omarchy", 1));
        assert_eq!(world.networks[0].id, "oftc");
        assert_eq!(world.networks[1].id, "omarchy");
        assert!(!move_network(&mut world, "omarchy", 1));
        assert!(!move_network(&mut world, "missing", 1));
    }

    #[test]
    fn move_focused_keeps_focus_on_the_moved_network() {
        let mut world = load_world();
        let mut state = SidebarState::default();
        assert!(!move_focused(&mut world, &state, 1));
        state.focus = Some("omarchy".to_string());
        assert!(move_focused(&mut world, &state, 1));
        assert_eq!(state.focus.as_deref(), Some("omarchy"));
        assert_eq!(world.networks[1].id, "omarchy");
        let rows = sidebar_rows(&world, &state);
        assert_eq!(rows[0].conversation.network_id, "oftc");
    }
}
