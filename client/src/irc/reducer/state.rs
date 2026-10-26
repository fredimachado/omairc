//! The state the reducer keeps: one conversation transcript plus its live state,
//! the channel half, the member view the panel reads, and the order a channel's
//! members are listed in.
//!
//! Port of the state types at the top of `tui/internal/irc/reducer.go` (Go:
//! `ConversationState`, `ChannelState`, `MemberState`, `MemberView`,
//! `OrderedMember`, `TranscriptAnchor`, `PeerPresence`) and of the
//! `IrcEventReducer::Store` / `IrcConversationState` members in
//! `src/irc/irceventreducer.h` (C++).
//!
//! Transcript rows are `world::Message`, the shape the Go `IrcReducedMessage`
//! maps to. A row's zero time (Go's `time.Time{}`, which event lines carry) is
//! `zero_time()`.

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};

use crate::irc::event::{ConversationKey, MsgId};
use crate::irc::presence::Away;
use crate::irc::serverfeatures::IrcPrefixSet;
use crate::irc::typing::TypingHint;
use crate::irc::world::Message;

/// The transcript cap per conversation (`IrcEventReducer::kMaxMessages`).
pub const MAX_MESSAGES: usize = 2000;

/// How long a NAMES sync may run before it is released
/// (`IrcEventReducer::kStaleNamesSyncMs`).
pub const STALE_NAMES_SYNC_MS: i64 = 30_000;

/// Every conversation, keyed by network then normalized target. A `BTreeMap`
/// gives the `std::map` order the C++ store and the Go sort both rely on.
pub type Store = BTreeMap<ConversationKey, ConversationState>;

/// The zero instant Go's `time.Time{}` stands for: an event line has no time, so
/// it carries this value and is never compared as a real timestamp.
pub fn zero_time() -> DateTime<Utc> {
    DateTime::<Utc>::MIN_UTC
}

/// One channel member as stored: the display nick and its rank bits
/// (`MemberState`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberState {
    pub display_nick: String,
    pub ranks: IrcPrefixSet,
}

/// A logical transcript position: the sequence of the self-join line a replay
/// batch splices above (`TranscriptAnchor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptAnchor {
    pub sequence: i64,
}

/// The channel half of a conversation (`ChannelState`). A direct message has
/// none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChannelState {
    /// Members by normalized nick.
    pub members: BTreeMap<String, MemberState>,
    pub topic: String,
    pub joined: bool,
    pub names_syncing: bool,
    /// When the current NAMES sync started. `None` when no sync runs.
    pub names_sync_started: Option<DateTime<Utc>>,
    pub history_anchor: Option<TranscriptAnchor>,
}

/// One conversation transcript plus its live state (`ConversationState`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationState {
    pub key: ConversationKey,
    /// The display target: the channel name, or the peer's current nick.
    pub target: String,
    pub messages: Vec<Message>,
    /// Typing hints by normalized nick.
    pub typing: BTreeMap<String, TypingHint>,
    pub unread: u32,
    pub mentions: u32,
    /// The sequence of the "New messages" boundary row, if one is planted.
    pub unread_mark: Option<i64>,
    /// The latest server read marker, if the server sent one.
    pub read_marker: Option<DateTime<Utc>>,
    pub muted: bool,
    /// Rows trimmed off the head by the cap, so `sequence - trimmed` is a
    /// position in `messages`.
    pub trimmed: usize,
    pub message_ids: HashSet<MsgId>,
    pub splice_epoch: u64,
    pub next_sequence: i64,
    /// Drops an overfull transcript from the tail after a CHATHISTORY older page
    /// lands while the reader is parked at the top.
    pub history_page_cap_tail: bool,
    pub channel: Option<ChannelState>,
}

impl ConversationState {
    /// An empty conversation with no channel half. The caller adds one for a
    /// channel.
    pub fn new(key: ConversationKey, target: &str) -> Self {
        Self {
            key,
            target: target.to_owned(),
            messages: Vec::new(),
            typing: BTreeMap::new(),
            unread: 0,
            mentions: 0,
            unread_mark: None,
            read_marker: None,
            muted: false,
            trimmed: 0,
            message_ids: HashSet::new(),
            splice_epoch: 0,
            next_sequence: 0,
            history_page_cap_tail: false,
            channel: None,
        }
    }

    /// Whether this conversation is a channel (`IsChannel`).
    pub fn is_channel(&self) -> bool {
        self.channel.is_some()
    }

    /// The stored channel members, or 0 for a direct message (`PeopleCount`).
    #[cfg(test)]
    pub fn people_count(&self) -> usize {
        self.channel
            .as_ref()
            .map_or(0, |channel| channel.members.len())
    }
}

/// The displayed row for one channel member, joining membership with the
/// per-network presence facts (`MemberView`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemberView {
    pub nick: String,
    /// The rank label, such as `@alice`.
    pub label: String,
    pub ranks: IrcPrefixSet,
    pub away: Option<Away>,
    pub status: String,
    pub avatar: String,
    pub bot: bool,
    pub display_name: String,
    pub pronouns: String,
    pub homepage: String,
    pub color: String,
    /// Empty when unknown, logged out, or the same as the nick under the case
    /// mapping.
    pub account: String,
    /// The meaningful gecos. Empty when blank or a placeholder.
    pub realname: String,
    /// The query-header labels: account or `unauthenticated`, then
    /// `server operator`, then `bot`.
    pub labels: Vec<String>,
}

impl MemberView {
    /// Whether the member is marked away (`IsAway`).
    pub fn is_away(&self) -> bool {
        self.away.is_some()
    }
}

/// What the reducer knows about a nick's availability (`PeerPresence`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerPresence {
    /// No shared channel proves the nick is online.
    Unknown,
    /// The nick shares a channel and is not away.
    Online,
    /// The nick shares a channel and is away.
    Away,
}

/// One channel member in panel order: `priority` is the index of the member's
/// highest rank in the server's PREFIX order, so a lower value is a higher
/// privilege and a member without a rank comes last (`OrderedMember`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderedMember {
    pub nick: String,
    pub priority: usize,
}
