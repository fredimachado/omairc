package controller

import (
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// ConversationSnapshot is one sidebar row. The fields mirror the roles
// ConversationListModel exposes (src/irc/conversationlistmodel.h): conversation,
// unread, mention, direct, networkId, conversationId, conversationName, typing,
// muted, presence, avatar, bot.
type ConversationSnapshot struct {
	Conversation     string
	Unread           int
	Mention          bool
	Direct           bool
	NetworkID        string
	ConversationID   string
	ConversationName string
	Typing           bool
	Muted            bool
	Presence         string
	Avatar           string
	Bot              bool
}

// MessageSnapshot is one transcript row of the selected conversation. The
// fields mirror the roles MessageListModel exposes (src/irc/messagelistmodel.h)
// minus the visual-only rows, which are a view concern.
type MessageSnapshot struct {
	Author    string
	Time      time.Time
	Body      string
	Kind      string
	Origin    string
	MsgID     string
	Sequence  int64
	Mentioned bool
}

// MemberSnapshot is one member-panel row of the selected channel. The fields
// mirror the roles MemberListModel exposes (src/irc/memberlistmodel.h),
// including BotRole.
type MemberSnapshot struct {
	Nick    string
	Label   string
	Status  string
	Away    bool
	Account string
	Bot     bool
}

// InboxSnapshot is one session-inbox row for the shell. It mirrors the
// IrcInboxModel roles (kind, networkId, actor, target, preview, msgid, label).
type InboxSnapshot struct {
	Kind      string
	NetworkID string
	Actor     string
	Target    string
	Preview   string
	MsgID     string
	Label     string
}

// Conversations returns the sidebar rows in sidebar order: joined channels,
// then parted channels, then direct messages, grouped by the controller's
// network order. The slice is the cached snapshot Publish last rebuilt; callers
// must not mutate it.
func (c *Controller) Conversations() []ConversationSnapshot {
	return c.conversations
}

// Messages returns the selected conversation's transcript rows, oldest first.
// The slice is the cached snapshot Publish last rebuilt; callers must not
// mutate it.
func (c *Controller) Messages() []MessageSnapshot {
	return c.messages
}

// UnreadMarkRow reports the transcript row the "New messages" boundary sits
// above: the index into Messages() of the message whose sequence equals the
// conversation's unread mark. It returns -1 when there is no mark, or when the
// mark is on the first row, which has nothing above it to separate from. It
// mirrors MessageListModel::unreadMarkRow, whose buildView only inserts the
// boundary when a preceding row exists.
func (c *Controller) UnreadMarkRow() int {
	if c.selected == nil {
		return -1
	}
	conversation := c.reducer.Find(*c.selected)
	if conversation == nil || conversation.UnreadMark == nil {
		return -1
	}
	for index, message := range conversation.Messages {
		if message.Sequence != *conversation.UnreadMark {
			continue
		}
		if index <= 0 || index >= len(c.messages) {
			return -1
		}
		return index
	}
	return -1
}

// Members returns the selected channel's member rows in the reducer's
// orderedMembers order (PREFIX rank, then nick). A direct message has no
// members. The slice is the cached snapshot Publish last rebuilt; callers must
// not mutate it.
func (c *Controller) Members() []MemberSnapshot {
	return c.members
}

// rebuildConversations recomputes the sidebar snapshot. It is called only from
// Publish.
func (c *Controller) rebuildConversations() {
	keys := irc.SidebarOrderWithNetworks(c.reducer, c.networkOrder)
	now := c.now()
	rows := make([]ConversationSnapshot, 0, len(keys))
	for _, key := range keys {
		conversation := c.reducer.Find(key)
		if conversation == nil {
			continue
		}
		rows = append(rows, c.conversationSnapshot(key, conversation, now))
	}
	c.conversations = rows
}

func (c *Controller) conversationSnapshot(key irc.ConversationKey,
	conversation *irc.ConversationState, now time.Time) ConversationSnapshot {
	awayNotify := c.capabilities[key.NetworkID].Contains(irc.CapabilityAwayNotify)
	row := ConversationSnapshot{
		Conversation:     conversation.Target,
		Unread:           conversation.Unread,
		Mention:          conversation.Mentions > 0,
		Direct:           !conversation.IsChannel(),
		NetworkID:        key.NetworkID,
		ConversationID:   irc.ConversationID(key),
		ConversationName: conversation.Target,
		Typing:           c.reducer.DirectPeerIsTyping(key, now),
		Muted:            conversation.Muted,
		Presence:         conversationPresence(c.reducer, conversation, awayNotify),
	}
	// Avatar and bot are peer facts; a channel row leaves both at the zero
	// value, matching ConversationListModel.
	if !conversation.IsChannel() {
		facts := c.reducer.NickPresence(key.NetworkID, key.NormalizedTarget)
		row.Avatar = facts.Avatar()
		row.Bot = facts.IsBot()
	}
	return row
}

// conversationPresence maps the reducer's PeerPresence to the sidebar's
// presence role. A channel row carries no presence. A direct-message row only
// paints a dot when the network negotiated away-notify, the same gate that
// hides another member's away dot; without it the server never pushes AWAY, so
// a visible peer would look permanently online. It mirrors conversationPresence
// in conversationlistmodel.cpp, whose PeerUnknown case is the zero value here
// because Phase 2 leaves an unreachable peer unpainted.
func conversationPresence(reducer *irc.EventReducer, conversation *irc.ConversationState, awayNotify bool) string {
	if conversation.IsChannel() || !awayNotify {
		return ""
	}
	switch reducer.PeerPresence(conversation.Key.NetworkID, conversation.Key.NormalizedTarget) {
	case irc.PeerOnline:
		return "online"
	case irc.PeerAway:
		return "away"
	case irc.PeerUnknown:
		return ""
	}
	return ""
}

// rebuildMessages recomputes the selected transcript snapshot. It is called
// only from Publish.
func (c *Controller) rebuildMessages() {
	if c.selected == nil {
		c.messages = nil
		return
	}
	conversation := c.reducer.Find(*c.selected)
	if conversation == nil {
		c.messages = nil
		return
	}
	rows := make([]MessageSnapshot, 0, len(conversation.Messages))
	// The view renders and groups on Time, so it is handed a display clock:
	// the reducer's stored instant read in the clock's zone, matching
	// MessageListModel::displayTime's toLocalTime.
	location := c.displayLocation()
	for _, message := range conversation.Messages {
		rows = append(rows, MessageSnapshot{
			Author:    message.Author,
			Time:      message.Timestamp.In(location),
			Body:      message.Body,
			Kind:      messageKindName(message.Kind),
			Origin:    originName(message.Origin),
			MsgID:     message.MsgID.Value,
			Sequence:  message.Sequence,
			Mentioned: c.reducer.IsTranscriptHighlight(conversation.Key.NetworkID, message.Author, message.Kind, message.Body),
		})
	}
	c.messages = rows
}

// rebuildMembers recomputes the selected channel's member snapshot. It is
// called only from Publish.
func (c *Controller) rebuildMembers() {
	if c.selected == nil {
		c.members = nil
		return
	}
	ordered := c.reducer.OrderedMembers(*c.selected)
	rows := make([]MemberSnapshot, 0, len(ordered))
	for _, member := range ordered {
		view, ok := c.reducer.MemberView(*c.selected, member.Nick)
		if !ok {
			continue
		}
		rows = append(rows, MemberSnapshot{
			Nick:    view.Nick,
			Label:   view.Label,
			Status:  view.Status,
			Away:    view.IsAway(),
			Account: view.Account,
			Bot:     view.Bot,
		})
	}
	c.members = rows
}

// messageKindName maps a reducer MessageKind to the transcript role string. It
// mirrors kindString in messagelistmodel.cpp.
func messageKindName(kind irc.MessageKind) string {
	switch kind {
	case irc.KindAction:
		return "action"
	case irc.KindEvent, irc.KindError:
		return "event"
	case irc.KindWhois:
		return "whois"
	case irc.KindNotice:
		return "notice"
	case irc.KindMessage:
		return "message"
	}
	return "message"
}

// originName maps a reducer Origin to the transcript role string. It mirrors
// the OriginRole mapping in messagelistmodel.cpp.
func originName(origin irc.Origin) string {
	if origin == irc.OriginReplay {
		return "replay"
	}
	return "live"
}
