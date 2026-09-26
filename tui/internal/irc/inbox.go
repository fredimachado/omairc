package irc

import "time"

// InboxKind classifies one inbox entry. The order mirrors IrcInboxKind.
type InboxKind int

const (
	// InboxMention is a channel line naming the user.
	InboxMention InboxKind = iota
	// InboxHighlight is a channel line the highlight rules matched.
	InboxHighlight
	// InboxDirect is a direct message.
	InboxDirect
	// InboxInvite is a channel invite.
	InboxInvite
	// InboxMonitorOnline is a monitor target coming online.
	InboxMonitorOnline
	// InboxKick is a kick aimed at the user.
	InboxKick
)

// InboxItem is one inbox entry. It mirrors IrcInboxItem. The store and item
// cap land in a later phase.
type InboxItem struct {
	Kind      InboxKind
	Timestamp time.Time
	NetworkID string
	Actor     string
	Target    string
	Preview   string
	MsgID     MsgID
}
