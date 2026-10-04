package irc

import (
	"strings"
	"time"
)

const readMarkerTimeLayout = "2006-01-02T15:04:05.000Z"

// ReadMarkerCommand returns the wire command for an enabled read-marker
// capability: MARKREAD for draft/read-marker, READ for soju.im/read.
func ReadMarkerCommand(negotiation *CapabilityNegotiation) string {
	if negotiation == nil {
		return ""
	}
	if negotiation.tokenEnabled("draft/read-marker") {
		return "MARKREAD"
	}
	if negotiation.tokenEnabled("soju.im/read") {
		return "READ"
	}
	return ""
}

func (n *CapabilityNegotiation) tokenEnabled(token string) bool {
	state, ok := n.stateOf(foldedName(token))
	return ok && state == tokenEnabled
}

// FormatReadMarkerTime renders a UTC timestamp for MARKREAD/READ set lines.
func FormatReadMarkerTime(when time.Time) string {
	return when.UTC().Format(readMarkerTimeLayout)
}

// ParseReadMarkerTime parses the timestamp= value from a read-marker line.
func ParseReadMarkerTime(value string) (time.Time, bool) {
	value = strings.TrimSpace(value)
	if value == "" {
		return time.Time{}, false
	}
	when, err := time.Parse(readMarkerTimeLayout, value)
	if err != nil {
		return time.Time{}, false
	}
	return when.UTC(), true
}

// ParseReadMarkerParameter parses the trailing parameter on a MARKREAD/READ
// line. It returns (nil, true) for *, (time, true) for timestamp=..., and
// (_, false) when the parameter is not a read marker.
func ParseReadMarkerParameter(parameter string) (*time.Time, bool) {
	parameter = strings.TrimSpace(parameter)
	if parameter == "*" {
		return nil, true
	}
	const prefix = "timestamp="
	if !strings.HasPrefix(parameter, prefix) {
		return nil, false
	}
	when, ok := ParseReadMarkerTime(parameter[len(prefix):])
	if !ok {
		return nil, false
	}
	return &when, true
}

func (r *EventReducer) coveredByReadMarker(conversation *ConversationState, msgTime time.Time) bool {
	if conversation == nil || conversation.ReadMarker == nil || msgTime.IsZero() {
		return false
	}
	return !msgTime.After(*conversation.ReadMarker)
}

// ApplyServerReadMarker stores a newer server read marker and recounts unread
// badges for lines at or before it. A nil marker is the server * sentinel and
// leaves state unchanged.
func (r *EventReducer) ApplyServerReadMarker(key ConversationKey, marker *time.Time) bool {
	if marker == nil {
		return false
	}
	conversation := r.findMutable(key)
	if conversation == nil {
		return false
	}
	if conversation.ReadMarker != nil && !marker.After(*conversation.ReadMarker) {
		return false
	}
	conversation.ReadMarker = marker
	r.recountUnreadFromReadMarker(conversation, key)
	return true
}

// LatestServerMessageTime returns the newest chat line timestamp that carries a
// server time, or false when none exist.
func LatestServerMessageTime(conversation *ConversationState) (time.Time, bool) {
	if conversation == nil {
		return time.Time{}, false
	}
	for index := len(conversation.Messages) - 1; index >= 0; index-- {
		message := conversation.Messages[index]
		if message.Timestamp.IsZero() {
			continue
		}
		switch message.Kind {
		case KindMessage, KindAction, KindNotice:
			return message.Timestamp, true
		default:
			continue
		}
	}
	return time.Time{}, false
}

func (r *EventReducer) recountUnreadFromReadMarker(conversation *ConversationState, key ConversationKey) {
	conversation.Unread = 0
	conversation.Mentions = 0
	conversation.UnreadMark = nil
	selected := r.selected != nil && *r.selected == key
	for _, message := range conversation.Messages {
		if message.Kind != KindMessage && message.Kind != KindAction && message.Kind != KindNotice {
			continue
		}
		if r.isSelf(key.NetworkID, message.Author) {
			continue
		}
		if r.coveredByReadMarker(conversation, message.Timestamp) {
			continue
		}
		nickHit := r.isNickMention(key.NetworkID, message.Body)
		highlightHit := !nickHit && r.isHighlightHit(key.NetworkID, message.Body)
		reason, hasReason := classifyChatLine(conversation, message.Kind, false, nickHit, highlightHit)
		skipLiveFocused := selected && message.Origin == OriginLive && !r.windowInactive
		skipReplayUnfocused := selected && message.Origin != OriginLive && r.windowInactive
		if skipLiveFocused || skipReplayUnfocused {
			continue
		}
		holdFocusedReplayMark := selected && message.Origin != OriginLive && conversation.UnreadMark != nil
		if conversation.Unread == 0 && !holdFocusedReplayMark {
			mark := message.Sequence
			conversation.UnreadMark = &mark
		}
		if selected && message.Origin != OriginLive {
			continue
		}
		conversation.Unread++
		if hasReason && (reason == chatLineNickMention || reason == chatLineHighlight) && !conversation.Muted {
			conversation.Mentions++
		}
	}
}
