//! The read side the shell draws from. Port of `Conversations`, `Messages`,
//! `Members`, `Topic`, `PeopleCount`, `TypingNicks`, `SelectedPeerHeader`,
//! `HasAwayPresence`, `HasMemberStatus`, `HasTyping`, `UnreadCountFor`,
//! `MentionFor`, `CurrentNick`, `SelfAway`, `InboxItems` and `ConsoleText` in
//! `tui/internal/controller/` (Go), and the `ConversationListModel`,
//! `MessageListModel` and `MemberListModel` roles in `src/irc/` (C++).
//!
//! Reads take `&self` and answer from the reducer as it stands. The UI calls
//! `tick` or `pump` before it reads, so the answers include every queued event.

use crate::irc::capability::IrcCapability;
use crate::irc::conversation::conversation_id;
use crate::irc::event::ConversationKey;
use crate::irc::inbox::InboxItem;
use crate::irc::reducer::{MemberView, PeerPresence};
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::statusentry::StatusEntry;
use crate::irc::world::Message;

use super::controller::Controller;
use super::snapshots::{ConversationRow, PeerHeader};

impl Controller {
    /// The selected conversation's key, or `None` while Status is the selection.
    pub fn selected(&self) -> Option<&ConversationKey> {
        self.selected.as_ref()
    }

    /// The selected conversation's network id.
    pub fn selected_network_id(&self) -> Option<&str> {
        self.selected.as_ref().map(|key| key.network_id.as_str())
    }

    /// The selected conversation's stable id, as the sidebar keys its row.
    pub fn selected_conversation_id(&self) -> Option<String> {
        self.selected.as_ref().map(conversation_id)
    }

    /// The selected conversation's display target: the channel name, or the peer's
    /// nick. Empty when nothing is selected.
    pub fn selected_target(&self) -> &str {
        match self.selected.as_ref().and_then(|key| self.reducer.find(key)) {
            Some(conversation) => conversation.target.as_str(),
            None => self.selected_target.as_str(),
        }
    }

    /// The network the keyboard and the composer point at (`FocusedNetworkID`): the
    /// Status network while Status is open, else the selection's network, else the
    /// Status network.
    pub fn focused_network_id(&self) -> &str {
        if self.console_open {
            return &self.console_network;
        }
        match self.selected.as_ref() {
            Some(key) => &key.network_id,
            None => &self.console_network,
        }
    }

    /// Whether the transcript shows Status (`ConsoleOpen`): Status was opened, or
    /// nothing is selected.
    pub fn shows_status(&self) -> bool {
        self.console_open || self.selected_target().is_empty()
    }

    /// Whether the selected conversation is a channel (`IsChannel`).
    pub fn is_channel(&self) -> bool {
        let Some(key) = self.selected.as_ref() else {
            return false;
        };
        self.reducer
            .server_features(&key.network_id)
            .is_channel(self.selected_target())
    }

    /// The selected conversation's topic (`Topic`). A direct message reads as its
    /// header line.
    pub fn topic(&self) -> String {
        let Some(key) = self.selected.as_ref() else {
            return String::new();
        };
        match self.reducer.find(key) {
            None => {
                if self.is_channel() {
                    String::new()
                } else {
                    "Direct message".to_owned()
                }
            }
            Some(conversation) => match conversation.channel.as_ref() {
                Some(channel) => channel.topic.clone(),
                None => format!("Direct message with {}", conversation.target),
            },
        }
    }

    /// The people count the header and the member toggle show (`PeopleCount`).
    pub fn people_count(&self) -> usize {
        match self.selected.as_ref() {
            Some(key) => self.reducer.ordered_members(key).len(),
            None => 0,
        }
    }

    /// The selected channel's members, in panel order: the server's PREFIX rank
    /// highest first, then nick (`MemberListModel`). Empty for a direct message.
    pub fn members(&self) -> Vec<MemberView> {
        let Some(key) = self.selected.as_ref() else {
            return Vec::new();
        };
        self.reducer
            .ordered_members(key)
            .into_iter()
            .filter_map(|member| self.reducer.member_view(key, &member.nick))
            .collect()
    }

    /// The nicks typing in the selected conversation right now (`TypingNicks`).
    pub fn typing_nicks(&self) -> Vec<String> {
        match self.selected.as_ref() {
            Some(key) => self.reducer.typing_nicks(key, self.clock.now()),
            None => Vec::new(),
        }
    }

    /// The selected conversation's transcript, oldest first. Empty when nothing is
    /// selected or the conversation has no rows yet.
    pub fn messages(&self) -> &[Message] {
        match self.selected.as_ref().and_then(|key| self.reducer.find(key)) {
            Some(conversation) => &conversation.messages,
            None => &[],
        }
    }

    /// The direct-message header for the selection (`SelectedPeerHeader`). `None`
    /// while Status shows, for a channel, or when nothing is selected.
    pub fn peer_header(&self) -> Option<PeerHeader> {
        if self.console_open || self.is_channel() {
            return None;
        }
        let key = self.selected.as_ref()?;
        let nick = self.selected_target();
        if nick.is_empty() {
            return None;
        }
        let normalized = self
            .reducer
            .conversation_key(&key.network_id, nick)
            .normalized_target;
        let presence = match self.reducer.peer_presence(&key.network_id, &normalized) {
            PeerPresence::Online => "online",
            PeerPresence::Away => "away",
            PeerPresence::Unknown => "offline",
        };
        Some(PeerHeader {
            nick: nick.to_owned(),
            presence,
            realname: self.reducer.meaningful_realname(&key.network_id, nick),
            labels: self.reducer.peer_fact_labels(&key.network_id, nick),
        })
    }

