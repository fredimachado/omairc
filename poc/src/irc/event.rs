//! Port of `tui/internal/irc/event.go` (Go) and `src/irc/ircevent.h` (C++).
//!
//! The translated facts a reducer consumes. `Event` is the closed taxonomy:
//! each variant wraps one payload struct, and `Event::kind` names the
//! alternative the way Go's `EventKind` does. Payloads hold decoded text and
//! parsed times; no wire bytes survive translation.
//!
//! The small value types the session layer used to own (`HistoryKind`,
//! `PendingInvite`, `Severity`) live here now, so `session/` imports them from
//! the portable core instead of the other way round.

use chrono::{DateTime, Utc};

use super::inbox::InboxKind;
use super::presence::Away;
use super::serverfeatures::IrcPrefixSet;
use super::typing::TypingPhase;

/// The alternative an `Event` carries (`EventKind`). The order matches the Go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Welcome,
    Message,
    Notice,
    Action,
    Join,
    Part,
    Quit,
    Nick,
    Kick,
    Topic,
    Names,
    Mode,
    Away,
    SelfAway,
    MemberMetadata,
    Account,
    Typing,
    History,
    WhoisTranscript,
    ChannelError,
    NickFacts,
}

/// One translated IRC fact (`Event`). The reducer reads these in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Welcome(WelcomeEvent),
    Message(MessageEvent),
    Notice(NoticeEvent),
    Action(ActionEvent),
    Join(JoinEvent),
    Part(PartEvent),
    Quit(QuitEvent),
    Nick(NickEvent),
    Kick(KickEvent),
    Topic(TopicEvent),
    Names(NamesEvent),
    Mode(ModeEvent),
    Away(AwayEvent),
    SelfAway(SelfAwayEvent),
    MemberMetadata(MemberMetadataEvent),
    Account(AccountEvent),
    Typing(TypingEvent),
    History(HistoryEvent),
    WhoisTranscript(WhoisTranscriptEvent),
    ChannelError(ChannelErrorEvent),
    NickFacts(NickFactsEvent),
}

impl Event {
    /// The alternative this event carries (`KindOf`).
    pub fn kind(&self) -> EventKind {
        match self {
            Self::Welcome(_) => EventKind::Welcome,
            Self::Message(_) => EventKind::Message,
            Self::Notice(_) => EventKind::Notice,
            Self::Action(_) => EventKind::Action,
            Self::Join(_) => EventKind::Join,
            Self::Part(_) => EventKind::Part,
            Self::Quit(_) => EventKind::Quit,
            Self::Nick(_) => EventKind::Nick,
            Self::Kick(_) => EventKind::Kick,
            Self::Topic(_) => EventKind::Topic,
            Self::Names(_) => EventKind::Names,
            Self::Mode(_) => EventKind::Mode,
            Self::Away(_) => EventKind::Away,
            Self::SelfAway(_) => EventKind::SelfAway,
            Self::MemberMetadata(_) => EventKind::MemberMetadata,
            Self::Account(_) => EventKind::Account,
            Self::Typing(_) => EventKind::Typing,
            Self::History(_) => EventKind::History,
            Self::WhoisTranscript(_) => EventKind::WhoisTranscript,
            Self::ChannelError(_) => EventKind::ChannelError,
            Self::NickFacts(_) => EventKind::NickFacts,
        }
    }
}

macro_rules! event_from_payload {
    ($($variant:ident($payload:ident)),* $(,)?) => {
        $(
            impl From<$payload> for Event {
                fn from(payload: $payload) -> Self {
                    Self::$variant(payload)
                }
            }
        )*
    };
}

event_from_payload!(
    Welcome(WelcomeEvent),
    Message(MessageEvent),
    Notice(NoticeEvent),
    Action(ActionEvent),
    Join(JoinEvent),
    Part(PartEvent),
    Quit(QuitEvent),
    Nick(NickEvent),
    Kick(KickEvent),
    Topic(TopicEvent),
    Names(NamesEvent),
    Mode(ModeEvent),
    Away(AwayEvent),
    SelfAway(SelfAwayEvent),
    MemberMetadata(MemberMetadataEvent),
    Account(AccountEvent),
    Typing(TypingEvent),
    History(HistoryEvent),
    WhoisTranscript(WhoisTranscriptEvent),
    ChannelError(ChannelErrorEvent),
    NickFacts(NickFactsEvent),
);

