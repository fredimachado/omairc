//! Conversation, network, and unread walks over `sidebar_rows`. Ports Qt's
//! `stepConversation`, `stepNetwork`, `jumpToNextUnread`, and
//! `ircNeighborAfterDrop`. Pure functions: callers apply the result. The rows and
//! the network order come from the controller, and the walks read nothing else.

use super::ids::ConversationRef;
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

/// Next network in sidebar order (`network_ids`), from the focused network if any,
/// else the current one. Wraps. Returns the network id to focus.
pub fn step_network(
    network_ids: &[String],
    focus: Option<&str>,
    current_network: Option<&str>,
    delta: isize,
) -> Option<String> {
    let count = network_ids.len();
    if count == 0 || delta == 0 {
        return None;
    }
    let position = |id: Option<&str>| {
        id.and_then(|id| network_ids.iter().position(|network| network == id))
    };
    let next = match position(focus).or_else(|| position(current_network)) {
        Some(index) => (index as isize + delta).rem_euclid(count as isize) as usize,
        None if delta > 0 => 0,
        None => count - 1,
    };
    Some(network_ids[next].clone())
}

/// Alt+A: the first mention after `current` (wrapping, skipping `current`),
/// else the first unread. Collapsed networks count; the caller expands the
/// network it lands in. `None` when nothing is unread.
pub fn next_unread(
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
        if row.mention {
            return Some(row.conversation.clone());
        }
        if unread.is_none() && row.unread > 0 {
            unread = Some(row.conversation.clone());
        }
    }
    unread
}

/// Where Alt+A lands: a conversation, or the Status pane of a network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnreadTarget {
    Conversation(ConversationRef),
    Status(String),
}

/// Alt+A with Qt's Status fallback (`jumpToNextUnread`). It lands where
/// `next_unread` does. When nothing is unread, it opens the Status pane of
/// `status_network`, which the caller passes as the focused network, else the
/// selected one. Empty sidebar rows do nothing, as in Qt. `None` when there is
/// nothing to land on.
pub fn next_unread_or_status(
    rows: &[SidebarRow],
    current: Option<&ConversationRef>,
    status_network: Option<&str>,
    network_ids: &[String],
) -> Option<UnreadTarget> {
    if rows.is_empty() {
        return None;
    }
    if let Some(conversation) = next_unread(rows, current) {
        return Some(UnreadTarget::Conversation(conversation));
    }
    let network_id = status_network?;
    if !network_ids.iter().any(|id| id == network_id) {
        return None;
    }
    Some(UnreadTarget::Status(network_id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::controller::Controller;
    use crate::controller::snapshots::ConversationRow;
    use crate::model::sidebar::sidebar_rows;

    fn rows_for(controller: &Controller, collapsed: &[&str]) -> Vec<SidebarRow> {
        let rows: Vec<ConversationRow> = controller.conversations();
        sidebar_rows(&rows, |id| collapsed.contains(&id))
    }

    fn omarchy(name: &str) -> ConversationRef {
        ConversationRef::new("omarchy", name)
    }

    #[test]
    fn alt_down_walks_the_visible_rows_and_wraps() {
        let controller = Controller::seeded_for_tests();
        let rows = rows_for(&controller, &[]);
        let next = step_conversation(&rows, Some(&omarchy("#omarchy")), 1);
        assert_eq!(next, Some(omarchy("#ricing")));
        let wrapped = step_conversation(&rows, Some(&omarchy("#ricing")), 1);
        assert_eq!(wrapped, Some(omarchy("anna")));
    }

    #[test]
    fn a_collapsed_network_is_skipped_by_the_walk() {
        let controller = Controller::seeded_for_tests();
        let rows = rows_for(&controller, &["omarchy"]);
        let next = step_conversation(&rows, Some(&ConversationRef::new("oftc", "#lab")), -1);
        assert_eq!(next.map(|c| c.network_id), Some("oftc".to_string()));
        assert!(step_conversation(&rows, None, 1)
            .is_some_and(|c| c.network_id == "oftc"));
    }

    #[test]
    fn every_network_collapsed_leaves_nothing_to_walk() {
        let controller = Controller::seeded_for_tests();
        let rows = rows_for(&controller, &["omarchy", "oftc"]);
        assert_eq!(step_conversation(&rows, None, 1), None);
    }

    #[test]
    fn network_walk_wraps_over_the_sidebar_order() {
        let ids = ["omarchy".to_string(), "oftc".to_string()];
        assert_eq!(step_network(&ids, None, Some("omarchy"), 1), Some("oftc".into()));
        assert_eq!(step_network(&ids, Some("oftc"), None, 1), Some("omarchy".into()));
        assert_eq!(step_network(&ids, None, None, -1), Some("oftc".into()));
        assert_eq!(step_network(&[], None, None, 1), None);
        assert_eq!(step_network(&ids, None, Some("omarchy"), 0), None);
    }

    #[test]
    fn alt_a_prefers_a_mention_then_takes_the_first_unread() {
        let controller = Controller::seeded_for_tests();
        let rows = rows_for(&controller, &[]);
        assert_eq!(next_unread(&rows, None), Some(omarchy("#ricing")));
    }

    #[test]
    fn with_nothing_unread_alt_a_opens_the_status_of_the_focused_network() {
        let mut controller = Controller::seeded_for_tests();
        controller.mark_all_read();
        let rows = rows_for(&controller, &[]);
        let networks = controller.network_order();
        assert_eq!(next_unread(&rows, None), None);
        assert_eq!(
            next_unread_or_status(&rows, None, Some("oftc"), &networks),
            Some(UnreadTarget::Status("oftc".to_string()))
        );
        assert_eq!(next_unread_or_status(&rows, None, Some("nope"), &networks), None);
    }

    #[test]
    fn an_empty_sidebar_does_nothing_at_all() {
        let networks = ["omarchy".to_string()];
        assert_eq!(next_unread_or_status(&[], None, Some("omarchy"), &networks), None);
    }
}
