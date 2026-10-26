//! `tui/internal/controller/snapshots.go`. The Go controller rebuilt these
//! snapshots on each publish; the Rust controller reads the reducer on demand, and
//! this module keeps the shapes the sidebar and the header draw from in one place.

use crate::irc::event::ConversationKey;
#[cfg(test)]
use crate::irc::world::MessageKind;
use crate::session::events::SessionState;

/// The transcript kind's name (`messageKindName`), as the saved scroll anchor records it.
// The scroll anchor is the Qt-format path, which nothing outside the tests drives yet.
#[cfg(test)]
pub fn message_kind_name(kind: MessageKind) -> &'static str {
    match kind {
        MessageKind::Action => "action",
        MessageKind::Event | MessageKind::Error => "event",
        MessageKind::Whois => "whois",
        MessageKind::Notice => "notice",
        MessageKind::Message => "message",
    }
}

/// One sidebar row (`ConversationSnapshot`, the `ConversationListModel` roles).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationRow {
    pub key: ConversationKey,
    /// The stable id the UI keys rows by (`conversationId`).
    pub conversation_id: String,
    /// The display target: the channel name, or the peer's nick.
    pub target: String,
    pub unread: u32,
    pub mention: bool,
    pub direct: bool,
    /// The peer is typing in this direct message.
    pub typing: bool,
    pub muted: bool,
    /// `online` or `away` for a direct message whose presence is known, else empty.
    pub presence: &'static str,
    pub avatar: String,
    pub bot: bool,
    /// The channel's topic, or a direct message peer's real name (the Jump detail line).
    pub detail: String,
}

/// The direct-message header (`PeerHeader`). Its title is the conversation's title, so
/// it carries only what the header adds to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerHeader {
    /// `online`, `away` or `offline`.
    pub presence: &'static str,
    /// The peer's real name when it is worth showing, else empty.
    pub realname: String,
    /// The account, operator and bot labels.
    pub labels: Vec<String>,
}

/// The connection word the footer and the subtitles show (`stateText`).
pub fn state_text(state: SessionState) -> &'static str {
    match state {
        SessionState::Idle | SessionState::Failed => "Offline",
        SessionState::Connecting
        | SessionState::StsUpgrading
        | SessionState::CapLs
        | SessionState::CapReq
        | SessionState::Sasl
        | SessionState::Registering => "Connecting",
        SessionState::Registered => "Connected",
        SessionState::Closing => "Disconnecting",
        SessionState::Reconnecting => "Reconnecting",
    }
}
