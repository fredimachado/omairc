package irc

import "time"

// EventKind identifies one alternative of the sealed Event taxonomy. The order
// matches the IrcEvent std::variant in src/irc/ircevent.h.
type EventKind int

const (
	// EventWelcome is a WelcomeEvent.
	EventWelcome EventKind = iota
	// EventMessage is a MessageEvent.
	EventMessage
	// EventNotice is a NoticeEvent.
	EventNotice
	// EventAction is an ActionEvent.
	EventAction
	// EventJoin is a JoinEvent.
	EventJoin
	// EventPart is a PartEvent.
	EventPart
	// EventQuit is a QuitEvent.
	EventQuit
	// EventNick is a NickEvent.
	EventNick
	// EventKick is a KickEvent.
	EventKick
	// EventTopic is a TopicEvent.
	EventTopic
	// EventNames is a NamesEvent.
	EventNames
	// EventMode is a ModeEvent.
	EventMode
	// EventAway is an AwayEvent.
	EventAway
	// EventSelfAway is a SelfAwayEvent.
	EventSelfAway
	// EventMemberMetadata is a MemberMetadataEvent.
	EventMemberMetadata
	// EventAccount is an AccountEvent.
	EventAccount
	// EventTyping is a TypingEvent.
	EventTyping
	// EventHistory is a HistoryEvent.
	EventHistory
	// EventWhoisTranscript is a WhoisTranscriptEvent.
	EventWhoisTranscript
	// EventChannelError is a ChannelErrorEvent.
	EventChannelError
)

// Event is one translated IRC fact. The unexported method seals the interface
// so only this package can add alternatives.
type Event interface {
	eventKind() EventKind
}

// KindOf reports the EventKind of e, letting other packages switch on the
// taxonomy without knowing every concrete type. A nil event reports
// EventWelcome.
func KindOf(e Event) EventKind {
	if e == nil {
		return EventWelcome
	}
	return e.eventKind()
}

// ConversationKey identifies one conversation: a network plus a normalized
// target. It mirrors IrcConversationKey.
type ConversationKey struct {
	NetworkID        string
	NormalizedTarget string
}

// IsZero reports whether either component is empty.
func (k ConversationKey) IsZero() bool {
	return k.NetworkID == "" || k.NormalizedTarget == ""
}

// MsgID is an IRCv3 msgid. It mirrors IrcMsgId.
type MsgID struct {
	Value string
}

// IsEmpty reports whether no message id is present.
func (m MsgID) IsEmpty() bool {
	return m.Value == ""
}

// MessageKindTag distinguishes a chat line from an emote. It mirrors
// IrcMessageKindTag.
type MessageKindTag int

const (
	// MessageKindChat is a normal PRIVMSG line.
	MessageKindChat MessageKindTag = iota
	// MessageKindEmote is a CTCP ACTION line.
	MessageKindEmote
)

// HistoryKind identifies which reply produced a replay batch. It mirrors
// IrcHistoryKind.
type HistoryKind int

const (
	// HistoryChat is a chathistory reply.
	HistoryChat HistoryKind = iota
	// HistoryBouncerPlayback is a bouncer playback batch.
	HistoryBouncerPlayback
)

// WelcomeEvent reports 001: the network is registered and the current nick is
// known.
type WelcomeEvent struct {
	NetworkID   string
	CurrentNick string
}

func (WelcomeEvent) eventKind() EventKind { return EventWelcome }

// MessageEvent is a delivered PRIVMSG.
type MessageEvent struct {
	Conversation ConversationKey
	Author       string
	Body         string
	Timestamp    time.Time
	Target       string
	MsgID        MsgID
}

func (MessageEvent) eventKind() EventKind { return EventMessage }

// NoticeEvent is a delivered NOTICE.
type NoticeEvent struct {
	Conversation ConversationKey
	Author       string
	Body         string
	Timestamp    time.Time
	Target       string
	MsgID        MsgID
}

func (NoticeEvent) eventKind() EventKind { return EventNotice }

// ActionEvent is a CTCP ACTION message.
type ActionEvent struct {
	Conversation ConversationKey
	Author       string
	Body         string
	Timestamp    time.Time
	Target       string
	MsgID        MsgID
}

func (ActionEvent) eventKind() EventKind { return EventAction }

// JoinEvent reports a JOIN. Account is set only when the JOIN carried the
// extended-join account parameter; nil means a classic one-parameter JOIN,
// which must not clear a known account.
type JoinEvent struct {
	NetworkID string
	Channel   string
	Nick      string
	Account   *string
}

func (JoinEvent) eventKind() EventKind { return EventJoin }

// PartEvent reports a PART.
type PartEvent struct {
	NetworkID string
	Channel   string
	Nick      string
	Reason    string
}

func (PartEvent) eventKind() EventKind { return EventPart }

// QuitEvent reports a QUIT.
type QuitEvent struct {
	NetworkID string
	Nick      string
	Reason    string
}

func (QuitEvent) eventKind() EventKind { return EventQuit }

// NickEvent reports a nick change.
type NickEvent struct {
	NetworkID string
	OldNick   string
	NewNick   string
}

