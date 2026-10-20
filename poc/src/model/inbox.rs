//! The session inbox: mentions, invites, and other things waiting for the
//! user. Port of `IrcInbox` (consume and append rules) and
//! `IrcInboxModel::labelFor`. Newest item first.

use super::ids::ConversationRef;
use super::mock::{Conversation, MockWorld};

/// Qt `IrcInbox::kMaxItems`.
pub const INBOX_CAP: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboxKind {
    Mention,
    Highlight,
    Direct,
    Invite,
    MonitorOnline,
    Kick,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InboxItem {
    pub kind: InboxKind,
    pub network_id: String,
    pub actor: String,
    pub target: String,
    pub preview: String,
}

impl InboxItem {
    /// The one-line text the inbox row shows.
    pub fn label(&self) -> String {
        match self.kind {
            InboxKind::Mention => format!("{} mentioned you in {}", self.actor, self.target),
            InboxKind::Highlight => format!("Highlight in {}", self.target),
            InboxKind::Direct => format!("Message from {}", self.actor),
            InboxKind::Invite => format!("{} invited you to {}", self.actor, self.target),
            InboxKind::MonitorOnline => format!("{} is online", self.actor),
            InboxKind::Kick => format!("You were kicked from {}", self.target),
        }
    }

    /// Mention, highlight, direct, and kick rows belong to a conversation and
    /// clear when it is selected. Invites and presence rows do not.
    fn is_conversation_kind(&self) -> bool {
        matches!(
            self.kind,
            InboxKind::Mention | InboxKind::Highlight | InboxKind::Direct | InboxKind::Kick
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inbox {
    items: Vec<InboxItem>,
}

impl Inbox {
    pub fn items(&self) -> &[InboxItem] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Adds a new item at the top. A repeated invite to the same channel, or a
    /// repeated online notice for the same nick, replaces the older row.
    pub fn push(&mut self, item: InboxItem) {
        let replaces = |old: &InboxItem| {
            old.network_id == item.network_id
                && match item.kind {
                    InboxKind::Invite => {
                        old.kind == InboxKind::Invite && same_name(&old.target, &item.target)
                    }
                    InboxKind::MonitorOnline => {
                        old.kind == InboxKind::MonitorOnline && same_name(&old.actor, &item.actor)
                    }
                    _ => false,
                }
        };
        if let Some(index) = self.items.iter().position(replaces) {
            self.items.remove(index);
        }
        self.items.insert(0, item);
        self.items.truncate(INBOX_CAP);
    }

    /// Enter on a row: removes it and hands it back so the caller can act on it.
    /// Drops every row for a network the Connect sheet removed.
    pub fn drop_network(&mut self, network_id: &str) {
        self.items.retain(|item| item.network_id != network_id);
    }

    pub fn activate(&mut self, index: usize) -> Option<InboxItem> {
        (index < self.items.len()).then(|| self.items.remove(index))
    }

    /// The delete button: removes the row without acting on it.
    pub fn dismiss(&mut self, index: usize) -> bool {
        if index < self.items.len() {
            self.items.remove(index);
            true
        } else {
            false
        }
    }

    /// Selecting a conversation clears its conversation rows. A non-channel
    /// target also clears the presence row for that nick, as Qt does.
    pub fn consume_conversation(&mut self, network_id: &str, key: &str, is_channel: bool) {
        self.items.retain(|item| {
            let same_network = item.network_id == network_id;
            let clears_conversation = item.is_conversation_kind() && same_name(&item.target, key);
            let clears_presence =
                !is_channel && item.kind == InboxKind::MonitorOnline && same_name(&item.actor, key);
            !(same_network && (clears_conversation || clears_presence))
        });
    }

    /// Joining a channel clears its invite row.
    pub fn consume_invite(&mut self, network_id: &str, channel: &str) {
        self.items.retain(|item| {
            !(item.kind == InboxKind::Invite
                && item.network_id == network_id
                && same_name(&item.target, channel))
        });
    }
}

/// IRC names compare case-insensitively. ASCII folding stands in for the
/// server's CASEMAPPING here, which is enough for the static seed.
fn same_name(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

/// The rows the demo starts with, newest first: a mention in `#ricing` on
/// Omarchy and an invite to `#debian` on OFTC. A row is only seeded when its
/// network or conversation exists in the world.
pub fn seed_inbox(world: &MockWorld) -> Inbox {
    let mut items = Vec::new();
    if world
        .find(&ConversationRef::new("omarchy", "#ricing"))
        .is_some()
    {
        items.push(InboxItem {
            kind: InboxKind::Mention,
            network_id: "omarchy".to_string(),
            actor: "mira".to_string(),
            target: "#ricing".to_string(),
            preview: String::new(),
        });
    }
    if world.network("oftc").is_some() {
        items.push(InboxItem {
            kind: InboxKind::Invite,
            network_id: "oftc".to_string(),
            actor: "rio".to_string(),
            target: "#debian".to_string(),
            preview: String::new(),
        });
    }
    Inbox { items }
}

/// The topic a channel has when an invite joins it. The demo server echoes
/// the same topic on its JOIN; this covers the one invite the seed offers.
pub fn demo_topic(network_id: &str, channel: &str) -> Option<&'static str> {
    match (network_id, channel.to_ascii_lowercase().as_str()) {
        ("oftc", "#debian") => Some("Debian GNU/Linux."),
        _ => None,
    }
}

/// The channel row `name` names on `network_id`, if the network has one. Channel
/// names match ignoring ASCII case, and a direct message never matches.
pub fn find_channel(world: &MockWorld, network_id: &str, name: &str) -> Option<ConversationRef> {
    let network = world.network(network_id)?;
    network
        .conversations
        .iter()
        .find(|c| c.channel && c.name.eq_ignore_ascii_case(name))
        .map(|c| ConversationRef::new(network_id, c.key()))
}

/// Accepting an invite: adds the channel to `network_id` unless it is already
/// there, with the demo topic, then returns its reference. `None` when the
/// network is unknown.
pub fn join_channel(world: &mut MockWorld, network_id: &str, channel: &str) -> Option<ConversationRef> {
    let topic = demo_topic(network_id, channel).map(str::to_string);
    let network = world.networks.iter_mut().find(|n| n.id == network_id)?;
    let existing = network
        .conversations
        .iter()
        .any(|c| c.channel && c.name.eq_ignore_ascii_case(channel));
    if !existing {
        network.conversations.push(Conversation {
            name: channel.to_string(),
            channel: true,
            topic,
            peer_realname: None,
            unread: 0,
            mention: false,
            typing: false,
            members: Vec::new(),
            messages: Vec::new(),
        });
    }
    Some(ConversationRef::new(network_id, channel.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mock::load_world;

    fn item(kind: InboxKind, network: &str, actor: &str, target: &str) -> InboxItem {
        InboxItem {
            kind,
            network_id: network.to_string(),
            actor: actor.to_string(),
            target: target.to_string(),
            preview: String::new(),
        }
    }

    fn labels(inbox: &Inbox) -> Vec<String> {
        inbox.items().iter().map(InboxItem::label).collect()
    }

    #[test]
    fn labels_match_qt_wording() {
        assert_eq!(
            item(InboxKind::Mention, "omarchy", "mira", "#ricing").label(),
            "mira mentioned you in #ricing"
        );
        assert_eq!(
            item(InboxKind::Highlight, "omarchy", "anna", "#help").label(),
            "Highlight in #help"
        );
        assert_eq!(
            item(InboxKind::Direct, "omarchy", "dax", "dax").label(),
            "Message from dax"
        );
        assert_eq!(
            item(InboxKind::Invite, "oftc", "rio", "#debian").label(),
            "rio invited you to #debian"
        );
        assert_eq!(
            item(InboxKind::MonitorOnline, "oftc", "kai", "").label(),
            "kai is online"
        );
        assert_eq!(
            item(InboxKind::Kick, "oftc", "op", "#lab").label(),
            "You were kicked from #lab"
        );
    }

    #[test]
    fn seed_lists_the_mention_then_the_invite() {
        let inbox = seed_inbox(&load_world());
        assert_eq!(
            labels(&inbox),
            [
                "mira mentioned you in #ricing",
                "rio invited you to #debian"
            ]
        );
    }

    #[test]
    fn activate_removes_and_returns_the_row() {
        let mut inbox = seed_inbox(&load_world());
        let taken = inbox.activate(0).expect("row 0 exists");
        assert_eq!(taken.kind, InboxKind::Mention);
        assert_eq!(labels(&inbox), ["rio invited you to #debian"]);
        assert!(inbox.activate(5).is_none());
    }

    #[test]
    fn dismiss_removes_without_acting() {
        let mut inbox = seed_inbox(&load_world());
        assert!(inbox.dismiss(1));
        assert_eq!(labels(&inbox), ["mira mentioned you in #ricing"]);
        assert!(!inbox.dismiss(1));
    }

    #[test]
    fn removing_a_network_drops_only_its_rows() {
        let mut inbox = Inbox::default();
        inbox.push(item(InboxKind::Mention, "omarchy", "mira", "#ricing"));
        inbox.push(item(InboxKind::Invite, "oftc", "rio", "#debian"));
        inbox.drop_network("oftc");
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox.items()[0].network_id, "omarchy");
        inbox.drop_network("nowhere");
        assert_eq!(inbox.len(), 1);
    }

    #[test]
    fn selecting_a_conversation_consumes_its_rows_case_insensitively() {
        let mut inbox = Inbox::default();
        inbox.push(item(InboxKind::Mention, "omarchy", "mira", "#Ricing"));
        inbox.push(item(InboxKind::Highlight, "omarchy", "anna", "#ricing"));
        inbox.push(item(InboxKind::Mention, "oftc", "mira", "#ricing"));
        inbox.consume_conversation("omarchy", "#ricing", true);
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox.items()[0].network_id, "oftc");
    }

    #[test]
    fn invites_survive_a_conversation_selection() {
        let mut inbox = Inbox::default();
        inbox.push(item(InboxKind::Invite, "oftc", "rio", "#debian"));
        inbox.consume_conversation("oftc", "#debian", true);
        assert_eq!(inbox.len(), 1);
    }

    #[test]
    fn presence_rows_clear_only_for_a_direct_message_target() {
        let mut inbox = Inbox::default();
        inbox.push(item(InboxKind::MonitorOnline, "oftc", "kai", ""));
        inbox.consume_conversation("oftc", "#lab", true);
        assert_eq!(inbox.len(), 1, "a channel selection leaves presence rows");
        inbox.consume_conversation("oftc", "Kai", false);
        assert!(inbox.is_empty());
    }

    #[test]
    fn joining_a_channel_clears_its_invite() {
        let mut inbox = seed_inbox(&load_world());
        inbox.consume_invite("oftc", "#DEBIAN");
        assert_eq!(labels(&inbox), ["mira mentioned you in #ricing"]);
    }

    #[test]
    fn a_repeated_invite_moves_to_the_top_instead_of_duplicating() {
        let mut inbox = Inbox::default();
        inbox.push(item(InboxKind::Invite, "oftc", "rio", "#debian"));
        inbox.push(item(InboxKind::Mention, "omarchy", "mira", "#ricing"));
        inbox.push(item(InboxKind::Invite, "oftc", "ota", "#Debian"));
        assert_eq!(inbox.len(), 2);
        assert_eq!(inbox.items()[0].actor, "ota");
    }

    #[test]
    fn joining_an_invited_channel_adds_it_once_with_its_demo_topic() {
        let mut world = load_world();
        let joined = join_channel(&mut world, "oftc", "#debian").expect("oftc exists");
        assert_eq!(joined, ConversationRef::new("oftc", "#debian"));
        let again = join_channel(&mut world, "oftc", "#DEBIAN").expect("oftc exists");
        assert_eq!(again, joined);
        let network = world.network("oftc").expect("oftc exists");
        let added: Vec<_> = network
            .conversations
            .iter()
            .filter(|c| c.name.eq_ignore_ascii_case("#debian"))
            .collect();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].topic.as_deref(), Some("Debian GNU/Linux."));
        assert!(join_channel(&mut world, "nowhere", "#debian").is_none());
    }

    #[test]
    fn find_channel_returns_the_existing_row_only() {
        let world = load_world();
        assert_eq!(
            find_channel(&world, "oftc", "#LAB"),
            Some(ConversationRef::new("oftc", "#lab"))
        );
        assert_eq!(
            find_channel(&world, "omarchy", "#ricing"),
            Some(ConversationRef::new("omarchy", "#ricing"))
        );
        assert_eq!(
            find_channel(&world, "oftc", "#debian"),
            None,
            "not joined yet"
        );
        assert_eq!(
            find_channel(&world, "omarchy", "anna"),
            None,
            "a DM is not a channel"
        );
        assert_eq!(find_channel(&world, "nowhere", "#lab"), None);
    }

    #[test]
    fn the_inbox_keeps_at_most_fifty_newest_rows() {
        let mut inbox = Inbox::default();
        for index in 0..(INBOX_CAP + 5) {
            inbox.push(item(
                InboxKind::Direct,
                "omarchy",
                &format!("nick{index}"),
                &format!("nick{index}"),
            ));
        }
        assert_eq!(inbox.len(), INBOX_CAP);
        assert_eq!(inbox.items()[0].actor, format!("nick{}", INBOX_CAP + 4));
    }
}
