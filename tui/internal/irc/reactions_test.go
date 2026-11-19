package irc

import (
	"fmt"
	"strings"
	"testing"
)

// The Go twin of client/src/irc/reducer/tests/reactions.rs and the reply and
// reaction cases of client/src/irc/translator/tests/chat.rs.

func TestTranslatesReplyTagOnChatNoticeAndAction(t *testing.T) {
	events := translateLine(t, "@+draft/reply=parent1 :alice!u@h PRIVMSG #room :agreed")
	message, ok := events[0].(MessageEvent)
	requireTrue(t, "message event", ok)
	requireString(t, "reply to", message.ReplyTo.Value, "parent1")

	events = translateLine(t, "@+draft/reply=parent1 :alice!u@h PRIVMSG #room :\x01ACTION nods\x01")
	action, ok := events[0].(ActionEvent)
	requireTrue(t, "action event", ok)
	requireString(t, "action reply to", action.ReplyTo.Value, "parent1")

	// A nick-targeted notice from a person files into their direct message and
	// keeps the reply target.
	events = translateLine(t, "@+draft/reply=parent1 :alice!u@h NOTICE omairc :re: that")
	requireInt(t, "direct notice events", len(events), 1)
	notice, ok := events[0].(NoticeEvent)
	requireTrue(t, "direct notice event", ok)
	requireString(t, "direct notice conversation", notice.Conversation.NormalizedTarget, "alice")
	requireString(t, "notice reply to", notice.ReplyTo.Value, "parent1")

	// So does a channel-context notice delivered to you inside a channel.
	events = translateJoined(t, "@+draft/reply=parent1;+draft/channel-context=#room :bot!u@h NOTICE omairc :re: that", roomMemberLookup())
	requireInt(t, "channel notice events", len(events), 1)
	notice, ok = events[0].(NoticeEvent)
	requireTrue(t, "channel notice event", ok)
	requireString(t, "channel notice target", notice.Target, "#room")
	requireString(t, "channel notice reply to", notice.ReplyTo.Value, "parent1")

	events = translateLine(t, ":alice!u@h PRIVMSG #room :hello")
	requireString(t, "plain reply to", events[0].(MessageEvent).ReplyTo.Value, "")
	events = translateLine(t, "@+draft/reply="+strings.Repeat("m", 129)+" :alice!u@h PRIVMSG #room :hello")
	requireString(t, "overlong reply to", events[0].(MessageEvent).ReplyTo.Value, "")
	events = translateLine(t, "@+draft/reply= :alice!u@h PRIVMSG #room :hello")
	requireString(t, "empty reply to", events[0].(MessageEvent).ReplyTo.Value, "")
}

func TestTranslatesReactTagmsgAndUnreact(t *testing.T) {
	events := translateLine(t, "@+draft/reply=parent1;+draft/react=👍 :bob!u@h TAGMSG #room")
	requireInt(t, "react events", len(events), 1)
	reaction, ok := events[0].(ReactionEvent)
	requireTrue(t, "reaction event", ok)
	requireString(t, "nick", reaction.Nick, "bob")
	requireString(t, "emoji", reaction.Emoji, "👍")
	requireString(t, "target", reaction.TargetMsgID.Value, "parent1")
	requireFalse(t, "removed", reaction.Removed)
	requireString(t, "conversation", reaction.Conversation.NormalizedTarget, "#room")

	events = translateLine(t, "@+draft/reply=parent1;+draft/unreact=👍 :bob!u@h TAGMSG #room")
	reaction, ok = events[0].(ReactionEvent)
	requireTrue(t, "unreact event", ok)
	requireTrue(t, "unreact removed", reaction.Removed)

	events = translateLine(t, "@+draft/reply=parent1;+draft/react=🎉 :alice!u@h TAGMSG omairc")
	reaction, ok = events[0].(ReactionEvent)
	requireTrue(t, "direct reaction", ok)
	requireString(t, "direct conversation", reaction.Conversation.NormalizedTarget, "alice")
}