/// A conversation: a network plus a normalized target (`ConversationKey`). The
/// derived order is network first, then target, as `conversationKeyLess` sorts.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConversationKey {
    pub network_id: String,
    pub normalized_target: String,
}

impl ConversationKey {
    pub fn new(network_id: impl Into<String>, normalized_target: impl Into<String>) -> Self {
        Self {
            network_id: network_id.into(),
            normalized_target: normalized_target.into(),
        }
    }

    /// True when either component is empty (`IsZero`).
    pub fn is_zero(&self) -> bool {
        self.network_id.is_empty() || self.normalized_target.is_empty()
    }
}

/// An IRCv3 `msgid` (`MsgID`). Empty when the server sent none.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MsgId {
    pub value: String,
}

impl MsgId {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    /// True when no message id is present (`IsEmpty`).
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }
}

/// Whether a replayed line is a chat line, an emote or a join/part/quit/nick
/// (`MessageKindTag`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageKindTag {
    /// A normal PRIVMSG line.
    #[default]
    Chat,
    /// A CTCP ACTION line.
    Emote,
    /// A join, part, quit, or nick line.
    Event,
}

/// Which reply produced a replay batch (`IrcHistoryKind`). A batch whose kind is
/// unknown is treated as `Chat` by the translator, the Go zero value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HistoryKind {
    /// A `chathistory` / `draft/chathistory` reply.
    #[default]
    Chat,
    /// A `znc.in/playback` batch from a bouncer.
    BouncerPlayback,
    /// A `draft/chathistory-targets` answer: names, not lines.
    Targets,
}

/// Severity of a lifecycle note (`irc.LogSeverity`). `statusentry.rs` will own
/// the full log severity; the session raises only these two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Info,
    Alert,
}

/// The last unhandled INVITE (`IrcPendingInvite`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingInvite {
    pub nick: String,
    pub channel: String,
}

/// Why a conversation is being touched (`IrcConversationCause`). The insert
/// predicate is `IrcConversationCause::inserts` in `conversation.rs`. This enum
/// is the only way `ensure_conversation` learns its cause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrcConversationCause {
    /// A local send in an existing conversation. Never inserts.
    QuietSend,
    /// An inbound line the user authored on another client. Never inserts.
    InboundSelf,
    /// A conversation the user opened. Inserts DMs only.
    UserOpen,
    /// A channel implied by channel state. Inserts channels only.
    ChannelState,
    /// An inbound line from someone else. Inserts channels and non-service DMs.
    InboundOther,
    /// A restore after ISUPPORT. Inserts non-service DMs only.
    Restore,
}

/// `001`: the network is registered and the current nick is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WelcomeEvent {
    pub network_id: String,
    pub current_nick: String,
}

/// A delivered PRIVMSG (`MessageEvent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageEvent {
    pub conversation: ConversationKey,
    pub author: String,
    pub body: String,
    pub timestamp: DateTime<Utc>,
    /// The IRCv3 `time` tag, when present and parseable. `timestamp` falls back
    /// to the wall clock without it.
    pub server_time: Option<DateTime<Utc>>,
    /// The display target: the channel, or the peer for a direct message.
    pub target: String,
    pub msgid: MsgId,
    pub only_visible_to_you: bool,
}

/// A delivered NOTICE (`NoticeEvent`). Only channel-context notices produce one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoticeEvent {
    pub conversation: ConversationKey,
    pub author: String,
    pub body: String,
    pub timestamp: DateTime<Utc>,
    pub server_time: Option<DateTime<Utc>>,
    pub target: String,
    pub msgid: MsgId,
    pub only_visible_to_you: bool,
}

