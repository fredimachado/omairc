package irc

import (
	"fmt"
	"testing"
)

type recordingConversationLog struct {
	lines []TranscriptLine
}

func (l *recordingConversationLog) Append(networkID, target string, mapping CaseMapping, line TranscriptLine) bool {
	l.lines = append(l.lines, line)
	return true
}

func (l *recordingConversationLog) Prepend(networkID, target string, mapping CaseMapping, lines []TranscriptLine) bool {
	l.lines = append(lines, l.lines...)
	return true
}

func (l *recordingConversationLog) ReadTail(networkID, target string, mapping CaseMapping, maxLines int) []TranscriptLine {
	if maxLines <= 0 || len(l.lines) == 0 {
		return nil
	}
	out := make([]TranscriptLine, 0, maxLines)
	if len(l.lines) <= maxLines {
		return append(out, l.lines...)
	}
	return append(out, l.lines[len(l.lines)-maxLines:]...)
}

func TestHistoryPageCapTailSplicesOlderChatAtHead(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(MessageEvent{
		Conversation: room,
		Target:       "#room",
		Author:       "alice",
		Body:         "live",
		MsgID:        MsgID{Value: "live"},
	}, reducerTimestamp)
	conversation := stateOf(t, reducer, room)
	requireTrue(t, "join anchor", conversation.Channel().HistoryAnchor != nil)

	reducer.Apply(HistoryEvent{
		Conversation: room,
		Target:       "#room",
		Kind:         HistoryChat,
		OlderPage:    true,
		Lines:        []ReplayLine{replayLine("bob", "older", "old")},
	}, reducerTimestamp)

	requireInt(t, "messages", len(conversation.Messages), 3)
	requireString(t, "messages[0]", conversation.Messages[0].Body, "older")
	requireString(t, "messages[2]", conversation.Messages[2].Body, "live")
	requireTrue(t, "join anchor kept", conversation.Channel().HistoryAnchor != nil)
}

func TestHistoryPageCapTailSplicesDirectMessageAtHead(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	dm := reducer.ConversationKey(networkA, "alice")
	reducer.EnsureConversation(dm, "alice", CauseUserOpen)
	reducer.Apply(MessageEvent{
		Conversation: dm,
		Target:       "alice",
		Author:       "alice",
		Body:         "live",
		MsgID:        MsgID{Value: "live"},
	}, reducerTimestamp)

	reducer.Apply(HistoryEvent{
		Conversation: dm,
		Target:       "alice",
		Kind:         HistoryChat,
		OlderPage:    true,
		Lines:        []ReplayLine{replayLine("alice", "older", "old")},
	}, reducerTimestamp)

	conversation := stateOf(t, reducer, dm)
	requireInt(t, "messages", len(conversation.Messages), 2)
	requireString(t, "messages[0]", conversation.Messages[0].Body, "older")
}

func TestHistoryChatDoesNotReopenClosedDirectMessage(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	dm := reducer.ConversationKey(networkA, "alice")
	reducer.EnsureConversation(dm, "alice", CauseUserOpen)
	requireTrue(t, "drop", reducer.DropDirectMessage(dm))
	reducer.Apply(HistoryEvent{
		Conversation: dm,
		Target:       "alice",
		Kind:         HistoryChat,
		Lines:        []ReplayLine{replayLine("alice", "stale", "id")},
	}, reducerTimestamp)
	requireFalse(t, "conversation absent", reducer.Find(dm) != nil)
}

func TestHistoryPagePrependPersistsTranscriptOrder(t *testing.T) {
	log := &recordingConversationLog{}
	reducer := NewEventReducer()
	reducer.SetConversationLog(log)
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(MessageEvent{
		Conversation: room,
		Target:       "#room",
		Author:       "alice",
		Body:         "live",
		MsgID:        MsgID{Value: "live"},
	}, reducerTimestamp)

	reducer.Apply(HistoryEvent{
		Conversation: room,
		Target:       "#room",
		Kind:         HistoryChat,
		OlderPage:    true,
		Lines:        []ReplayLine{replayLine("bob", "older", "old")},
	}, reducerTimestamp)

	if len(log.lines) < 2 {
		t.Fatalf("log lines = %d, want at least 2", len(log.lines))
	}
	requireString(t, "log[0]", log.lines[0].Body, "older")
	requireString(t, "log tail", log.lines[len(log.lines)-1].Body, "live")
}

