//! The World-facing shapes the UI reads: `Network`, `Conversation`, `Member`
//! and `Message`. They replace the mock world in `model/mock.rs` once the
//! reducer fills them.
//!
//! `Message` is the Go `IrcReducedMessage` (`reducer.go`), with the `mentioned`
//! flag the present `MockMessage` UI already reads. `Network`, `Conversation`
//! and `Member` keep the field names and types of the mock structs they replace,
//! except for the time fields, which are real instants now.
//!
//! Two present fields are not here yet: a network's Status log (`status`, from
//! `statusentry.go`) and its `/list` directory (`directory`, from the channel
//! list controller). Each arrives with its own leaf module. Nothing in this file
//! reads the wire. The reducer builds these values; the UI reads them.

use chrono::{DateTime, Utc};

use super::event::MsgId;

/// The kind of one transcript row (`IrcMessageKind`). Mirrors the Go order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageKind {
    /// A normal chat line.
    #[default]
    Message,
    /// A NOTICE line.
    Notice,
    /// A CTCP ACTION (emote) line.
    Action,
    /// A join, part, quit, mode or state line. Errors persist under this kind too.
    Event,
    /// A local error line.
    Error,
    /// One formatted WHOIS transcript line.
    Whois,
}

/// Whether a row came from the live stream or from a replay batch (`IrcOrigin`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Origin {
    /// An inbound line as it arrived.
    #[default]
    Live,
    /// A line spliced from a CHATHISTORY or bouncer playback batch.
    Replay,
}

/// One transcript row (`Message`; `IrcReducedMessage` in Go).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub kind: MessageKind,
    /// The author's nick. Empty for a server, event or whois line.
    pub author: String,
    pub body: String,
    /// The display instant: the server time tag when present, else the local
    /// wall clock at receipt.
    pub time: DateTime<Utc>,
    /// The raw time tag, when the server sent one. Never the wall-clock fallback.
    pub server_time: Option<DateTime<Utc>>,
    pub msgid: MsgId,
    pub origin: Origin,
    /// True when this line named the local user, so the sidebar and the inbox
    /// count it as a mention.
    pub mentioned: bool,
    /// True when the row may collapse into the event run above it.
    pub collapsible: bool,
    /// A monotonically increasing sequence within the conversation. Splices and
    /// anchors use it, so it never reorders.
    pub sequence: i64,
    /// True when only the local user can see the line (channel-context delivery).
    pub only_visible_to_you: bool,
}

/// One channel member as the panel shows it (`Member`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub nick: String,
    /// The rank glyphs, highest first (`@`, `+`, ...).
    pub prefix: String,
    pub realname: String,
    /// `online`, `away`, or another presence word.
    pub presence: String,
    pub bot: bool,
    /// The nick's status line (`761 status`), drawn muted under the nick.
    pub status: Option<String>,
    /// The nick's avatar (`761 avatar`): a bundled image or an https URL.
    pub avatar: Option<String>,
}

/// One conversation: a channel or a direct message (`Conversation`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversation {
    pub name: String,
    pub channel: bool,
    pub topic: Option<String>,
    /// A direct message peer's realname, from WHOIS or WHO.
    pub peer_realname: Option<String>,
    pub unread: u32,
    pub mention: bool,
    pub typing: bool,
    pub members: Vec<Member>,
    pub messages: Vec<Message>,
}

impl Conversation {
    pub fn is_channel(&self) -> bool {
        self.channel
    }

    /// The total behind both the `N PEOPLE` pill and the member heading.
    pub fn people_count(&self) -> usize {
        self.members.len()
    }

    /// Opening a conversation reads it: its unread count and mention flag clear.
    pub fn mark_read(&mut self) {
        self.unread = 0;
        self.mention = false;
    }
}

/// One network: its identity, its channel prefixes and its conversations
/// (`Network`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    pub id: String,
    pub display_name: String,
    /// Index into the palette of Qt's `IrcNetworkProfile::iconColor`, if the
    /// network has one.
    pub icon_color: Option<u8>,
    pub nick: String,
    /// The last command error. The header subtitle shows it in place of the
    /// topic until the next successful command or selection clears it.
    pub last_error: Option<String>,
    /// The channel prefix characters (`CHANTYPES`). Empty until ISUPPORT says.
    pub chantypes: String,
    /// The network's icon (`draft/ICON`): a bundled image or an https URL.
    pub icon: Option<String>,
    pub conversations: Vec<Conversation>,
}

impl Network {
    /// A network with no conversations yet. The reducer adds one when a
    /// connection is first seen.
    pub fn empty(
        id: &str,
        display_name: &str,
        nick: &str,
        icon_color: Option<u8>,
        chantypes: &str,
    ) -> Self {
        Self {
            id: id.to_string(),
            display_name: display_name.to_string(),
            icon_color,
            nick: nick.to_string(),
            last_error: None,
            chantypes: chantypes.to_string(),
            icon: None,
            conversations: Vec::new(),
        }
    }

    /// The first member record for `nick` in any of this network's channels. A
    /// nick's status, avatar and bot flag are per network, so any record answers
    /// for all of them.
    pub fn member_for(&self, nick: &str) -> Option<&Member> {
        self.conversations
            .iter()
            .flat_map(|conversation| &conversation.members)
            .find(|member| member.nick == nick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn member(nick: &str) -> Member {
        Member {
            nick: nick.to_string(),
            prefix: String::new(),
            realname: String::new(),
            presence: "online".to_string(),
            bot: false,
            status: None,
            avatar: None,
        }
    }

    #[test]
    fn people_count_and_member_lookup_read_the_same_members() {
        let mut network = Network::empty("oftc", "OFTC", "omairc", None, "#&");
        network.conversations.push(Conversation {
            name: "#omarchy".to_string(),
            channel: true,
            topic: None,
            peer_realname: None,
            unread: 3,
            mention: true,
            typing: false,
            members: vec![member("alice"), member("bob")],
            messages: Vec::new(),
        });
        let channel = &network.conversations[0];
        assert!(channel.is_channel());
        assert_eq!(channel.people_count(), 2);
        assert_eq!(
            network.member_for("bob").map(|m| m.nick.as_str()),
            Some("bob")
        );
        assert!(network.member_for("carol").is_none());
    }

    #[test]
    fn opening_a_conversation_clears_unread_and_mention() {
        let mut conversation = Conversation {
            name: "alice".to_string(),
            channel: false,
            topic: None,
            peer_realname: None,
            unread: 2,
            mention: true,
            typing: false,
            members: Vec::new(),
            messages: Vec::new(),
        };
        conversation.mark_read();
        assert_eq!(conversation.unread, 0);
        assert!(!conversation.mention);
    }

    #[test]
    fn message_defaults_are_live_chat_lines() {
        assert_eq!(MessageKind::default(), MessageKind::Message);
        assert_eq!(Origin::default(), Origin::Live);
        let time = Utc.with_ymd_and_hms(2026, 9, 4, 0, 0, 0).unwrap();
        let line = Message {
            kind: MessageKind::Whois,
            author: String::new(),
            body: "alice is a user".to_string(),
            time,
            server_time: None,
            msgid: MsgId::default(),
            origin: Origin::Replay,
            mentioned: false,
            collapsible: false,
            sequence: 7,
            only_visible_to_you: false,
        };
        assert_eq!(line.origin, Origin::Replay);
        assert!(line.msgid.is_empty());
    }
}