func (NickEvent) eventKind() EventKind { return EventNick }

// KickEvent reports a KICK.
type KickEvent struct {
	NetworkID string
	Channel   string
	Target    string
	Author    string
	Reason    string
}

func (KickEvent) eventKind() EventKind { return EventKick }

// TopicEvent reports a topic change.
type TopicEvent struct {
	NetworkID string
	Channel   string
	Topic     string
	Author    string
}

func (TopicEvent) eventKind() EventKind { return EventTopic }

// Name is one NAMES entry: a nick and its rank glyphs.
type Name struct {
	Nick  string
	Ranks PrefixSet
}

// NamesEvent reports one NAMES batch. Complete is false for a partial 353.
type NamesEvent struct {
	NetworkID string
	Channel   string
	Names     []Name
	Complete  bool
}

func (NamesEvent) eventKind() EventKind { return EventNames }

// ModeEvent reports a MODE line.
type ModeEvent struct {
	NetworkID string
	Target    string
	Author    string
	Mode      string
	Arguments []string
}

func (ModeEvent) eventKind() EventKind { return EventMode }

// AwayEvent reports an away-notify change for one nick. A nil Away means the
// nick is back.
type AwayEvent struct {
	NetworkID string
	Nick      string
	Away      *Away
}

func (AwayEvent) eventKind() EventKind { return EventAway }

// SelfAwayEvent reports the local user's own away state.
type SelfAwayEvent struct {
	NetworkID string
	Away      bool
}

func (SelfAwayEvent) eventKind() EventKind { return EventSelfAway }

// MemberMetadataEvent reports one metadata value for a nick.
type MemberMetadataEvent struct {
	NetworkID string
	Nick      string
	Key       string
	Value     string
}

func (MemberMetadataEvent) eventKind() EventKind { return EventMemberMetadata }

// AccountEvent reports an account-notify change for one nick.
type AccountEvent struct {
	NetworkID string
	Nick      string
	Account   string
}

func (AccountEvent) eventKind() EventKind { return EventAccount }

// TypingEvent reports a typing notification for a conversation.
type TypingEvent struct {
	Conversation ConversationKey
	Nick         string
	Phase        TypingPhase
	ReceivedAt   time.Time
}

func (TypingEvent) eventKind() EventKind { return EventTyping }

// ReplayLine is one line inside a replay batch. ServerTime is the parsed time
// tag only; Timestamp falls back to the local wall clock when the tag is
// missing or unparseable, and that fallback must not advance the bouncer PLAY
// clock.
type ReplayLine struct {
	Author     string
	Body       string
	Timestamp  time.Time
	Kind       MessageKindTag
	MsgID      MsgID
	ServerTime *time.Time
}

// HistoryEvent is one completed replay batch.
type HistoryEvent struct {
	Conversation ConversationKey
	Target       string
	Lines        []ReplayLine
	Kind         HistoryKind
}

func (HistoryEvent) eventKind() EventKind { return EventHistory }

// WhoisTranscriptEvent is a formatted WHOIS transcript destined for a
// conversation.
type WhoisTranscriptEvent struct {
	Destination   ConversationKey
	FormattedBody string
}

func (WhoisTranscriptEvent) eventKind() EventKind { return EventWhoisTranscript }

// ChannelErrorEvent is a channel-scoped error line.
type ChannelErrorEvent struct {
	NetworkID string
	Channel   string
	Body      string
}

func (ChannelErrorEvent) eventKind() EventKind { return EventChannelError }

// KeptReplay is a replay line the reducer actually kept: inserted, or already
// present so author/body/kind/time or msgid dedup skipped it. ServerTime is
// the raw time tag, never the translator's wall-clock fallback.
type KeptReplay struct {
	NetworkID  string
	Target     string
	ServerTime time.Time
}

// RememberedQuery is a bouncer query whose splice kept a line from the user,
// including a nick they have since changed.
type RememberedQuery struct {
	NetworkID string
	Target    string
}

// ConversationCause names why a conversation is being touched. It mirrors
// IrcConversationCause; the insert predicate lands with the reducer.
type ConversationCause int

const (
	// CauseUserOpen is a conversation the user opened.
	CauseUserOpen ConversationCause = iota
	// CauseChannelState is a channel implied by channel state.
	CauseChannelState
	// CauseInboundOther is an inbound line from someone else.
	CauseInboundOther
	// CauseInboundSelf is an inbound line the user authored on another client.
	CauseInboundSelf
	// CauseQuietSend is a local send in an existing conversation.
	CauseQuietSend
	// CauseRestore is a restore after ISUPPORT.
	CauseRestore
)

// MentionArrival is a mention the reducer hands to the controller.
type MentionArrival struct {
	Author    string
	Body      string
	NetworkID string
	Target    string
	MsgID     MsgID
}

// InboxArrival is an inbox item the reducer hands to the controller.
type InboxArrival struct {
	Kind      InboxKind
	Actor     string
	Body      string
	NetworkID string
	Target    string
	MsgID     MsgID
}