/// A CTCP ACTION (`ActionEvent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionEvent {
    pub conversation: ConversationKey,
    pub author: String,
    pub body: String,
    pub timestamp: DateTime<Utc>,
    pub server_time: Option<DateTime<Utc>>,
    pub target: String,
    pub msgid: MsgId,
    pub only_visible_to_you: bool,
}

/// A JOIN. `account` is `Some` only when the JOIN carried the extended-join
/// account parameter. `None` is a classic JOIN, which must not clear a known
/// account. `realname` is `Some` only when a third parameter exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinEvent {
    pub network_id: String,
    pub channel: String,
    pub nick: String,
    pub account: Option<String>,
    pub realname: Option<String>,
}

/// A PART (`PartEvent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartEvent {
    pub network_id: String,
    pub channel: String,
    pub nick: String,
    pub reason: String,
}

/// A QUIT (`QuitEvent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuitEvent {
    pub network_id: String,
    pub nick: String,
    pub reason: String,
}

/// A nick change (`NickEvent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NickEvent {
    pub network_id: String,
    pub old_nick: String,
    pub new_nick: String,
}

/// A KICK (`KickEvent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KickEvent {
    pub network_id: String,
    pub channel: String,
    pub target: String,
    pub author: String,
    pub reason: String,
}

/// A topic change or `332` (`TopicEvent`). `author` is empty for `332`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopicEvent {
    pub network_id: String,
    pub channel: String,
    pub topic: String,
    pub author: String,
}

/// One NAMES entry: a nick and its rank letters (`Name`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    pub nick: String,
    pub ranks: IrcPrefixSet,
}

/// One NAMES batch (`NamesEvent`). `complete` is false for a partial `353`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamesEvent {
    pub network_id: String,
    pub channel: String,
    pub names: Vec<Name>,
    pub complete: bool,
}

/// A MODE line (`ModeEvent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeEvent {
    pub network_id: String,
    pub target: String,
    pub author: String,
    pub mode: String,
    pub arguments: Vec<String>,
}

/// An away-notify change for one nick (`AwayEvent`). `away` is `None` when the
/// nick is back. `realname` is set by `352` and `311`; `None` leaves a stored
/// real name alone. `realname_only` marks a WHOIS `311`, which must not change
/// away.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AwayEvent {
    pub network_id: String,
    pub nick: String,
    pub away: Option<Away>,
    pub realname: Option<String>,
    pub realname_only: bool,
}

/// The local user's own away state (`SelfAwayEvent`), from `305` / `306`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfAwayEvent {
    pub network_id: String,
    pub away: bool,
}

/// One metadata value for a nick (`MemberMetadataEvent`). An empty `value` clears
/// the key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberMetadataEvent {
    pub network_id: String,
    pub nick: String,
    pub key: String,
    pub value: String,
}

/// An account-notify change for one nick (`AccountEvent`). `*` or "" is a logout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountEvent {
    pub network_id: String,
    pub nick: String,
    pub account: String,
}

/// A typing notification for a conversation (`TypingEvent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypingEvent {
    pub conversation: ConversationKey,
    pub nick: String,
    pub phase: TypingPhase,
    pub received_at: DateTime<Utc>,
}

/// One line inside a replay batch (`ReplayLine`). `server_time` is the parsed
/// time tag only. `timestamp` falls back to the wall clock, and that fallback
/// must not advance the bouncer PLAY clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayLine {
    pub author: String,
    pub body: String,
    pub timestamp: DateTime<Utc>,
    pub kind: MessageKindTag,
    pub msgid: MsgId,
    pub server_time: Option<DateTime<Utc>>,
    pub only_visible_to_you: bool,
}

/// One completed replay batch (`HistoryEvent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEvent {
    pub conversation: ConversationKey,
    pub target: String,
    pub lines: Vec<ReplayLine>,
    pub kind: HistoryKind,
    /// True for a solicited `CHATHISTORY BEFORE` page.
    pub older_page: bool,
}

/// A formatted WHOIS transcript for a conversation (`WhoisTranscriptEvent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhoisTranscriptEvent {
    pub destination: ConversationKey,
    pub formatted_body: String,
}

