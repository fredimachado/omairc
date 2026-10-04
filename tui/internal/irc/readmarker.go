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
	return ReadMarkerTimeMillis(when).Format(readMarkerTimeLayout)
}

// ReadMarkerTimeMillis truncates a timestamp to whole UTC milliseconds for
// read-marker comparisons and outbound formatting.
func ReadMarkerTimeMillis(when time.Time) time.Time {
	if when.IsZero() {
		return time.Time{}
	}
	ms := when.UTC().UnixMilli()
	return time.Unix(0, ms*int64(time.Millisecond)).UTC()
}

// ReadMarkerTimeAfter reports whether a is strictly newer than b at millisecond
// precision.
func ReadMarkerTimeAfter(a, b time.Time) bool {
	if a.IsZero() || b.IsZero() {
		return false
	}
	return ReadMarkerTimeMillis(a).After(ReadMarkerTimeMillis(b))
}

// ParseReadMarkerTime parses the timestamp= value from a read-marker line.
// Fractional seconds may use any precision; only a trailing Z is accepted.
func ParseReadMarkerTime(value string) (time.Time, bool) {
	value = strings.TrimSpace(value)
	if value == "" || !strings.HasSuffix(value, "Z") {
		return time.Time{}, false
	}
	parsed, err := time.Parse(time.RFC3339Nano, value)
	if err != nil {
		parsed, err = time.Parse(time.RFC3339, value)
		if err != nil {
			return time.Time{}, false
		}
	}
	return ReadMarkerTimeMillis(parsed), true
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

func (r *EventReducer) coveredByReadMarker(conversation *ConversationState, serverTime *time.Time) bool {
	if conversation == nil || conversation.ReadMarker == nil || serverTime == nil || serverTime.IsZero() {
		return false
	}
	return !ReadMarkerTimeAfter(*serverTime, *conversation.ReadMarker)
}

// ApplyServerReadMarker stores a newer server read marker and recounts unread
// badges for lines at or before it. A nil marker is the server * sentinel and
// leaves state unchanged.
func (r *EventReducer) ApplyServerReadMarker(key ConversationKey, marker *time.Time) bool {
	if marker == nil {
		return false
	}
	normalized := ReadMarkerTimeMillis(*marker)
	marker = &normalized
	conversation := r.findMutable(key)
	if conversation == nil {
		return false
	}
	if conversation.ReadMarker != nil && !ReadMarkerTimeAfter(*marker, *conversation.ReadMarker) {
		return false
	}
	conversation.ReadMarker = marker
	r.recountUnreadFromReadMarker(conversation, key)
	return true
}

// LatestServerMessageTime returns the newest chat line server-time tag that
// carries a parsed server time, or false when none exist.
func LatestServerMessageTime(conversation *ConversationState) (time.Time, bool) {
	if conversation == nil {
		return time.Time{}, false
	}
	for index := len(conversation.Messages) - 1; index >= 0; index-- {
		message := conversation.Messages[index]
		if message.ServerTime == nil || message.ServerTime.IsZero() {
			continue
		}
		switch message.Kind {
		case KindMessage, KindAction, KindNotice:
			return ReadMarkerTimeMillis(*message.ServerTime), true
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
		if r.coveredByReadMarker(conversation, message.ServerTime) {
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