func TestTranslatesUnbelievableReactTagmsgToNothing(t *testing.T) {
	exact := "@+draft/reply=parent1;+draft/react=" + strings.Repeat("a", 64) + " :bob!u@h TAGMSG #room"
	requireInt(t, "64 bytes accepted", len(translateLine(t, exact)), 1)
	long := "@+draft/reply=parent1;+draft/react=" + strings.Repeat("a", 65) + " :bob!u@h TAGMSG #room"
	for _, line := range []string{
		long,
		"@+draft/react=👍 :bob!u@h TAGMSG #room",
		"@+draft/reply=parent1;+draft/react= :bob!u@h TAGMSG #room",
		"@+draft/reply=parent1;+draft/react=\\n :bob!u@h TAGMSG #room",
		"@+draft/reply=parent1;+draft/react=\x07 :bob!u@h TAGMSG #room",
		"@+draft/reply=parent1;+draft/react=a\x7f :bob!u@h TAGMSG #room",
		"@+draft/reply=parent1;+draft/react=a\u0085 :bob!u@h TAGMSG #room",
		"@+draft/reply=parent1;+draft/react=👍;+draft/unreact=👍 :bob!u@h TAGMSG #room",
		"@+draft/reply=a\\sb;+draft/react=👍 :bob!u@h TAGMSG #room",
		"@+draft/reply=parent1;+draft/react=👍 TAGMSG #room",
		"@+draft/reply=parent1 :bob!u@h TAGMSG #room",
		"@+foo=bar :bob!u@h TAGMSG #room",
	} {
		if events := translateLine(t, line); len(events) != 0 {
			t.Fatalf("Translate(%q) produced %d events, want 0", line, len(events))
		}
	}
}

func reactionEvent(key ConversationKey, nick, target, emoji string, removed bool) ReactionEvent {
	return ReactionEvent{
		Conversation: key,
		Nick:         nick,
		TargetMsgID:  MsgID{Value: target},
		Emoji:        emoji,
		Removed:      removed,
		ReceivedAt:   reducerTimestamp,
	}
}

func chatEvent(key ConversationKey, author, body, msgid, replyTo string) MessageEvent {
	return MessageEvent{
		Conversation: key,
		Author:       author,
		Body:         body,
		Timestamp:    reducerTimestamp,
		Target:       key.NormalizedTarget,
		MsgID:        MsgID{Value: msgid},
		ReplyTo:      MsgID{Value: replyTo},
	}
}

func roomWithLine() (*EventReducer, ConversationKey) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	key := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(chatEvent(key, "alice", "ship it", "m1", ""), reducerTimestamp)
	return reducer, key
}

func chipsOf(reducer *EventReducer, key ConversationKey, msgid string) []ReactionChip {
	return reducer.Find(key).Reactions[msgid].Chips()
}

func TestReplyKeepsTheParentAndAMissingParentIsNoError(t *testing.T) {
	reducer, key := roomWithLine()
	reducer.Apply(chatEvent(key, "bob", "agreed", "m2", "m1"), reducerTimestamp)
	reducer.Apply(chatEvent(key, "carol", "late", "m3", "gone"), reducerTimestamp)
	messages := stateOf(t, reducer, key).Messages
	byBody := func(body string) ReducedMessage {
		for _, message := range messages {
			if message.Body == body {
				return message
			}
		}
		t.Fatalf("missing body %q", body)
		return ReducedMessage{}
	}
	requireString(t, "reply", byBody("agreed").ReplyTo, "m1")
	requireString(t, "orphan", byBody("late").ReplyTo, "gone")
	requireString(t, "plain", byBody("ship it").ReplyTo, "")
}

func TestReactionAddsOncePerNickAcrossCaseMappingAndUnreactRemoves(t *testing.T) {
	reducer, key := roomWithLine()
	reducer.Apply(reactionEvent(key, "Bob[", "m1", "👍", false), reducerTimestamp)
	reducer.Apply(reactionEvent(key, "bob{", "m1", "👍", false), reducerTimestamp)
	chips := chipsOf(reducer, key, "m1")
	requireInt(t, "chips", len(chips), 1)
	requireInt(t, "count", chips[0].Count, 1)
	requireStrings(t, "nicks", chips[0].Nicks, []string{"Bob["})

	reducer.Apply(reactionEvent(key, "carol", "m1", "👍", false), reducerTimestamp)
	reducer.Apply(reactionEvent(key, "carol", "m1", "🎉", false), reducerTimestamp)
	chips = chipsOf(reducer, key, "m1")
	requireInt(t, "count", chips[0].Count, 2)
	requireString(t, "second emoji", chips[1].Emoji, "🎉")

	reducer.Apply(reactionEvent(key, "dave", "m1", "👍", true), reducerTimestamp)
	reducer.Apply(reactionEvent(key, "carol", "m1", "🔥", true), reducerTimestamp)
	requireInt(t, "no-op unreacts", len(chipsOf(reducer, key, "m1")), 2)

	reducer.Apply(reactionEvent(key, "BOB{", "m1", "👍", true), reducerTimestamp)
	requireStrings(t, "after unreact", chipsOf(reducer, key, "m1")[0].Nicks, []string{"carol"})
	reducer.Apply(reactionEvent(key, "carol", "m1", "🎉", true), reducerTimestamp)
	reducer.Apply(reactionEvent(key, "carol", "m1", "👍", true), reducerTimestamp)
	requireInt(t, "entry dropped", len(stateOf(t, reducer, key).Reactions), 0)
}