/// A channel-scoped error line (`ChannelErrorEvent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelErrorEvent {
    pub network_id: String,
    pub channel: String,
    pub body: String,
}

/// A WHO or WHOIS identity fact (`NickFactsEvent`). A `None` field is left
/// unchanged. `352` sets both when the trailing gecos is present and always sets
/// the operator flag. `311` sets the real name only. `313` sets operator true
/// only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NickFactsEvent {
    pub network_id: String,
    pub nick: String,
    pub realname: Option<String>,
    pub server_operator: Option<bool>,
}

/// A replay line the reducer kept: inserted, or already present so dedup skipped
/// it (`KeptReplay`). `server_time` is the raw time tag, never the wall-clock
/// fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeptReplay {
    pub network_id: String,
    pub target: String,
    pub server_time: DateTime<Utc>,
}

/// A bouncer query whose splice kept a line from the user, including a nick they
/// have since changed (`RememberedQuery`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RememberedQuery {
    pub network_id: String,
    pub target: String,
}

/// A mention the reducer hands to the controller (`MentionArrival`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionArrival {
    pub author: String,
    pub body: String,
    pub network_id: String,
    pub target: String,
    pub msgid: MsgId,
}

/// An inbox item the reducer hands to the controller (`InboxArrival`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InboxArrival {
    pub kind: InboxKind,
    pub actor: String,
    pub body: String,
    pub network_id: String,
    pub target: String,
    pub msgid: MsgId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 4, 0, 0, 0).unwrap()
    }

    #[test]
    fn every_payload_reports_its_own_kind() {
        let welcome: Event = WelcomeEvent {
            network_id: "n".into(),
            current_nick: "omairc".into(),
        }
        .into();
        assert_eq!(welcome.kind(), EventKind::Welcome);

        let names: Event = NamesEvent {
            network_id: "n".into(),
            channel: "#room".into(),
            names: Vec::new(),
            complete: true,
        }
        .into();
        assert_eq!(names.kind(), EventKind::Names);

        let typing: Event = TypingEvent {
            conversation: ConversationKey::new("n", "alice"),
            nick: "alice".into(),
            phase: TypingPhase::Paused,
            received_at: at(),
        }
        .into();
        assert_eq!(typing.kind(), EventKind::Typing);

        let facts: Event = NickFactsEvent {
            network_id: "n".into(),
            nick: "alice".into(),
            realname: None,
            server_operator: Some(true),
        }
        .into();
        assert_eq!(facts.kind(), EventKind::NickFacts);
    }

    #[test]
    fn kind_order_matches_the_go_taxonomy() {
        // The Go iota order: `EventWelcome` is 0 and `EventNickFacts` is the last.
        assert_eq!(EventKind::Welcome as usize, 0);
        assert_eq!(EventKind::Message as usize, 1);
        assert_eq!(EventKind::SelfAway as usize, 13);
        assert_eq!(EventKind::MemberMetadata as usize, 14);
        assert_eq!(EventKind::NickFacts as usize, 20);
    }

    #[test]
    fn conversation_keys_sort_network_first_then_target() {
        let mut keys = vec![
            ConversationKey::new("b", "#a"),
            ConversationKey::new("a", "zed"),
            ConversationKey::new("a", "#room"),
        ];
        keys.sort();
        assert_eq!(
            keys,
            vec![
                ConversationKey::new("a", "#room"),
                ConversationKey::new("a", "zed"),
                ConversationKey::new("b", "#a"),
            ]
        );
    }

    #[test]
    fn zero_keys_and_empty_msgids_are_detected() {
        assert!(ConversationKey::default().is_zero());
        assert!(ConversationKey::new("n", "").is_zero());
        assert!(!ConversationKey::new("n", "#room").is_zero());
        assert!(MsgId::default().is_empty());
        assert!(!MsgId::new("abc").is_empty());
    }

    #[test]
    fn history_and_message_kind_defaults_are_the_go_zero_values() {
        assert_eq!(HistoryKind::default(), HistoryKind::Chat);
        assert_eq!(MessageKindTag::default(), MessageKindTag::Chat);
    }
}