func TestOlderPageSplicesAtHeadWithoutTailCapFlag(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(MessageEvent{
		Conversation: room,
		Target:       "#room",
		Author:       "alice",
		Body:         "live",
		MsgID:        MsgID{Value: "live"},
	}, reducerTimestamp)
	conversation := stateOf(t, reducer, room)
	conversation.channel.HistoryAnchor = nil

	reducer.Apply(HistoryEvent{
		Conversation: room,
		Target:       "#room",
		Kind:         HistoryChat,
		OlderPage:    true,
		Lines:        []ReplayLine{replayLine("bob", "older", "old")},
	}, reducerTimestamp)

	requireString(t, "messages[0]", conversation.Messages[0].Body, "older")
}

func TestOlderPageReplaySkipsUnreadMark(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	selected := room
	reducer.MarkSelected(selected)
	conversation := stateOf(t, reducer, room)

	reducer.Apply(HistoryEvent{
		Conversation: room,
		Target:       "#room",
		Kind:         HistoryChat,
		OlderPage:    true,
		Lines:        []ReplayLine{replayLine("bob", "older", "old")},
	}, reducerTimestamp)

	requireFalse(t, "unread mark", conversation.UnreadMark != nil)
	requireInt(t, "unread", conversation.Unread, 0)
}

func TestOlderPageShiftsHistoryAnchorAfterDedupedLatest(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(MessageEvent{
		Conversation: room,
		Target:       "#room",
		Author:       "alice",
		Body:         "live",
		MsgID:        MsgID{Value: "live"},
	}, reducerTimestamp)
	conversation := stateOf(t, reducer, room)
	anchorBefore := conversation.Channel().HistoryAnchor.Sequence

	reducer.Apply(HistoryEvent{
		Conversation: room,
		Target:       "#room",
		Kind:         HistoryChat,
		Lines:        []ReplayLine{replayLine("alice", "live", "live")},
	}, reducerTimestamp)
	requireTrue(t, "join anchor kept", conversation.Channel().HistoryAnchor != nil)
	requireInt64(t, "anchor after deduped LATEST", conversation.Channel().HistoryAnchor.Sequence, anchorBefore)

	reducer.Apply(HistoryEvent{
		Conversation: room,
		Target:       "#room",
		Kind:         HistoryChat,
		OlderPage:    true,
		Lines:        []ReplayLine{replayLine("bob", "older", "old")},
	}, reducerTimestamp)
	requireInt64(t, "anchor after older page", conversation.Channel().HistoryAnchor.Sequence, anchorBefore+1)
}

func TestHistoryPageCapTailKeepsPrependedHeadAtMaxMessages(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	conversation := stateOf(t, reducer, room)
	conversation.Messages = make([]ReducedMessage, MaxMessages)
	conversation.MessageIDs = map[MsgID]struct{}{}
	for i := range conversation.Messages {
		msgid := MsgID{Value: fmt.Sprintf("fill-%d", i)}
		conversation.Messages[i] = ReducedMessage{
			Author:    "alice",
			Body:      fmt.Sprintf("fill-%d", i),
			MsgID:     msgid,
			Sequence:  int64(i + 1),
			Kind:      KindMessage,
			Origin:    OriginLive,
		}
		conversation.MessageIDs[msgid] = struct{}{}
	}
	conversation.NextSequence = int64(MaxMessages + 1)
	conversation.HistoryPageCapTail = true

	reducer.Apply(HistoryEvent{
		Conversation: room,
		Target:       "#room",
		Kind:         HistoryChat,
		OlderPage:    true,
		Lines:        []ReplayLine{replayLine("bob", "page-head", "page-head")},
	}, reducerTimestamp)

	requireInt(t, "messages after prepend", len(conversation.Messages), MaxMessages)
	requireString(t, "messages[0]", conversation.Messages[0].Body, "page-head")
}

func TestWelcomeClearsHistoryPageCapTail(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	conversation := stateOf(t, reducer, room)
	conversation.HistoryPageCapTail = true
	welcome(reducer, networkA)
	requireFalse(t, "tail cap cleared", conversation.HistoryPageCapTail)
}
