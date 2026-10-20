//! Conversation, network, and unread walks over `sidebar_rows`. Ports Qt's
//! `stepConversation`, `stepNetwork`, `jumpToNextUnread`, and
//! `ircNeighborAfterDrop`. Pure functions: callers apply the result.

use super::ids::ConversationRef;
use super::mock::MockWorld;
use super::sidebar::SidebarRow;

/// Next visible conversation from `current` (which may be hidden in a
/// collapsed network), skipping collapsed rows and wrapping. `None` when no
/// row is visible or `delta` is zero.
pub fn step_conversation(
    rows: &[SidebarRow],
    current: Option<&ConversationRef>,
    delta: isize,
) -> Option<ConversationRef> {
    if delta == 0 || rows.iter().all(|row| row.collapsed) {
        return None;
    }
    let count = rows.len() as isize;
    let start = match current.and_then(|c| rows.iter().position(|row| &row.conversation == c)) {
        Some(index) => index as isize,
        None if delta > 0 => -1,
        None => count,
    };
    for step in 1..=count {
        let index = (start + delta * step).rem_euclid(count) as usize;
        if rows[index].collapsed {
            continue;
        }
        return Some(rows[index].conversation.clone());
    }
    None
}

/// Next network in stored order, from the focused network if any, else the
/// current one. Wraps. Returns the network id to focus.
pub fn step_network(
    world: &MockWorld,
    focus: Option<&str>,
    current_network: Option<&str>,
    delta: isize,
) -> Option<String> {
    let count = world.networks.len();
    if count == 0 || delta == 0 {
        return None;
    }
    let position = |id: Option<&str>| {
        id.and_then(|id| world.networks.iter().position(|network| network.id == id))
    };
    let next = match position(focus).or_else(|| position(current_network)) {
        Some(index) => (index as isize + delta).rem_euclid(count as isize) as usize,
        None if delta > 0 => 0,
        None => count - 1,
    };
    Some(world.networks[next].id.clone())
}

/// Alt+A: the first mention after `current` (wrapping, skipping `current`),
/// else the first unread. Collapsed networks count; the caller expands the
/// network it lands in. `None` when nothing is unread.
pub fn next_unread(
    world: &MockWorld,
    rows: &[SidebarRow],
    current: Option<&ConversationRef>,
) -> Option<ConversationRef> {
    let count = rows.len();
    if count == 0 {
        return None;
    }
    let current_index = current.and_then(|c| rows.iter().position(|row| &row.conversation == c));
    let start = current_index.map_or(0, |index| (index + 1) % count);
    let mut unread = None;
    for step in 0..count {
        let index = (start + step) % count;
        if Some(index) == current_index {
            continue;
        }
        let row = &rows[index];
        let Some(conversation) = world.find(&row.conversation) else {
            continue;
        };
        if conversation.mention {
            return Some(row.conversation.clone());
        }
        if unread.is_none() && conversation.unread > 0 {
            unread = Some(row.conversation.clone());
        }
    }
    unread
}

/// Alt+Shift+A: every conversation reads. The selection does not move.
pub fn mark_all_read(world: &mut MockWorld) {
    for network in &mut world.networks {
        for conversation in &mut network.conversations {
            conversation.mark_read();
        }
    }
}