func TestReactionToUnknownMsgIDIsDroppedWithoutARow(t *testing.T) {
	reducer, key := roomWithLine()
	rows := len(stateOf(t, reducer, key).Messages)
	reducer.Apply(reactionEvent(key, "bob", "nope", "👍", false), reducerTimestamp)
	requireInt(t, "reactions", len(stateOf(t, reducer, key).Reactions), 0)
	requireInt(t, "rows", len(stateOf(t, reducer, key).Messages), rows)
}

func TestReactionToMembershipLineIsDropped(t *testing.T) {
	reducer, key := roomWithLine()
	reducer.Apply(HistoryEvent{
		Conversation: key,
		Target:       "#room",
		Kind:         HistoryChat,
		Lines:        []ReplayLine{{Body: "bob joined", Timestamp: reducerTimestamp, Kind: MessageKindEvent, MsgID: MsgID{Value: "e1"}}},
	}, reducerTimestamp)
	reducer.Apply(reactionEvent(key, "bob", "e1", "👍", false), reducerTimestamp)
	requireInt(t, "reactions", len(stateOf(t, reducer, key).Reactions), 0)
}

func TestReactionNeverOpensAConversation(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	direct := reducer.ConversationKey(networkA, "alice")
	channel := reducer.ConversationKey(networkA, "#nowhere")
	before := len(reducer.Conversations())
	reducer.Apply(reactionEvent(direct, "alice", "m1", "👍", false), reducerTimestamp)
	reducer.Apply(reactionEvent(channel, "alice", "m1", "👍", false), reducerTimestamp)
	requireInt(t, "conversations", len(reducer.Conversations()), before)
	requireTrue(t, "no direct", reducer.Find(direct) == nil)
	requireTrue(t, "no channel", reducer.Find(channel) == nil)
}

func TestReactionChangesNoUnreadMentionInboxOrLog(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	log := &recordingConversationLog{}
	reducer.SetConversationLog(log)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#other", Nick: "omairc"}, reducerTimestamp)
	key := reducer.ConversationKey(networkA, "#room")
	reducer.MarkSelected(reducer.ConversationKey(networkA, "#other"))
	reducer.Apply(chatEvent(key, "alice", "omairc: look", "m1", ""), reducerTimestamp)
	reducer.TakeMentionArrival()
	reducer.TakeInboxArrival()
	state := stateOf(t, reducer, key)
	unread, mentions := state.Unread, state.Mentions
	requireInt(t, "unread", unread, 1)
	logged := len(log.lines)

	event := reactionEvent(key, "bob", "m1", "👍", false)
	reducer.Apply(event, reducerTimestamp)
	notify := ClassifyViewNotify(event, reducer, nil)
	reducer.Apply(reactionEvent(key, "omairc", "m1", "❤", false), reducerTimestamp)

	state = stateOf(t, reducer, key)
	requireInt(t, "unread after", state.Unread, unread)
	requireInt(t, "mentions after", state.Mentions, mentions)
	if _, ok := reducer.TakeMentionArrival(); ok {
		t.Fatal("a reaction raised a mention arrival")
	}
	if _, ok := reducer.TakeInboxArrival(); ok {
		t.Fatal("a reaction raised an inbox arrival")
	}
	requireInt(t, "log lines", len(log.lines), logged)
	if notify != ViewNotifyTranscript() {
		t.Fatalf("notify = %+v, want transcript only", notify)
	}
	requireInt(t, "own reaction applies", len(chipsOf(reducer, key, "m1")), 2)
}

