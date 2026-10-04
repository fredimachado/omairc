package irc

import "testing"

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

	reducer.MarkHistoryPageCapTail(room)
	reducer.Apply(HistoryEvent{
		Conversation: room,
		Target:       "#room",
		Kind:         HistoryChat,
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

	reducer.MarkHistoryPageCapTail(dm)
	reducer.Apply(HistoryEvent{
		Conversation: dm,
		Target:       "alice",
		Kind:         HistoryChat,
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

	reducer.MarkHistoryPageCapTail(room)
	reducer.Apply(HistoryEvent{
		Conversation: room,
		Target:       "#room",
		Kind:         HistoryChat,
		Lines:        []ReplayLine{replayLine("bob", "older", "old")},
	}, reducerTimestamp)

	if len(log.lines) < 2 {
		t.Fatalf("log lines = %d, want at least 2", len(log.lines))
	}
	requireString(t, "log[0]", log.lines[0].Body, "older")
	requireString(t, "log tail", log.lines[len(log.lines)-1].Body, "live")
}
