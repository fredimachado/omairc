package irc

import (
	"sort"
	"strings"
)

// ConversationID builds the stable id for a conversation key: the network id,
// a newline, then the normalized target. It mirrors ircConversationId.
func ConversationID(key ConversationKey) string {
	return key.NetworkID + "\n" + key.NormalizedTarget
}

// ParseConversationID parses a ConversationID back into its key. ok is false
// unless the id holds exactly one newline with both sides non-empty. It mirrors
// ircParseConversationId.
func ParseConversationID(id string) (ConversationKey, bool) {
	separator := strings.IndexByte(id, '\n')
	if separator <= 0 || separator+1 >= len(id) {
		return ConversationKey{}, false
	}
	if strings.IndexByte(id[separator+1:], '\n') >= 0 {
		return ConversationKey{}, false
	}
	key := ConversationKey{
		NetworkID:        id[:separator],
		NormalizedTarget: id[separator+1:],
	}
	if key.NetworkID == "" || key.NormalizedTarget == "" {
		return ConversationKey{}, false
	}
	return key, true
}

// ConversationCauseInserts reports whether a cause may create a conversation.
// It is ported verbatim from ircConversationCauseInserts: the user opening a
// query inserts, channel state inserts channels, inbound lines from others
// insert channels and non-service queries, restore inserts non-service queries,
// and a local send or a self-authored inbound line never invents anything.
func ConversationCauseInserts(cause ConversationCause, targetIsChannel, targetLooksLikeService bool) bool {
	switch cause {
	case CauseUserOpen:
		return !targetIsChannel
	case CauseChannelState:
		return targetIsChannel
	case CauseInboundOther:
		return targetIsChannel || !targetLooksLikeService
	case CauseRestore:
		return !targetIsChannel && !targetLooksLikeService
	case CauseInboundSelf, CauseQuietSend:
		return false
	}
	return false
}

// TargetLooksLikeService reports whether a target names a network service
// rather than a person. It mirrors ircTargetLooksLikeService, which passes an
// empty host and the advertised CHANTYPES to ircIsServiceIdentity.
func TargetLooksLikeService(target string, features ServerFeatures) bool {
	return IsServiceIdentity(target, "", features.ChannelTypes())
}

// SidebarOrder returns every conversation in sidebar order, letting the
// reducer's insertion order define the network order. It mirrors the one
// argument ircSidebarOrder.
func SidebarOrder(reducer *EventReducer) []ConversationKey {
	return SidebarOrderWithNetworks(reducer, nil)
}

// SidebarOrderWithNetworks returns every conversation in sidebar order. An
// empty networkOrder builds one from the conversation keys, sorted; otherwise
// the given order wins. Within a network, joined channels come first, then
// parted channels, then direct messages, each group by case-insensitive
// display target and finally by conversation key. It mirrors the two argument
// ircSidebarOrder.
func SidebarOrderWithNetworks(reducer *EventReducer, networkOrder []string) []ConversationKey {
	conversations := reducer.Conversations()
	keys := make([]ConversationKey, 0, len(conversations))
	for key := range conversations {
		keys = append(keys, key)
	}

	order := append([]string(nil), networkOrder...)
	if len(order) == 0 {
		for _, key := range keys {
			if !containsString(order, key.NetworkID) {
				order = append(order, key.NetworkID)
			}
		}
		sort.Strings(order)
	}

	sort.Slice(keys, func(i, j int) bool {
		left, right := keys[i], keys[j]
		leftNetwork := networkRank(left.NetworkID, order)
		rightNetwork := networkRank(right.NetworkID, order)
		if leftNetwork != rightNetwork {
			return leftNetwork < rightNetwork
		}
		if left.NetworkID != right.NetworkID {
			return left.NetworkID < right.NetworkID
		}
		leftGroup := groupRank(reducer.Find(left))
		rightGroup := groupRank(reducer.Find(right))
		if leftGroup != rightGroup {
			return leftGroup < rightGroup
		}
		leftTarget := left.NormalizedTarget
		if state := reducer.Find(left); state != nil {
			leftTarget = state.Target
		}
		rightTarget := right.NormalizedTarget
		if state := reducer.Find(right); state != nil {
			rightTarget = state.Target
		}
		if name := compareCaseInsensitive(leftTarget, rightTarget); name != 0 {
			return name < 0
		}
		return conversationKeyLess(left, right)
	})
	return keys
}

// NeighborAfterDrop picks the conversation to select after dropping one. It
// prefers the next row in the same network, then the previous row in the same
// network, then the next row, then the previous row; a key that is not in the
// list falls back to the last row. ok is false when there is no candidate. It
// mirrors ircNeighborAfterDrop.
func NeighborAfterDrop(ordered []ConversationKey, dropping ConversationKey) (ConversationKey, bool) {
	n := len(ordered)
	index := -1
	for position, key := range ordered {
		if key == dropping {
			index = position
			break
		}
	}
	if index >= 0 {
		if index+1 < n && ordered[index+1].NetworkID == dropping.NetworkID {
			return ordered[index+1], true
		}
		if index > 0 && ordered[index-1].NetworkID == dropping.NetworkID {
			return ordered[index-1], true
		}
		if index+1 < n {
			return ordered[index+1], true
		}
		if index > 0 {
			return ordered[index-1], true
		}
		return ConversationKey{}, false
	}
	if n > 0 {
		return ordered[n-1], true
	}
	return ConversationKey{}, false
}

// conversationKeyLess orders conversation keys the way std::map does: network
// id, then normalized target.
func conversationKeyLess(left, right ConversationKey) bool {
	if left.NetworkID != right.NetworkID {
		return left.NetworkID < right.NetworkID
	}
	return left.NormalizedTarget < right.NormalizedTarget
}

func networkRank(networkID string, order []string) int {
	for index, candidate := range order {
		if candidate == networkID {
			return index
		}
	}
	return len(order)
}

// groupRank places joined channels first (0), then unjoined channels (1), then
// direct messages (2). It mirrors the anonymous groupRank in
// conversationlistmodel.cpp.
func groupRank(conversation *ConversationState) int {
	if conversation == nil || !conversation.IsChannel() {
		return 2
	}
	if conversation.Channel() != nil && conversation.Channel().Joined {
		return 0
	}
	return 1
}

// compareCaseInsensitive orders two display targets the way
// QString::compare(Qt::CaseInsensitive) does for ASCII: folded, byte by byte.
func compareCaseInsensitive(left, right string) int {
	foldedLeft := strings.ToLower(left)
	foldedRight := strings.ToLower(right)
	switch {
	case foldedLeft < foldedRight:
		return -1
	case foldedLeft > foldedRight:
		return 1
	}
	return 0
}

func containsString(values []string, wanted string) bool {
	for _, value := range values {
		if value == wanted {
			return true
		}
	}
	return false
}