/// Qt `ircNeighborAfterDrop`: the row to select after `dropping` leaves.
/// Prefers the next row in the same network, then the previous one, then any
/// neighbour. A row that is not in `ordered` falls back to the last row.
pub fn neighbor_after_close(
    ordered: &[SidebarRow],
    dropping: &ConversationRef,
) -> Option<ConversationRef> {
    let count = ordered.len();
    let Some(index) = ordered.iter().position(|row| &row.conversation == dropping) else {
        return ordered.last().map(|row| row.conversation.clone());
    };
    let same_network = |other: usize| ordered[other].conversation.network_id == dropping.network_id;
    let pick = if index + 1 < count && same_network(index + 1) {
        index + 1
    } else if index > 0 && same_network(index - 1) {
        index - 1
    } else if index + 1 < count {
        index + 1
    } else if index > 0 {
        index - 1
    } else {
        return None;
    };
    Some(ordered[pick].conversation.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mock::load_world;
    use crate::model::sidebar::{sidebar_rows, SidebarState};

    fn omarchy(key: &str) -> ConversationRef {
        ConversationRef::new("omarchy", key)
    }

    #[test]
    fn fresh_alt_down_from_omarchy_lands_on_ricing() {
        let world = load_world();
        let rows = sidebar_rows(&world, &SidebarState::default());
        let next = step_conversation(&rows, Some(&omarchy("#omarchy")), 1);
        assert_eq!(next, Some(omarchy("#ricing")));
    }

    #[test]
    fn alt_up_walks_backwards_and_wraps() {
        let world = load_world();
        let rows = sidebar_rows(&world, &SidebarState::default());
        assert_eq!(
            step_conversation(&rows, Some(&omarchy("#desktop")), -1),
            Some(ConversationRef::new("oftc", "rio"))
        );
        assert_eq!(
            step_conversation(&rows, Some(&ConversationRef::new("oftc", "rio")), 1),
            Some(omarchy("#desktop"))
        );
    }

    #[test]
    fn walk_skips_collapsed_networks_but_continues_from_a_hidden_current_row() {
        let world = load_world();
        let mut state = SidebarState::default();
        state.collapsed.insert("omarchy".to_string());
        let rows = sidebar_rows(&world, &state);
        // The current row sits in the collapsed network: walk still lands on
        // the first visible row after it in sidebar order.
        assert_eq!(
            step_conversation(&rows, Some(&omarchy("#ricing")), 1),
            Some(ConversationRef::new("oftc", "#build"))
        );
        // Walking back from #build skips the collapsed omarchy rows and the
        // wrap lands on rio, the last visible row.
        assert_eq!(
            step_conversation(&rows, Some(&ConversationRef::new("oftc", "#build")), -1),
            Some(ConversationRef::new("oftc", "rio"))
        );
    }

    #[test]
    fn walk_with_every_network_collapsed_is_a_no_op() {
        let world = load_world();
        let mut state = SidebarState::default();
        state.collapsed.insert("omarchy".to_string());
        state.collapsed.insert("oftc".to_string());
        let rows = sidebar_rows(&world, &state);
        assert_eq!(step_conversation(&rows, Some(&omarchy("#ricing")), 1), None);
    }

    #[test]
    fn walk_without_a_selection_starts_at_the_ends() {
        let world = load_world();
        let rows = sidebar_rows(&world, &SidebarState::default());
        assert_eq!(step_conversation(&rows, None, 1), Some(omarchy("#desktop")));
        assert_eq!(
            step_conversation(&rows, None, -1),
            Some(ConversationRef::new("oftc", "rio"))
        );
    }

    #[test]
    fn network_walk_wraps_and_prefers_focus_over_current() {
        let world = load_world();
        assert_eq!(step_network(&world, None, Some("omarchy"), 1), Some("oftc".into()));
        assert_eq!(step_network(&world, Some("oftc"), Some("omarchy"), 1), Some("omarchy".into()));
        assert_eq!(step_network(&world, Some("omarchy"), None, -1), Some("oftc".into()));
        assert_eq!(step_network(&world, None, None, -1), Some("oftc".into()));
        assert_eq!(step_network(&world, None, None, 1), Some("omarchy".into()));
    }

    #[test]
    fn alt_a_takes_the_first_mention_then_unread() {
        let world = load_world();
        let rows = sidebar_rows(&world, &SidebarState::default());
        // #ricing mentions fred; anna is the next mention once it is marked.
        assert_eq!(next_unread(&world, &rows, None), Some(omarchy("#ricing")));
        let mut world = world;
        world.find_mut(&omarchy("#ricing")).unwrap().mark_read();
        assert_eq!(next_unread(&world, &rows, Some(&omarchy("#ricing"))), Some(omarchy("anna")));
    }

    #[test]
    fn alt_a_falls_back_to_unread_and_then_to_nothing() {
        let mut world = load_world();
        let rows = sidebar_rows(&world, &SidebarState::default());
        // Clear the mentions so only plain unread rows remain.
        for network in &mut world.networks {
            for conversation in &mut network.conversations {
                conversation.mention = false;
            }
        }
        assert_eq!(next_unread(&world, &rows, Some(&omarchy("#ricing"))), Some(omarchy("anna")));
        mark_all_read(&mut world);
        assert_eq!(next_unread(&world, &rows, None), None);
    }

    #[test]
    fn mark_all_read_clears_every_conversation() {
        let mut world = load_world();
        mark_all_read(&mut world);
        assert!(world
            .networks
            .iter()
            .flat_map(|n| &n.conversations)
            .all(|c| c.unread == 0 && !c.mention));
    }

    #[test]
    fn neighbor_prefers_same_network_next_then_previous() {
        let world = load_world();
        let rows = sidebar_rows(&world, &SidebarState::default());
        // #ricing (omarchy) has #omarchy before it and anna after it: next wins.
        assert_eq!(neighbor_after_close(&rows, &omarchy("#ricing")), Some(omarchy("anna")));
        // dax is last in omarchy: previous in the same network.
        assert_eq!(neighbor_after_close(&rows, &omarchy("dax")), Some(omarchy("anna")));
        // rio is the last row overall with no next row, so the previous row
        // in the same network (#omarchy on oftc) wins.
        assert_eq!(
            neighbor_after_close(&rows, &ConversationRef::new("oftc", "rio")),
            Some(ConversationRef::new("oftc", "#omarchy"))
        );
    }

    #[test]
    fn neighbor_of_missing_row_is_the_last_row() {
        let world = load_world();
        let rows = sidebar_rows(&world, &SidebarState::default());
        assert_eq!(
            neighbor_after_close(&rows, &omarchy("#gone")),
            Some(ConversationRef::new("oftc", "rio"))
        );
        assert_eq!(neighbor_after_close(&[], &omarchy("#gone")), None);
    }
}
