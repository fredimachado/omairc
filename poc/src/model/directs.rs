//! Direct messages: open from a nick (nick sheet, member list, transcript
//! author) and close with Ctrl+W. Ports Qt `openDirectMessage` and
//! `closeDirectMessage`. Pure data; the caller selects the result.

use super::ids::ConversationRef;
use super::mock::{Conversation, MockWorld, Network};
use super::navigation::neighbor_after_close;
use super::sidebar::{sidebar_rows, SidebarState};

/// Opens a DM with `nick` on `network_id`.
///
/// An existing DM is selected: its unread count and mention flag clear. A new
/// one is created empty, with the topic `Direct message with <nick>` and the
/// real name of that nick from the network's member lists. Returns `None` for
/// an empty nick, the network's own nick (exact, case-sensitive), a
/// channel-looking nick, or an unknown network.
///
/// The channel check uses the RFC 2812 default prefixes. The Qt client asks
/// `IrcServerFeatures` instead; this model has no server features yet.
pub fn open_direct(world: &mut MockWorld, network_id: &str, nick: &str) -> Option<ConversationRef> {
    if nick.is_empty() || looks_like_channel(nick) {
        return None;
    }
    let network = world.networks.iter_mut().find(|n| n.id == network_id)?;
    if nick == network.nick {
        return None;
    }
    if let Some(existing) = network
        .conversations
        .iter_mut()
        .find(|c| !c.channel && c.name.eq_ignore_ascii_case(nick))
    {
        existing.mark_read();
        return Some(ConversationRef::new(network_id, existing.key()));
    }
    let peer_realname = member_realname(network, nick);
    network.conversations.push(Conversation {
        name: nick.to_string(),
        channel: false,
        topic: Some(format!("Direct message with {nick}")),
        peer_realname,
        unread: 0,
        mention: false,
        typing: false,
        members: Vec::new(),
        messages: Vec::new(),
    });
    Some(ConversationRef::new(network_id, nick))
}

/// Ctrl+W on a DM: removes it. The outer `None` means `target` is not an open
/// DM, so nothing happens. The inner value is the row to select next, the
/// same as `neighbor_after_close` on the sidebar order before the removal.
pub fn close_direct(
    world: &mut MockWorld,
    state: &SidebarState,
    target: &ConversationRef,
) -> Option<Option<ConversationRef>> {
    if world.find(target)?.channel {
        return None;
    }
    let rows = sidebar_rows(world, state);
    let next = neighbor_after_close(&rows, target);
    let network = world.networks.iter_mut().find(|n| n.id == target.network_id)?;
    network
        .conversations
        .retain(|c| c.channel || c.key() != target.key);
    Some(next)
}

fn looks_like_channel(nick: &str) -> bool {
    matches!(nick.chars().next(), Some('#' | '&' | '+' | '!'))
}

fn member_realname(network: &Network, nick: &str) -> Option<String> {
    network
        .conversations
        .iter()
        .flat_map(|c| &c.members)
        .find(|m| m.nick == nick)
        .map(|m| m.realname.clone())
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mock::load_world;

    fn omarchy(key: &str) -> ConversationRef {
        ConversationRef::new("omarchy", key)
    }

    #[test]
    fn nick_sheet_mira_creates_a_direct_message_on_the_selected_network() {
        let mut world = load_world();
        let opened = open_direct(&mut world, "omarchy", "mira");
        assert_eq!(opened, Some(omarchy("mira")));
        let dm = world.find(&omarchy("mira")).expect("mira dm");
        assert!(!dm.channel);
        assert!(dm.messages.is_empty());
        assert_eq!(dm.topic.as_deref(), Some("Direct message with mira"));
        assert_eq!(dm.peer_realname.as_deref(), Some("Mira Chen"));
    }

    #[test]
    fn a_new_direct_message_sorts_after_the_existing_ones() {
        let mut world = load_world();
        open_direct(&mut world, "omarchy", "mira");
        let rows = sidebar_rows(&world, &SidebarState::default());
        let omarchy_rows: Vec<_> = rows
            .iter()
            .filter(|r| r.conversation.network_id == "omarchy")
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(
            omarchy_rows,
            ["#desktop", "#help", "#omarchy", "#ricing", "anna", "dax", "mira"]
        );
    }

    #[test]
    fn opening_an_existing_direct_message_only_selects_it_and_reads_it() {
        let mut world = load_world();
        let before = world.networks[0].conversations.len();
        let anna = open_direct(&mut world, "omarchy", "anna");
        assert_eq!(anna, Some(omarchy("anna")));
        assert_eq!(world.networks[0].conversations.len(), before);
        let conv = world.find(&omarchy("anna")).expect("anna");
        assert_eq!(conv.unread, 0);
        assert!(!conv.mention);
    }

    #[test]
    fn a_direct_message_is_per_network() {
        let mut world = load_world();
        // rio exists only on oftc; opening it on omarchy creates a new one.
        assert_eq!(
            open_direct(&mut world, "omarchy", "rio"),
            Some(omarchy("rio"))
        );
        assert!(world.find(&ConversationRef::new("oftc", "rio")).is_some());
        assert_eq!(world.find(&omarchy("rio")).expect("omarchy rio").peer_realname, None);
    }

    #[test]
    fn self_nick_is_a_no_op() {
        let mut world = load_world();
        let before = world.networks[0].conversations.len();
        assert_eq!(open_direct(&mut world, "omarchy", "fred"), None);
        assert_eq!(world.networks[0].conversations.len(), before);
        // Self is exact and case-sensitive: a different case is another nick.
        assert_eq!(open_direct(&mut world, "omarchy", "Fred"), Some(omarchy("Fred")));
    }

    #[test]
    fn channel_looking_or_empty_nick_is_a_no_op() {
        let mut world = load_world();
        assert_eq!(open_direct(&mut world, "omarchy", "#ricing"), None);
        assert_eq!(open_direct(&mut world, "omarchy", "&local"), None);
        assert_eq!(open_direct(&mut world, "omarchy", ""), None);
        assert_eq!(open_direct(&mut world, "nope", "mira"), None);
    }

    #[test]
    fn ctrl_w_on_mira_selects_dax() {
        let mut world = load_world();
        open_direct(&mut world, "omarchy", "mira");
        let next = close_direct(&mut world, &SidebarState::default(), &omarchy("mira"));
        assert_eq!(next, Some(Some(omarchy("dax"))));
        assert!(world.find(&omarchy("mira")).is_none());
    }

    #[test]
    fn ctrl_w_on_a_channel_or_missing_row_does_nothing() {
        let mut world = load_world();
        let state = SidebarState::default();
        assert_eq!(close_direct(&mut world, &state, &omarchy("#ricing")), None);
        assert_eq!(close_direct(&mut world, &state, &omarchy("nobody")), None);
        assert!(world.find(&omarchy("#ricing")).is_some());
    }
}