func TestDroppedReactionRepaintsNothing(t *testing.T) {
	reducer, key := roomWithLine()
	if n := ClassifyViewNotify(reactionEvent(key, "bob", "nope", "👍", false), reducer, nil); n != ViewNotifyNone() {
		t.Fatalf("unknown msgid notify = %+v", n)
	}
	missing := reducer.ConversationKey(networkA, "#nowhere")
	if n := ClassifyViewNotify(reactionEvent(missing, "bob", "m1", "👍", false), reducer, nil); n != ViewNotifyNone() {
		t.Fatalf("missing conversation notify = %+v", n)
	}
}

func TestReactionsAreBoundedPerLine(t *testing.T) {
	reducer, key := roomWithLine()
	for index := 0; index < MaxReactionEmojis+4; index++ {
		reducer.Apply(reactionEvent(key, "bob", "m1", fmt.Sprintf("e%d", index), false), reducerTimestamp)
	}
	requireInt(t, "emojis", len(chipsOf(reducer, key, "m1")), MaxReactionEmojis)
	for index := 0; index < MaxReactionNicks+4; index++ {
		reducer.Apply(reactionEvent(key, fmt.Sprintf("n%d", index), "m1", "e0", false), reducerTimestamp)
	}
	requireInt(t, "nicks", chipsOf(reducer, key, "m1")[0].Count, MaxReactionNicks)
}

func TestTrimmedLineTakesItsReactionsWithIt(t *testing.T) {
	reducer, key := roomWithLine()
	reducer.Apply(reactionEvent(key, "bob", "m1", "👍", false), reducerTimestamp)
	requireInt(t, "before", len(chipsOf(reducer, key, "m1")), 1)
	for index := 0; index < MaxMessages; index++ {
		reducer.Apply(chatEvent(key, "alice", fmt.Sprintf("filler %d", index), "", ""), reducerTimestamp)
	}
	state := stateOf(t, reducer, key)
	if _, known := state.MessageIDs[MsgID{Value: "m1"}]; known {
		t.Fatal("m1 was not trimmed")
	}
	requireInt(t, "reactions", len(state.Reactions), 0)
}

func TestMergedDirectMessagesKeepTheReactionsOfTheMovedLines(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	old := reducer.ConversationKey(networkA, "alice")
	next := reducer.ConversationKey(networkA, "alicia")
	reducer.Apply(chatEvent(next, "alicia", "already here", "m2", ""), reducerTimestamp)
	reducer.Apply(chatEvent(old, "alice", "hi", "m1", ""), reducerTimestamp)
	reducer.Apply(reactionEvent(old, "alice", "m1", "👍", false), reducerTimestamp)
	reducer.Apply(NickEvent{NetworkID: networkA, OldNick: "alice", NewNick: "alicia"}, reducerTimestamp)
	requireTrue(t, "merged", reducer.Find(old) == nil)
	requireInt(t, "moved chip", len(chipsOf(reducer, next, "m1")), 1)
}

func TestReactionKindOrderEndsWithReaction(t *testing.T) {
	requireInt(t, "nick facts", int(EventNickFacts), 20)
	requireInt(t, "reaction", int(EventReaction), 22)
}

func TestReplyKeepsItsParentAcrossARestartAndReactionsAreNotLogged(t *testing.T) {
	log := &recordingConversationLog{}
	first := NewEventReducer()
	first.SetConversationLog(log)
	welcome(first, networkA)
	first.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	applyWire(t, first, "@msgid=m1 :alice!u@h PRIVMSG #room :ship it")
	applyWire(t, first, "@msgid=m2;+draft/reply=m1 :bob!u@h PRIVMSG #room :agreed")
	applyWire(t, first, "@+draft/reply=m1;+draft/react=👍 :carol!u@h TAGMSG #room")
	requireInt(t, "logged lines", len(log.lines), 3)

	again := NewEventReducer()
	again.SetConversationLog(log)
	welcome(again, networkA)
	again.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	state := stateOf(t, again, again.ConversationKey(networkA, "#room"))
	found := false
	for _, message := range state.Messages {
		if message.Body == "agreed" {
			found = true
			requireString(t, "hydrated reply", message.ReplyTo, "m1")
		}
	}
	requireTrue(t, "hydrated line", found)
	requireInt(t, "reactions", len(state.Reactions), 0)
}
