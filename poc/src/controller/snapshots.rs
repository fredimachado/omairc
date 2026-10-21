//! Port of `tui/internal/controller/snapshots.go` (Go). The row roles follow
//! `src/irc/conversationlistmodel.h` (C++). The Go controller rebuilt these
//! snapshots on each publish; the Rust controller reads the reducer on demand, and
//! this module keeps the shapes the sidebar and the header draw from in one place.

use crate::irc::event::ConversationKey;
use crate::session::events::SessionState;

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
}

/// The direct-message header (`PeerHeader`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerHeader {
    pub nick: String,
    /// `online`, `away` or `offline`.
    pub presence: &'static str,
    pub realname: String,
    /// The account, operator and bot labels.
    pub labels: Vec<String>,
}

/// The connection word the footer and the subtitles show (`stateText`).
pub fn state_text(state: SessionState) -> &'static str {
    match state {
        SessionState::Idle | SessionState::Failed => "Offline",
        SessionState::Connecting
        | SessionState::CapLs
        | SessionState::CapReq
        | SessionState::Sasl
        | SessionState::Registering => "Connecting",
        SessionState::Registered => "Connected",
        SessionState::Closing => "Disconnecting",
        SessionState::Reconnecting => "Reconnecting",
    }
}
