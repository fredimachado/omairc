package irc

import (
	"fmt"
	"time"
)

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

// MaxInboxItems is the waiting-list cap. It mirrors IrcInbox::kMaxItems.
const MaxInboxItems = 50

// InboxItem is one inbox entry. It mirrors IrcInboxItem.
type InboxItem struct {
	Kind      InboxKind
	Timestamp time.Time
	NetworkID string
	Actor     string
	Target    string
	Preview   string
	MsgID     MsgID
}

// Inbox is a session waiting list, newest first. It mirrors IrcInbox.
type Inbox struct {
	items []InboxItem
}

// NewInbox returns an empty waiting list.
func NewInbox() *Inbox {
	return &Inbox{}
}

// Count reports the number of waiting entries.
func (in *Inbox) Count() int {
	return len(in.items)
}

// At returns the entry at index, or a zero InboxItem when index is out of
// range. It never panics, mirroring IrcInbox::at for callers that guard.
func (in *Inbox) At(index int) InboxItem {
	if index < 0 || index >= len(in.items) {
		return InboxItem{}
	}
	return in.items[index]
}

// Items returns a copy of the waiting list, newest first.
func (in *Inbox) Items() []InboxItem {
	return append([]InboxItem(nil), in.items...)
}

// Append inserts item at the front, deduplicating invites by channel and
// monitor-online rows by nick, then trims the tail to MaxInboxItems. It
// mirrors IrcInbox::append.
func (in *Inbox) Append(item InboxItem, mapping CaseMapping) {
	switch item.Kind {
	case InboxInvite:
		for index, existing := range in.items {
			if existing.Kind == InboxInvite && existing.NetworkID == item.NetworkID &&
				mapping.Equals(existing.Target, item.Target) {
				in.remove(index)
				break
			}
		}
	case InboxMonitorOnline:
		for index, existing := range in.items {
			if existing.Kind == InboxMonitorOnline && existing.NetworkID == item.NetworkID &&
				mapping.Equals(existing.Actor, item.Actor) {
				in.remove(index)
				break
			}
		}
	}

	in.items = append([]InboxItem{item}, in.items...)
	in.trimToCap()
}

// ConsumeAt removes the entry at index. It is a no-op when index is out of
// range, mirroring IrcInbox::consumeAt.
func (in *Inbox) ConsumeAt(index int) {
	if index < 0 || index >= len(in.items) {
		return
	}
	in.remove(index)
}

// ConsumeConversation removes every mention, highlight, direct, or kick row
// for target on networkID. It mirrors IrcInbox::consumeConversation.
func (in *Inbox) ConsumeConversation(networkID, target string, mapping CaseMapping) {
	in.consumeMatching(func(item InboxItem) bool {
		return item.NetworkID == networkID && isConversationKind(item.Kind) &&
			mapping.Equals(item.Target, target)
	})
}

// ConsumeInvite removes every invite row for channel on networkID. It mirrors
// IrcInbox::consumeInvite.
func (in *Inbox) ConsumeInvite(networkID, channel string, mapping CaseMapping) {
	in.consumeMatching(func(item InboxItem) bool {
		return item.Kind == InboxInvite && item.NetworkID == networkID &&
			mapping.Equals(item.Target, channel)
	})
}

// ConsumeMonitor removes every monitor-online row for nick on networkID. It
// mirrors IrcInbox::consumeMonitor.
func (in *Inbox) ConsumeMonitor(networkID, nick string, mapping CaseMapping) {
	in.consumeMatching(func(item InboxItem) bool {
		return item.Kind == InboxMonitorOnline && item.NetworkID == networkID &&
			mapping.Equals(item.Actor, nick)
	})
}

// PurgeNetwork removes every row for networkID. It mirrors
// IrcInbox::purgeNetwork.
func (in *Inbox) PurgeNetwork(networkID string) {
	in.consumeMatching(func(item InboxItem) bool {
		return item.NetworkID == networkID
	})
}

// KindName is the stable wire name for kind, mirroring
// IrcInboxModel::kindName. It returns "" for an unknown kind.
func KindName(kind InboxKind) string {
	switch kind {
	case InboxMention:
		return "mention"
	case InboxHighlight:
		return "highlight"
	case InboxDirect:
		return "direct"
	case InboxInvite:
		return "invite"
	case InboxMonitorOnline:
		return "monitorOnline"
	case InboxKick:
		return "kick"
	}
	return ""
}

// Label is the human-readable row text, mirroring IrcInboxModel::labelFor. It
// returns "" for an unknown kind.
func (item InboxItem) Label() string {
	switch item.Kind {
	case InboxMention:
		return fmt.Sprintf("%s mentioned you in %s", item.Actor, item.Target)
	case InboxHighlight:
		return fmt.Sprintf("Highlight in %s", item.Target)
	case InboxDirect:
		return fmt.Sprintf("Message from %s", item.Actor)
	case InboxInvite:
		return fmt.Sprintf("%s invited you to %s", item.Actor, item.Target)
	case InboxMonitorOnline:
		return fmt.Sprintf("%s is online", item.Actor)
	case InboxKick:
		return fmt.Sprintf("You were kicked from %s", item.Target)
	}
	return ""
}

// isConversationKind reports whether kind is cleared by consumeConversation.
// It mirrors the anonymous isConversationKind in ircinbox.cpp.
func isConversationKind(kind InboxKind) bool {
	switch kind {
	case InboxMention, InboxHighlight, InboxDirect, InboxKick:
		return true
	case InboxInvite, InboxMonitorOnline:
		return false
	}
	return false
}

// remove deletes the entry at index. The caller guarantees index is in range.
func (in *Inbox) remove(index int) {
	in.items = append(in.items[:index], in.items[index+1:]...)
}

// consumeMatching removes every entry that matches, preserving order.
func (in *Inbox) consumeMatching(match func(InboxItem) bool) {
	kept := in.items[:0]
	for _, item := range in.items {
		if !match(item) {
			kept = append(kept, item)
		}
	}
	in.items = kept
}

// trimToCap drops the oldest entries until the list fits MaxInboxItems,
// mirroring IrcInbox::trimToCap.
func (in *Inbox) trimToCap() {
	for len(in.items) > MaxInboxItems {
		in.items = in.items[:len(in.items)-1]
	}
}