    /// The sidebar rows, in sidebar order: networks in their recorded order, then each
    /// network's conversations (`Conversations`).
    pub fn conversations(&self) -> Vec<ConversationRow> {
        let now = self.clock.now();
        self.reducer
            .sidebar_order_with_networks(&self.network_order)
            .into_iter()
            .filter_map(|key| {
                let conversation = self.reducer.find(&key)?;
                let direct = !conversation.is_channel();
                let away_notify = self.caps(&key.network_id).contains(IrcCapability::AwayNotify);
                let presence = if direct && away_notify {
                    match self
                        .reducer
                        .peer_presence(&key.network_id, &key.normalized_target)
                    {
                        PeerPresence::Online => "online",
                        PeerPresence::Away => "away",
                        PeerPresence::Unknown => "",
                    }
                } else {
                    ""
                };
                let facts = direct.then(|| {
                    self.reducer
                        .nick_presence(&key.network_id, &key.normalized_target)
                });
                Some(ConversationRow {
                    conversation_id: conversation_id(&key),
                    target: conversation.target.clone(),
                    unread: conversation.unread,
                    mention: conversation.mentions > 0,
                    direct,
                    typing: self.reducer.direct_peer_is_typing(&key, now),
                    muted: conversation.muted,
                    presence,
                    avatar: facts
                        .as_ref()
                        .map(|facts| facts.avatar().to_owned())
                        .unwrap_or_default(),
                    bot: facts.as_ref().is_some_and(|facts| facts.is_bot()),
                    key,
                })
            })
            .collect()
    }

    /// Unread messages across a network's conversations (`UnreadCountFor`).
    pub fn unread_count_for(&self, network_id: &str) -> u32 {
        self.reducer
            .conversations()
            .iter()
            .filter(|(key, _)| key.network_id == network_id)
            .map(|(_, conversation)| conversation.unread)
            .sum()
    }

    /// Whether any of a network's conversations holds an unread mention
    /// (`MentionFor`).
    pub fn mention_for(&self, network_id: &str) -> bool {
        self.reducer
            .conversations()
            .iter()
            .any(|(key, conversation)| key.network_id == network_id && conversation.mentions > 0)
    }

    /// The nick a network's session holds (`CurrentNick`).
    pub fn current_nick_for(&self, network_id: &str) -> String {
        self.current_nick(network_id)
    }

    /// Whether the user is away on the focused network (`SelfAway`).
    pub fn self_away(&self) -> bool {
        let network_id = self.focused_network_id();
        !network_id.is_empty() && self.reducer.self_away(network_id)
    }

    /// The last error for the focused network (`LastError`).
    pub fn last_error(&self) -> Option<&str> {
        self.last_error_for(self.focused_network_id())
    }

    /// Whether the selected channel's members can show away state
    /// (`HasAwayPresence`).
    pub fn has_away_presence(&self) -> bool {
        self.selected_network_id()
            .is_some_and(|network| self.caps(network).contains(IrcCapability::AwayNotify))
    }

    /// Whether the member panel can show status prefixes (`HasMemberStatus`).
    pub fn has_member_status(&self) -> bool {
        self.selected_network_id().is_some_and(|network| {
            let caps = self.caps(network);
            caps.contains(IrcCapability::MemberMetadata) && caps.contains(IrcCapability::Batch)
        })
    }

    /// Whether typing indicators can be sent and shown (`HasTyping`).
    pub fn has_typing(&self) -> bool {
        self.selected_network_id()
            .is_some_and(|network| self.caps(network).contains(IrcCapability::MessageTags))
    }

    /// The session inbox, newest first (`InboxItems`).
    pub fn inbox_items(&self) -> &[InboxItem] {
        self.inbox.items()
    }

    /// The Status lines for a network, oldest first. Secrets are already masked.
    pub fn status_lines(&self, network_id: &str) -> Vec<&StatusEntry> {
        self.console.lines(network_id)
    }

    /// The ISUPPORT features the reducer holds for a network. The shell reads the
    /// channel prefixes and the line length from here.
    pub fn features(&self, network_id: &str) -> &IrcServerFeatures {
        self.reducer.server_features(network_id)
    }

    /// The display name of a network: its profile name, else its host
    /// (`IrcNetworkProfile::resolvedName`). When another network shows the same name,
    /// the nick joins it, as `irc.example · fred`. Empty for an unknown network.
    pub fn network_name(&self, network_id: &str) -> String {
        let Some(session) = self.session(network_id) else {
            return String::new();
        };
        let base = session.name();
        let clash = self
            .manager
            .network_ids()
            .iter()
            .filter(|other| other.as_str() != network_id)
            .filter_map(|other| self.session(other))
            .any(|other| other.name() == base);
        if clash {
            format!("{base} · {}", session.nick())
        } else {
            base
        }
    }

    /// The capabilities a network last reported, empty before any report.
    pub(super) fn caps(&self, network_id: &str) -> crate::irc::capability::IrcCapabilitySet {
        self.capabilities
            .get(network_id)
            .copied()
            .unwrap_or_default()
    }
}
