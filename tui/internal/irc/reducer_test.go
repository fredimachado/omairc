package irc

import (
	"slices"
	"strconv"
	"testing"
	"time"
)

const (
	networkA = "network-a"
	networkB = "network-b"
)

// reducerTimestamp is the fixed wall clock the ported reducer tests stamp onto
// live events, mirroring the C++ suite's `timestamp` constant.
var reducerTimestamp = time.Date(2026, time.September, 4, 0, 0, 0, 0, time.UTC)

// welcome seeds a registered network, defaulting the current nick to omairc
// like the C++ welcome() helper.
func welcome(reducer *EventReducer, networkID string, nicks ...string) {
	nick := "omairc"
	if len(nicks) > 0 {
		nick = nicks[0]
	}
	reducer.Apply(WelcomeEvent{NetworkID: networkID, CurrentNick: nick}, reducerTimestamp)
}

// parsedName parses one 353 NAMES token with the default feature set.
func parsedName(t *testing.T, token string) Name {
	t.Helper()
	features := NewServerFeatures()
	parsed, ok := features.ParseNamesToken(token)
	if !ok {
		t.Fatalf("ParseNamesToken(%q) failed", token)
	}
	return Name{Nick: parsed.Nick, Ranks: parsed.Ranks}
}

// mustParse parses a wire line that has to succeed.
func mustParse(t *testing.T, line string) Message {
	t.Helper()
	message, err := Parse(line)
	if err != nil {
		t.Fatalf("Parse(%q) failed: %v", line, err)
	}
	return message
}

// applyWire translates one wire line with the reducer's own features and folds
// the events in, mirroring the C++ applyWire helper.
func applyWire(t *testing.T, reducer *EventReducer, line string) {
	t.Helper()
	message := mustParse(t, line)
	for _, event := range Translate(networkA, "omairc", reducer.ServerFeatures(networkA), message, reducerTimestamp) {
		reducer.Apply(event, reducerTimestamp)
	}
}

// roomOf returns a channel on networkA, defaulting to #room.
func roomOf(reducer *EventReducer, channels ...string) *ConversationState {
	target := "#room"
	if len(channels) > 0 {
		target = channels[0]
	}
	return reducer.Find(reducer.ConversationKey(networkA, target))
}

// lastBody returns the body of the last row of a channel transcript.
func lastBody(reducer *EventReducer, channels ...string) string {
	conversation := roomOf(reducer, channels...)
	if conversation == nil || len(conversation.Messages) == 0 {
		return ""
	}
	return conversation.Messages[len(conversation.Messages)-1].Body
}

// stateOf returns a conversation that must exist.
func stateOf(t *testing.T, reducer *EventReducer, key ConversationKey) *ConversationState {
	t.Helper()
	conversation := reducer.Find(key)
	if conversation == nil {
		t.Fatalf("conversation %+v is missing", key)
	}
	return conversation
}

// replayLine builds one replay row with the fixed timestamp and no server
// time, mirroring the C++ replayLine helper.
func replayLine(author, body, msgid string) ReplayLine {
	return ReplayLine{
		Author:    author,
		Body:      body,
		Timestamp: reducerTimestamp,
		Kind:      MessageKindChat,
		MsgID:     MsgID{Value: msgid},
	}
}

func requireInt(t *testing.T, label string, got, want int) {
	t.Helper()
	if got != want {
		t.Fatalf("%s = %d, want %d", label, got, want)
	}
}

func requireInt64(t *testing.T, label string, got, want int64) {
	t.Helper()
	if got != want {
		t.Fatalf("%s = %d, want %d", label, got, want)
	}
}

func requireString(t *testing.T, label, got, want string) {
	t.Helper()
	if got != want {
		t.Fatalf("%s = %q, want %q", label, got, want)
	}
}

func requireTrue(t *testing.T, label string, got bool) {
	t.Helper()
	if !got {
		t.Fatalf("%s = false, want true", label)
	}
}

func requireFalse(t *testing.T, label string, got bool) {
	t.Helper()
	if got {
		t.Fatalf("%s = true, want false", label)
	}
}

func requireStrings(t *testing.T, label string, got, want []string) {
	t.Helper()
	if !slices.Equal(got, want) {
		t.Fatalf("%s = %q, want %q", label, got, want)
	}
}

func memberViewOf(t *testing.T, reducer *EventReducer, key ConversationKey, nick string) MemberView {
	t.Helper()
	view, ok := reducer.MemberView(key, nick)
	if !ok {
		t.Fatalf("MemberView(%+v, %q) is missing", key, nick)
	}
	return view
}

func TestNamesFillAndCompleteWithoutDuplicates(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)

	reducer.Apply(NamesEvent{
		NetworkID: networkA,
		Channel:   "#room",
		Names:     []Name{parsedName(t, "@Alice"), parsedName(t, "Bob")},
	}, reducerTimestamp)
	reducer.Apply(NamesEvent{
		NetworkID: networkA,
		Channel:   "#room",
		Names:     []Name{parsedName(t, "@ALICE"), parsedName(t, "+Carol")},
		Complete:  true,
	}, reducerTimestamp)
	reducer.Apply(JoinEvent{
		NetworkID: networkA, Channel: "#room", Nick: "carol"}, reducerTimestamp)

	conversation := stateOf(t, reducer, reducer.ConversationKey(networkA, "#room"))
	requireTrue(t, "isChannel", conversation.IsChannel())
	requireInt(t, "peopleCount", conversation.PeopleCount(), 3)
	requireFalse(t, "namesSyncing", conversation.Channel().NamesSyncing)
	if ranks := conversation.Channel().Members["alice"].Ranks; ranks != parsedName(t, "@Alice").Ranks {
		t.Fatalf("alice ranks = %b, want %b", ranks, parsedName(t, "@Alice").Ranks)
	}
	requireString(t, "alice label", memberViewOf(t, reducer, conversation.Key, "alice").Label, "@ALICE")
	requireString(t, "alice status", memberViewOf(t, reducer, conversation.Key, "alice").Status, "")
}

func TestNickAndQuitStayNetworkScoped(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	welcome(reducer, networkB)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Alice"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkB, Channel: "#room", Nick: "Alice"}, reducerTimestamp)

	reducer.Apply(NickEvent{NetworkID: networkA, OldNick: "ALICE", NewNick: "Alicia"}, reducerTimestamp)

	channelA := stateOf(t, reducer, reducer.ConversationKey(networkA, "#room"))
	channelB := stateOf(t, reducer, reducer.ConversationKey(networkB, "#room"))
	if _, ok := channelA.Channel().Members["alicia"]; !ok {
		t.Fatal("channelA must hold alicia")
	}
	if _, ok := channelA.Channel().Members["alice"]; ok {
		t.Fatal("channelA must not hold alice")
	}
	if _, ok := channelB.Channel().Members["alice"]; !ok {
		t.Fatal("channelB must still hold alice")
	}

	reducer.Apply(QuitEvent{NetworkID: networkA, Nick: "ALICIA", Reason: "gone"}, reducerTimestamp)
	requireInt(t, "channelA peopleCount", channelA.PeopleCount(), 0)
	requireInt(t, "channelB peopleCount", channelB.PeopleCount(), 1)
}

func TestSelfMembershipControlsChannelLifecycle(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#one", Nick: "OMAIRC"}, reducerTimestamp)

	one := stateOf(t, reducer, reducer.ConversationKey(networkA, "#one"))
	requireTrue(t, "joined", one.Channel().Joined)

	reducer.Apply(PartEvent{NetworkID: networkA, Channel: "#one", Nick: "omairc"}, reducerTimestamp)
	requireFalse(t, "joined after part", one.Channel().Joined)
	requireInt(t, "peopleCount after part", one.PeopleCount(), 0)

	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#two", Nick: "omairc"}, reducerTimestamp)
	two := stateOf(t, reducer, reducer.ConversationKey(networkA, "#two"))
	reducer.Apply(KickEvent{
		NetworkID: networkA,
		Channel:   "#two",
		Target:    "OMAIRC",
		Author:    "operator",
		Reason:    "bye",
	}, reducerTimestamp)
	requireFalse(t, "joined after kick", two.Channel().Joined)
	requireInt(t, "peopleCount after kick", two.PeopleCount(), 0)
}

func TestDirectMessagesUseCompositeKeys(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	welcome(reducer, networkB)
	aliceA := reducer.ConversationKey(networkA, "Alice")
	aliceB := reducer.ConversationKey(networkB, "Alice")

	reducer.Apply(MessageEvent{
		Conversation: aliceA, Author: "ALICE", Body: "hello",
		Timestamp: reducerTimestamp, Target: "Alice",
	}, reducerTimestamp)

	direct := stateOf(t, reducer, aliceA)
	requireFalse(t, "isChannel", direct.IsChannel())
	requireString(t, "target", direct.Target, "Alice")
	requireInt(t, "messages", len(direct.Messages), 1)
	requireFalse(t, "networkB direct", reducer.Find(aliceB) != nil)

	reducer.Apply(NickEvent{NetworkID: networkA, OldNick: "alice", NewNick: "Alicia"}, reducerTimestamp)
	requireFalse(t, "old key dropped", reducer.Find(aliceA) != nil)
	renamed := stateOf(t, reducer, reducer.ConversationKey(networkA, "alicia"))
	requireString(t, "renamed target", renamed.Target, "Alicia")
}

func TestNickAppendsEventToDirectMessage(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	alice := reducer.ConversationKey(networkA, "Alice")
	reducer.Apply(MessageEvent{
		Conversation: alice, Author: "Alice", Body: "hello",
		Timestamp: reducerTimestamp, Target: "Alice",
	}, reducerTimestamp)

	reducer.Apply(NickEvent{NetworkID: networkA, OldNick: "Alice", NewNick: "Alicia"}, reducerTimestamp)

	renamed := stateOf(t, reducer, reducer.ConversationKey(networkA, "Alicia"))
	requireString(t, "target", renamed.Target, "Alicia")
	requireInt(t, "messages", len(renamed.Messages), 2)
	last := renamed.Messages[len(renamed.Messages)-1]
	requireInt(t, "last kind", int(last.Kind), int(KindEvent))
	requireString(t, "last body", last.Body, "Alice is now Alicia")
}

func TestNickCaseOnlyUpdatesDirectDisplayNick(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	alice := reducer.ConversationKey(networkA, "Alice")
	reducer.Apply(MessageEvent{
		Conversation: alice, Author: "Alice", Body: "hello",
		Timestamp: reducerTimestamp, Target: "Alice",
	}, reducerTimestamp)

	reducer.Apply(NickEvent{NetworkID: networkA, OldNick: "Alice", NewNick: "ALICE"}, reducerTimestamp)

	same := stateOf(t, reducer, alice)
	requireString(t, "target", same.Target, "ALICE")
	requireInt(t, "messages", len(same.Messages), 2)
	requireString(t, "last body", same.Messages[len(same.Messages)-1].Body, "Alice is now ALICE")
}

func TestIdenticalChannelsStayIsolated(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	welcome(reducer, networkB)
	keyA := reducer.ConversationKey(networkA, "#same")
	keyB := reducer.ConversationKey(networkB, "#same")

	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#same", Nick: "Alice"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkB, Channel: "#same", Nick: "Bob"}, reducerTimestamp)
	reducer.Apply(MessageEvent{Conversation: keyA, Author: "Alice", Body: "only a", Timestamp: reducerTimestamp, Target: "#same"}, reducerTimestamp)
	reducer.Apply(MessageEvent{Conversation: keyB, Author: "Bob", Body: "only b", Timestamp: reducerTimestamp, Target: "#same"}, reducerTimestamp)

	channelA := stateOf(t, reducer, keyA)
	channelB := stateOf(t, reducer, keyB)
	requireInt(t, "channelA peopleCount", channelA.PeopleCount(), 1)
	requireInt(t, "channelB peopleCount", channelB.PeopleCount(), 1)
	requireString(t, "channelA last", channelA.Messages[len(channelA.Messages)-1].Body, "only a")
	requireString(t, "channelB last", channelB.Messages[len(channelB.Messages)-1].Body, "only b")
}

func TestAdvertisedChannelTypesCreateChannels(t *testing.T) {
	reducer := NewEventReducer()
	features := NewServerFeatures()
	features.ApplyToken("CHANTYPES=&")
	reducer.SetServerFeatures(networkA, features)
	welcome(reducer, networkA)

	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "&local", Nick: "omairc"}, reducerTimestamp)
	channel := stateOf(t, reducer, reducer.ConversationKey(networkA, "&local"))
	requireTrue(t, "isChannel", channel.IsChannel())
}

func TestUnreadMentionsRespectSelection(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA, "Potato[]")
	selected := reducer.ConversationKey(networkA, "#selected")
	background := reducer.ConversationKey(networkA, "#background")
	reducer.MarkSelected(selected)

	reducer.Apply(MessageEvent{
		Conversation: selected, Author: "Alice", Body: "Potato{}: selected",
		Timestamp: reducerTimestamp, Target: "#selected",
	}, reducerTimestamp)
	reducer.Apply(MessageEvent{
		Conversation: background, Author: "Alice", Body: "hello Potato{}",
		Timestamp: reducerTimestamp, Target: "#background",
	}, reducerTimestamp)
	reducer.Apply(MessageEvent{
		Conversation: background, Author: "Potato{}", Body: "my own text",
		Timestamp: reducerTimestamp, Target: "#background",
	}, reducerTimestamp)

	selectedState := stateOf(t, reducer, selected)
	backgroundState := stateOf(t, reducer, background)
	requireInt(t, "selected unread", selectedState.Unread, 0)
	requireInt(t, "selected mentions", selectedState.Mentions, 0)
	requireInt(t, "background unread", backgroundState.Unread, 1)
	requireInt(t, "background mentions", backgroundState.Mentions, 1)

	reducer.MarkSelected(background)
	requireInt(t, "background unread after select", backgroundState.Unread, 0)
	requireInt(t, "background mentions after select", backgroundState.Mentions, 0)
}

func TestWindowInactiveMarksSelectedChatUnread(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	selected := reducer.ConversationKey(networkA, "#selected")
	background := reducer.ConversationKey(networkA, "#background")
	reducer.MarkSelected(selected)

	reducer.Apply(MessageEvent{
		Conversation: selected, Author: "Alice", Body: "while-focused",
		Timestamp: reducerTimestamp, Target: "#selected",
	}, reducerTimestamp)
	selectedState := stateOf(t, reducer, selected)
	requireInt(t, "focused unread", selectedState.Unread, 0)
	requireFalse(t, "focused unreadMark", selectedState.UnreadMark != nil)

	reducer.SetWindowActive(false)
	reducer.Apply(MessageEvent{
		Conversation: selected, Author: "Bob", Body: "first-unfocused",
		Timestamp: reducerTimestamp, Target: "#selected",
	}, reducerTimestamp)
	reducer.Apply(MessageEvent{
		Conversation: selected, Author: "Bob", Body: "second-unfocused",
		Timestamp: reducerTimestamp, Target: "#selected",
	}, reducerTimestamp)
	requireInt(t, "unfocused unread", selectedState.Unread, 2)
	requireTrue(t, "unfocused unreadMark", selectedState.UnreadMark != nil)
	requireInt64(t, "unfocused unreadMark", *selectedState.UnreadMark, selectedState.Messages[1].Sequence)

	reducer.SetWindowActive(true)
	reducer.Apply(MessageEvent{
		Conversation: selected, Author: "Bob", Body: "while-focused-again",
		Timestamp: reducerTimestamp, Target: "#selected",
	}, reducerTimestamp)
	requireInt(t, "refocused unread", selectedState.Unread, 2)
	requireTrue(t, "refocused unreadMark", selectedState.UnreadMark != nil)

	// A background conversation still plants unread regardless of focus.
	reducer.Apply(MessageEvent{
		Conversation: background, Author: "Alice", Body: "background",
		Timestamp: reducerTimestamp, Target: "#background",
	}, reducerTimestamp)
	backgroundState := stateOf(t, reducer, background)
	requireInt(t, "background unread", backgroundState.Unread, 1)
	requireTrue(t, "background unreadMark", backgroundState.UnreadMark != nil)
}

func TestMarkReadConsumesUnreadButKeepsMark(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	selected := reducer.ConversationKey(networkA, "#selected")
	reducer.MarkSelected(selected)
	reducer.SetWindowActive(false)

	reducer.Apply(MessageEvent{
		Conversation: selected, Author: "Alice", Body: "omairc: away ping",
		Timestamp: reducerTimestamp, Target: "#selected",
	}, reducerTimestamp)
	selectedState := stateOf(t, reducer, selected)
	requireInt(t, "unread", selectedState.Unread, 1)
	requireInt(t, "mentions", selectedState.Mentions, 1)
	requireTrue(t, "unreadMark", selectedState.UnreadMark != nil)
	if _, ok := reducer.TakeMentionArrival(); !ok {
		t.Fatal("mention arrival is missing")
	}
	if _, ok := reducer.TakeInboxArrival(); ok {
		t.Fatal("selected conversation must not plant an inbox arrival")
	}

	requireTrue(t, "markRead changed", reducer.MarkRead(selected))
	requireInt(t, "unread after read", selectedState.Unread, 0)
	requireInt(t, "mentions after read", selectedState.Mentions, 0)
	requireTrue(t, "mark kept", selectedState.UnreadMark != nil)

	// A later selection (a real switch away and back) clears the mark.
	other := reducer.ConversationKey(networkA, "#other")
	reducer.MarkSelected(other)
	reducer.MarkSelected(selected)
	requireFalse(t, "mark cleared on reselect", selectedState.UnreadMark != nil)
}

func TestMentionArrivalSurvivesSelection(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	selected := reducer.ConversationKey(networkA, "#selected")
	reducer.MarkSelected(selected)

	reducer.Apply(MessageEvent{
		Conversation: selected, Author: "Alice", Body: "omairc: ping",
		Timestamp: reducerTimestamp, Target: "#selected",
	}, reducerTimestamp)

	mention, ok := reducer.TakeMentionArrival()
	requireTrue(t, "mention present", ok)
	requireString(t, "mention author", mention.Author, "Alice")
	requireString(t, "mention body", mention.Body, "omairc: ping")
	requireInt(t, "mentions", stateOf(t, reducer, selected).Mentions, 0)
	if _, ok := reducer.TakeMentionArrival(); ok {
		t.Fatal("mention arrival must be consumed once")
	}

	reducer.Apply(MessageEvent{
		Conversation: selected, Author: "Alice", Body: "no nick here",
		Timestamp: reducerTimestamp, Target: "#selected",
	}, reducerTimestamp)
	if _, ok := reducer.TakeMentionArrival(); ok {
		t.Fatal("plain chat must not mention")
	}

	reducer.Apply(MessageEvent{
		Conversation: selected, Author: "omairc", Body: "omairc: self",
		Timestamp: reducerTimestamp, Target: "#selected",
	}, reducerTimestamp)
	if _, ok := reducer.TakeMentionArrival(); ok {
		t.Fatal("self chat must not mention")
	}
}

func TestMentionArrivalOnDirectMessage(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	dm := reducer.ConversationKey(networkA, "Alice")
	room := reducer.ConversationKey(networkA, "#room")

	reducer.Apply(MessageEvent{
		Conversation: dm, Author: "Alice", Body: "hello",
		Timestamp: reducerTimestamp, Target: "Alice",
	}, reducerTimestamp)
	unselected, ok := reducer.TakeMentionArrival()
	requireTrue(t, "unselected mention", ok)
	requireString(t, "unselected author", unselected.Author, "Alice")
	requireString(t, "unselected body", unselected.Body, "hello")
	requireInt(t, "dm mentions", stateOf(t, reducer, dm).Mentions, 0)
	requireInt(t, "dm unread", stateOf(t, reducer, dm).Unread, 1)

	reducer.MarkSelected(dm)
	reducer.Apply(MessageEvent{
		Conversation: dm, Author: "Alice", Body: "hello",
		Timestamp: reducerTimestamp, Target: "Alice",
	}, reducerTimestamp)
	if _, ok := reducer.TakeMentionArrival(); !ok {
		t.Fatal("selected direct message must still plant a mention arrival")
	}
	requireInt(t, "selected dm mentions", stateOf(t, reducer, dm).Mentions, 0)
	requireInt(t, "selected dm unread", stateOf(t, reducer, dm).Unread, 0)

	reducer.Apply(MessageEvent{
		Conversation: room, Author: "Alice", Body: "hello",
		Timestamp: reducerTimestamp, Target: "#room",
	}, reducerTimestamp)
	if _, ok := reducer.TakeMentionArrival(); ok {
		t.Fatal("channel chat without a nick hit must not mention")
	}

	reducer.ClearSelection()
	reducer.Apply(MessageEvent{
		Conversation: dm, Author: "Alice", Body: "hey omairc",
		Timestamp: reducerTimestamp, Target: "Alice",
	}, reducerTimestamp)
	if _, ok := reducer.TakeMentionArrival(); !ok {
		t.Fatal("nick mention must plant an arrival")
	}
	requireInt(t, "dm mentions after nick mention", stateOf(t, reducer, dm).Mentions, 1)

	reducer.Apply(ActionEvent{
		Conversation: dm, Author: "Alice", Body: "waves",
		Timestamp: reducerTimestamp, Target: "Alice",
	}, reducerTimestamp)
	if _, ok := reducer.TakeMentionArrival(); !ok {
		t.Fatal("action must plant a direct arrival")
	}

	reducer.Apply(NoticeEvent{
		Conversation: dm, Author: "Alice", Body: "hello",
		Timestamp: reducerTimestamp, Target: "Alice",
	}, reducerTimestamp)
	if _, ok := reducer.TakeMentionArrival(); ok {
		t.Fatal("notice must not mention")
	}

	reducer.Apply(MessageEvent{
		Conversation: dm, Author: "omairc", Body: "hello",
		Timestamp: reducerTimestamp, Target: "Alice",
	}, reducerTimestamp)
	if _, ok := reducer.TakeMentionArrival(); ok {
		t.Fatal("self direct message must not mention")
	}
}

func TestMentionArrivalCarriesNetworkTargetAndMsgid(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#omarchy")
	reducer.Apply(MessageEvent{
		Conversation: room, Author: "Alice", Body: "omairc: ping",
		Timestamp: reducerTimestamp, Target: "#omarchy",
		MsgID: MsgID{Value: "mid-1"},
	}, reducerTimestamp)

	mention, ok := reducer.TakeMentionArrival()
	requireTrue(t, "mention present", ok)
	requireString(t, "author", mention.Author, "Alice")
	requireString(t, "body", mention.Body, "omairc: ping")
	requireString(t, "network", mention.NetworkID, networkA)
	requireString(t, "target", mention.Target, "#omarchy")
	requireString(t, "msgid", mention.MsgID.Value, "mid-1")

	dm := reducer.ConversationKey(networkA, "Alice")
	reducer.Apply(MessageEvent{
		Conversation: dm, Author: "Alice", Body: "hello",
		Timestamp: reducerTimestamp, Target: "Alice",
		MsgID: MsgID{Value: "dm-7"},
	}, reducerTimestamp)
	direct, ok := reducer.TakeMentionArrival()
	requireTrue(t, "direct present", ok)
	requireString(t, "direct author", direct.Author, "Alice")
	requireString(t, "direct body", direct.Body, "hello")
	requireString(t, "direct network", direct.NetworkID, networkA)
	requireString(t, "direct target", direct.Target, "Alice")
	requireString(t, "direct msgid", direct.MsgID.Value, "dm-7")
}

func TestMutedChatDoesNotMention(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.SetMuted(room, true)
	reducer.Apply(MessageEvent{
		Conversation: room, Author: "Alice", Body: "omairc: ping",
		Timestamp: reducerTimestamp, Target: "#room",
	}, reducerTimestamp)

	if _, ok := reducer.TakeMentionArrival(); ok {
		t.Fatal("muted chat must not mention")
	}
	muted := stateOf(t, reducer, room)
	requireTrue(t, "muted", muted.Muted)
	requireInt(t, "mentions", muted.Mentions, 0)
	requireInt(t, "unread", muted.Unread, 1)
	requireString(t, "last body", muted.Messages[len(muted.Messages)-1].Body, "omairc: ping")

	reducer.MarkSelected(room)
	requireTrue(t, "muted after select", reducer.Find(room).Muted)
	requireInt(t, "unread after select", reducer.Find(room).Unread, 0)

	reducer.ClearSelection()
	reducer.SetMuted(room, false)
	requireFalse(t, "unmuted", reducer.Find(room).Muted)
	reducer.Apply(MessageEvent{
		Conversation: room, Author: "Alice", Body: "omairc: back",
		Timestamp: reducerTimestamp, Target: "#room",
	}, reducerTimestamp)
	if _, ok := reducer.TakeMentionArrival(); !ok {
		t.Fatal("unmuted chat must mention again")
	}
	requireInt(t, "mentions after unmute", reducer.Find(room).Mentions, 1)
}

func TestHighlightWordMentionsLikeNick(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA, "fred")
	reducer.SetHighlightWords(networkA, []string{"omairc"})
	selected := reducer.ConversationKey(networkA, "#selected")
	background := reducer.ConversationKey(networkA, "#background")
	reducer.MarkSelected(selected)

	reducer.Apply(MessageEvent{
		Conversation: background, Author: "Alice", Body: "please review omairc",
		Timestamp: reducerTimestamp, Target: "#background",
	}, reducerTimestamp)
	requireInt(t, "unread", stateOf(t, reducer, background).Unread, 1)
	requireInt(t, "mentions", stateOf(t, reducer, background).Mentions, 1)
	hit, ok := reducer.TakeMentionArrival()
	requireTrue(t, "highlight arrival", ok)
	requireString(t, "highlight author", hit.Author, "Alice")
	requireString(t, "highlight body", hit.Body, "please review omairc")

	reducer.Apply(MessageEvent{
		Conversation: background, Author: "Alice", Body: "please review omaircd",
		Timestamp: reducerTimestamp, Target: "#background",
	}, reducerTimestamp)
	requireInt(t, "unread after word-continuation", stateOf(t, reducer, background).Unread, 2)
	requireInt(t, "mentions after word-continuation", stateOf(t, reducer, background).Mentions, 1)
	if _, ok := reducer.TakeMentionArrival(); ok {
		t.Fatal("a word continuation must not mention")
	}

	reducer.SetHighlightWords(networkA, []string{"deploy"})
	reducer.Apply(MessageEvent{
		Conversation: background, Author: "Alice", Body: "please review deploy",
		Timestamp: reducerTimestamp, Target: "#background",
	}, reducerTimestamp)
	requireInt(t, "mentions after replacement word", stateOf(t, reducer, background).Mentions, 2)
	if _, ok := reducer.TakeMentionArrival(); !ok {
		t.Fatal("the replacement word must mention")
	}

	reducer.Apply(MessageEvent{
		Conversation: background, Author: "Alice", Body: "please review deployment",
		Timestamp: reducerTimestamp, Target: "#background",
	}, reducerTimestamp)
	requireInt(t, "mentions after deployment", stateOf(t, reducer, background).Mentions, 2)
	if _, ok := reducer.TakeMentionArrival(); ok {
		t.Fatal("deployment must not mention for the word deploy")
	}

	reducer.SetHighlightWords(networkA, nil)
	reducer.Apply(MessageEvent{
		Conversation: background, Author: "Alice", Body: "fred: still a nick",
		Timestamp: reducerTimestamp, Target: "#background",
	}, reducerTimestamp)
	requireInt(t, "mentions after clearing words", stateOf(t, reducer, background).Mentions, 3)
	if _, ok := reducer.TakeMentionArrival(); !ok {
		t.Fatal("the nick must still mention")
	}
}

func TestWelcomeResetsMembership(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Alice"}, reducerTimestamp)

	conversation := stateOf(t, reducer, reducer.ConversationKey(networkA, "#room"))
	requireInt(t, "peopleCount", conversation.PeopleCount(), 2)
	requireTrue(t, "joined", conversation.Channel().Joined)
	requireTrue(t, "historyAnchor", conversation.Channel().HistoryAnchor != nil)

	welcome(reducer, networkA)
	requireInt(t, "peopleCount after welcome", conversation.PeopleCount(), 0)
	requireFalse(t, "joined after welcome", conversation.Channel().Joined)
	requireFalse(t, "historyAnchor after welcome", conversation.Channel().HistoryAnchor != nil)

	// A stale history line must not splice into the reset (unjoined) channel.
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(HistoryEvent{
		Conversation: room,
		Target:       "#room",
		Lines:        []ReplayLine{replayLine("alice", "stale", "id-welcome")},
	}, reducerTimestamp)
	requireInt(t, "messages", len(conversation.Messages), 2)
	requireString(t, "messages[0]", conversation.Messages[0].Body, "omairc joined")
	requireString(t, "messages[1]", conversation.Messages[1].Body, "Alice joined")
}

func TestAwayIsOneFactVisibleInEveryChannel(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	omarchy := reducer.ConversationKey(networkA, "#omarchy")
	desktop := reducer.ConversationKey(networkA, "#desktop")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "Alice"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#desktop", Nick: "Alice"}, reducerTimestamp)

	reducer.Apply(AwayEvent{NetworkID: networkA, Nick: "alice", Away: &Away{Reason: "lunch"}}, reducerTimestamp)
	reducer.Apply(AwayEvent{NetworkID: networkA, Nick: "alice", Away: &Away{Reason: "lunch"}}, reducerTimestamp)
	requireTrue(t, "omarchy away", memberViewOf(t, reducer, omarchy, "alice").IsAway())
	requireTrue(t, "desktop away", memberViewOf(t, reducer, desktop, "alice").IsAway())
	requireString(t, "away reason", memberViewOf(t, reducer, omarchy, "alice").Away.Reason, "lunch")

	reducer.Apply(NickEvent{NetworkID: networkA, OldNick: "Alice", NewNick: "Alicia"}, reducerTimestamp)
	requireTrue(t, "omarchy away after nick", memberViewOf(t, reducer, omarchy, "alicia").IsAway())
	requireTrue(t, "desktop away after nick", memberViewOf(t, reducer, desktop, "alicia").IsAway())

	reducer.Apply(AwayEvent{NetworkID: networkA, Nick: "Alicia", Away: nil}, reducerTimestamp)
	requireFalse(t, "omarchy back", memberViewOf(t, reducer, omarchy, "alicia").IsAway())
	requireFalse(t, "desktop back", memberViewOf(t, reducer, desktop, "alicia").IsAway())
}

func TestMetadataStatusIsSeparateFromPrefixModes(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(NamesEvent{
		NetworkID: networkA, Channel: "#room",
		Names: []Name{parsedName(t, "@Alice")}, Complete: true,
	}, reducerTimestamp)
	reducer.Apply(MemberMetadataEvent{
		NetworkID: networkA, Nick: "Alice", Key: "status", Value: "writing docs",
	}, reducerTimestamp)

	member := memberViewOf(t, reducer, room, "alice")
	requireString(t, "status", member.Status, "writing docs")
	requireString(t, "label", member.Label, "@Alice")
	if member.Ranks != parsedName(t, "@Alice").Ranks {
		t.Fatalf("ranks = %b, want %b", member.Ranks, parsedName(t, "@Alice").Ranks)
	}
	requireFalse(t, "away", member.IsAway())

	reducer.Apply(MemberMetadataEvent{
		NetworkID: networkA, Nick: "Alice", Key: "status", Value: "",
	}, reducerTimestamp)
	requireString(t, "cleared status", memberViewOf(t, reducer, room, "alice").Status, "")
}

func TestMetadataKeysStoreIndependentlyOfAwayAndPrefix(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(NamesEvent{
		NetworkID: networkA, Channel: "#room",
		Names: []Name{parsedName(t, "@Alice")}, Complete: true,
	}, reducerTimestamp)
	reducer.Apply(AwayEvent{NetworkID: networkA, Nick: "Alice", Away: &Away{Reason: "lunch"}}, reducerTimestamp)
	reducer.Apply(MemberMetadataEvent{NetworkID: networkA, Nick: "Alice", Key: "avatar", Value: "https://example.com/a.png"}, reducerTimestamp)
	reducer.Apply(MemberMetadataEvent{NetworkID: networkA, Nick: "Alice", Key: "bot", Value: "PacketBot"}, reducerTimestamp)
	reducer.Apply(MemberMetadataEvent{NetworkID: networkA, Nick: "Alice", Key: "display-name", Value: "Anna Docs"}, reducerTimestamp)
	reducer.Apply(MemberMetadataEvent{NetworkID: networkA, Nick: "Alice", Key: "status", Value: "writing docs"}, reducerTimestamp)

	member := memberViewOf(t, reducer, room, "alice")
	requireTrue(t, "away", member.IsAway())
	requireString(t, "label", member.Label, "@Alice")
	requireString(t, "status", member.Status, "writing docs")
	requireString(t, "avatar", member.Avatar, "https://example.com/a.png")
	requireTrue(t, "bot", member.Bot)
	requireString(t, "displayName", member.DisplayName, "Anna Docs")

	facts := reducer.NickPresence(networkA, "ALICE")
	requireTrue(t, "facts away", facts.Away != nil)
	requireString(t, "facts avatar", facts.Avatar(), "https://example.com/a.png")
	requireTrue(t, "facts bot", facts.IsBot())
	requireString(t, "facts display-name", facts.Metadata(DisplayNameKey()), "Anna Docs")

	reducer.Apply(MemberMetadataEvent{NetworkID: networkA, Nick: "Alice", Key: "bot", Value: ""}, reducerTimestamp)
	afterClear := memberViewOf(t, reducer, room, "alice")
	requireFalse(t, "bot after clear", afterClear.Bot)
	requireString(t, "status after clear", afterClear.Status, "writing docs")
	requireString(t, "avatar after clear", afterClear.Avatar, "https://example.com/a.png")
	requireString(t, "displayName after clear", afterClear.DisplayName, "Anna Docs")
	requireTrue(t, "away after clear", afterClear.IsAway())
	requireString(t, "label after clear", afterClear.Label, "@Alice")
}

func TestPresenceIsDroppedWithTheLastChannelAndOnWelcome(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	omarchy := reducer.ConversationKey(networkA, "#omarchy")
	desktop := reducer.ConversationKey(networkA, "#desktop")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "Alice"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#desktop", Nick: "Alice"}, reducerTimestamp)
	reducer.Apply(AwayEvent{NetworkID: networkA, Nick: "Alice", Away: &Away{Reason: "lunch"}}, reducerTimestamp)

	reducer.Apply(PartEvent{NetworkID: networkA, Channel: "#desktop", Nick: "Alice"}, reducerTimestamp)
	if _, ok := reducer.MemberView(desktop, "alice"); ok {
		t.Fatal("desktop must not hold alice after the part")
	}
	requireTrue(t, "omarchy away", memberViewOf(t, reducer, omarchy, "alice").IsAway())

	reducer.Apply(QuitEvent{NetworkID: networkA, Nick: "Alice"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "Alice"}, reducerTimestamp)
	requireFalse(t, "away forgotten on quit", memberViewOf(t, reducer, omarchy, "alice").IsAway())

	reducer.Apply(AwayEvent{NetworkID: networkA, Nick: "Alice", Away: &Away{Reason: "lunch"}}, reducerTimestamp)
	welcome(reducer, networkA)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "Alice"}, reducerTimestamp)
	requireFalse(t, "away cleared on welcome", memberViewOf(t, reducer, omarchy, "alice").IsAway())
}

func TestLosingACapabilityClearsTheFactsItFed(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Alice"}, reducerTimestamp)
	reducer.Apply(AwayEvent{NetworkID: networkA, Nick: "Alice", Away: &Away{Reason: "lunch"}}, reducerTimestamp)
	reducer.Apply(MemberMetadataEvent{NetworkID: networkA, Nick: "Alice", Key: "status", Value: "writing docs"}, reducerTimestamp)

	reducer.ClearPresenceFacts(networkA, true, false)
	requireFalse(t, "away cleared", memberViewOf(t, reducer, room, "alice").IsAway())
	requireString(t, "status kept", memberViewOf(t, reducer, room, "alice").Status, "writing docs")

	reducer.ClearPresenceFacts(networkA, false, true)
	requireString(t, "status cleared", memberViewOf(t, reducer, room, "alice").Status, "")
}

func TestModeEditsExistingRowsOnly(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(NamesEvent{
		NetworkID: networkA, Channel: "#room",
		Names: []Name{parsedName(t, "Alice")}, Complete: true,
	}, reducerTimestamp)

	applyMode := func(mode string, argument string) {
		reducer.Apply(ModeEvent{
			NetworkID: networkA, Target: "#room", Author: "op",
			Mode: mode, Arguments: []string{argument},
		}, reducerTimestamp)
	}
	applyMode("+o", "alice")
	requireString(t, "after +o", memberViewOf(t, reducer, room, "alice").Label, "@Alice")
	applyMode("+o", "alice")
	requireString(t, "after repeat +o", memberViewOf(t, reducer, room, "alice").Label, "@Alice")
	applyMode("+v", "alice")
	requireString(t, "after +v", memberViewOf(t, reducer, room, "alice").Label, "@Alice")
	applyMode("-o", "alice")
	requireString(t, "after -o", memberViewOf(t, reducer, room, "alice").Label, "+Alice")
	applyMode("-v", "alice")
	requireString(t, "after -v", memberViewOf(t, reducer, room, "alice").Label, "Alice")
	applyMode("+o", "ghost")
	if _, ok := reducer.MemberView(room, "ghost"); ok {
		t.Fatal("mode must not invent a member")
	}
	requireInt(t, "peopleCount", stateOf(t, reducer, room).PeopleCount(), 1)
}

func TestJoinOfListedNickKeepsRanks(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(NamesEvent{
		NetworkID: networkA, Channel: "#room",
		Names: []Name{parsedName(t, "@+Alice")}, Complete: true,
	}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "alice"}, reducerTimestamp)

	member := memberViewOf(t, reducer, room, "alice")
	requireString(t, "label", member.Label, "@alice")
	requireString(t, "nick", member.Nick, "alice")
}

func TestDropDirectMessageErasesOnlyDirectRows(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	lena := reducer.ConversationKey(networkA, "lena")
	channel := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(MessageEvent{Conversation: lena, Author: "lena", Body: "hi", Timestamp: reducerTimestamp, Target: "lena"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	reducer.MarkSelected(lena)

	requireInt(t, "conversations", len(reducer.Conversations()), 2)
	requireTrue(t, "drop direct", reducer.DropDirectMessage(lena))
	requireFalse(t, "direct gone", reducer.Find(lena) != nil)
	requireTrue(t, "channel kept", reducer.Find(channel) != nil)
	requireTrue(t, "channel is channel", reducer.Find(channel).IsChannel())
	requireInt(t, "conversations after drop", len(reducer.Conversations()), 1)
	requireFalse(t, "second drop", reducer.DropDirectMessage(lena))
	requireFalse(t, "channel is not direct", reducer.DropDirectMessage(channel))
	requireTrue(t, "channel still kept", reducer.Find(channel) != nil)
	requireInt(t, "conversations after rejected drop", len(reducer.Conversations()), 1)

	reducer.Apply(MessageEvent{Conversation: lena, Author: "lena", Body: "again", Timestamp: reducerTimestamp, Target: "lena"}, reducerTimestamp)
	recreated := stateOf(t, reducer, lena)
	requireInt(t, "recreated unread", recreated.Unread, 1)
}

func TestDropChannelErasesOnlyChannelRows(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	lena := reducer.ConversationKey(networkA, "lena")
	channel := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(MessageEvent{Conversation: lena, Author: "lena", Body: "hi", Timestamp: reducerTimestamp, Target: "lena"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	reducer.MarkSelected(channel)

	requireInt(t, "conversations", len(reducer.Conversations()), 2)
	requireTrue(t, "drop channel", reducer.DropChannel(channel))
	requireFalse(t, "channel gone", reducer.Find(channel) != nil)
	requireTrue(t, "direct kept", reducer.Find(lena) != nil)
	requireFalse(t, "direct not channel", reducer.Find(lena).IsChannel())
	requireInt(t, "conversations after drop", len(reducer.Conversations()), 1)
	_, selected := reducer.Selected()
	requireFalse(t, "selection cleared", selected)
	requireFalse(t, "second drop", reducer.DropChannel(channel))
	requireFalse(t, "direct is not channel", reducer.DropChannel(lena))
	requireTrue(t, "direct still kept", reducer.Find(lena) != nil)
	requireInt(t, "conversations after rejected drop", len(reducer.Conversations()), 1)

	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	recreated := stateOf(t, reducer, channel)
	requireTrue(t, "recreated is channel", recreated.IsChannel())
	requireTrue(t, "recreated joined", recreated.Channel().Joined)
}

func TestClearMessagesWipesTranscriptKeepsRow(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	lena := reducer.ConversationKey(networkA, "lena")
	channel := reducer.ConversationKey(networkA, "#room")
	missing := reducer.ConversationKey(networkA, "ghost")

	reducer.Apply(MessageEvent{Conversation: lena, Author: "lena", Body: "hi", Timestamp: reducerTimestamp, Target: "lena"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Alice"}, reducerTimestamp)
	reducer.Apply(TopicEvent{NetworkID: networkA, Channel: "#room", Topic: "topic", Author: "op"}, reducerTimestamp)
	reducer.Apply(MessageEvent{Conversation: channel, Author: "Alice", Body: "hello", Timestamp: reducerTimestamp, Target: "#room"}, reducerTimestamp)

	requireInt(t, "conversations", len(reducer.Conversations()), 2)
	requireTrue(t, "direct exists", reducer.Find(lena) != nil)
	requireFalse(t, "direct not channel", reducer.Find(lena).IsChannel())
	requireInt(t, "direct messages", len(reducer.Find(lena).Messages), 1)

	reducer.ClearMessages(lena)
	direct := stateOf(t, reducer, lena)
	requireFalse(t, "direct still not channel", direct.IsChannel())
	requireInt(t, "direct messages after clear", len(direct.Messages), 0)
	requireInt(t, "conversations after clear", len(reducer.Conversations()), 2)

	reducer.ClearMessages(lena)
	requireInt(t, "direct messages after second clear", len(reducer.Find(lena).Messages), 0)
	requireTrue(t, "direct still present", reducer.Find(lena) != nil)

	room := stateOf(t, reducer, channel)
	requireTrue(t, "room is channel", room.IsChannel())
	requireInt(t, "room peopleCount", room.PeopleCount(), 2)
	requireString(t, "room topic", room.Channel().Topic, "topic")
	requireTrue(t, "room has messages", len(room.Messages) != 0)
	reducer.ClearMessages(channel)
	requireInt(t, "room messages after clear", len(room.Messages), 0)
	requireInt(t, "room peopleCount after clear", room.PeopleCount(), 2)
	requireString(t, "room topic after clear", room.Channel().Topic, "topic")
	requireTrue(t, "room still channel", room.IsChannel())

	requireFalse(t, "missing absent", reducer.Find(missing) != nil)
	reducer.ClearMessages(missing)
	requireFalse(t, "missing still absent", reducer.Find(missing) != nil)
	requireInt(t, "conversations after missing clear", len(reducer.Conversations()), 2)

	reducer.Apply(MessageEvent{
		Conversation: lena, Author: "lena", Body: "seen",
		Timestamp: reducerTimestamp, Target: "lena", MsgID: MsgID{Value: "seen-id"},
	}, reducerTimestamp)
	reducer.ClearMessages(lena)
	requireInt(t, "direct messages after clear", len(reducer.Find(lena).Messages), 0)
	if _, ok := reducer.Find(lena).MessageIDs[MsgID{Value: "seen-id"}]; !ok {
		t.Fatal("clearMessages must keep the msgid set")
	}
	reducer.Apply(HistoryEvent{
		Conversation: lena,
		Target:       "lena",
		Lines: []ReplayLine{
			replayLine("lena", "seen", "seen-id"),
			replayLine("lena", "later", "later-id"),
		},
	}, reducerTimestamp)
	requireInt(t, "direct messages after replay", len(reducer.Find(lena).Messages), 1)
	requireString(t, "replay body", reducer.Find(lena).Messages[0].Body, "later")
	requireString(t, "replay msgid", reducer.Find(lena).Messages[0].MsgID.Value, "later-id")

	requireTrue(t, "drop direct", reducer.DropDirectMessage(lena))
	requireFalse(t, "direct gone", reducer.Find(lena) != nil)
	requireTrue(t, "channel kept", reducer.Find(channel) != nil)
	requireFalse(t, "channel not direct", reducer.DropDirectMessage(channel))
	requireTrue(t, "channel still kept", reducer.Find(channel) != nil)
	requireInt(t, "conversations", len(reducer.Conversations()), 1)
}

func TestMessagesCapAtTwoThousandFifo(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")

	for index := 0; index < 2001; index++ {
		reducer.Apply(MessageEvent{
			Conversation: room, Author: "Alice", Body: strconv.Itoa(index),
			Timestamp: reducerTimestamp, Target: "#room",
		}, reducerTimestamp)
	}

	conversation := stateOf(t, reducer, room)
	requireInt(t, "messages", len(conversation.Messages), 2000)
	requireString(t, "front", conversation.Messages[0].Body, "1")
	requireString(t, "back", conversation.Messages[len(conversation.Messages)-1].Body, "2000")
	requireInt(t, "trimmed", conversation.Trimmed, 1)

	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Bob"}, reducerTimestamp)
	requireInt(t, "messages after join", len(conversation.Messages), 2000)
	requireString(t, "front after join", conversation.Messages[0].Body, "2")
	last := conversation.Messages[len(conversation.Messages)-1]
	requireString(t, "back after join", last.Body, "Bob joined")
	requireInt(t, "back kind after join", int(last.Kind), int(KindEvent))
	requireInt(t, "trimmed after join", conversation.Trimmed, 2)
}

func TestClearMessagesEmptiesAfterCap(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")

	for index := 0; index < 2001; index++ {
		reducer.Apply(MessageEvent{
			Conversation: room, Author: "Alice", Body: strconv.Itoa(index),
			Timestamp: reducerTimestamp, Target: "#room",
		}, reducerTimestamp)
	}

	conversation := stateOf(t, reducer, room)
	requireInt(t, "messages", len(conversation.Messages), 2000)
	reducer.ClearMessages(room)
	requireInt(t, "messages after clear", len(conversation.Messages), 0)
	requireInt(t, "trimmed after clear", conversation.Trimmed, 2001)
	requireTrue(t, "conversation kept", reducer.Find(room) != nil)
}

func TestSelfAwayIsNetworkMembershipNotMemberPresence(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	omarchy := reducer.ConversationKey(networkA, "#omarchy")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#desktop", Nick: "omairc"}, reducerTimestamp)

	reducer.Apply(AwayEvent{NetworkID: networkA, Nick: "omairc", Away: &Away{}}, reducerTimestamp)
	requireFalse(t, "selfAway from away-notify", reducer.SelfAway(networkA))
	requireTrue(t, "member row away", memberViewOf(t, reducer, omarchy, "omairc").IsAway())

	reducer.Apply(SelfAwayEvent{NetworkID: networkA, Away: true}, reducerTimestamp)
	reducer.Apply(SelfAwayEvent{NetworkID: networkA, Away: true}, reducerTimestamp)
	requireTrue(t, "selfAway set", reducer.SelfAway(networkA))

	reducer.Apply(PartEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(PartEvent{NetworkID: networkA, Channel: "#desktop", Nick: "omairc"}, reducerTimestamp)
	requireTrue(t, "selfAway survives parts", reducer.SelfAway(networkA))

	reducer.ClearPresenceFacts(networkA, true, true)
	requireTrue(t, "selfAway survives presence clear", reducer.SelfAway(networkA))

	reducer.Apply(SelfAwayEvent{NetworkID: networkA, Away: false}, reducerTimestamp)
	reducer.Apply(SelfAwayEvent{NetworkID: networkA, Away: false}, reducerTimestamp)
	requireFalse(t, "selfAway cleared", reducer.SelfAway(networkA))

	reducer.Apply(SelfAwayEvent{NetworkID: networkA, Away: true}, reducerTimestamp)
	welcome(reducer, networkA)
	requireFalse(t, "selfAway reset on welcome", reducer.SelfAway(networkA))
	requireFalse(t, "other network selfAway", reducer.SelfAway(networkB))
}

func TestSelfAwayShowsOnOurOwnRowInEveryChannel(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	omarchy := reducer.ConversationKey(networkA, "#omarchy")
	desktop := reducer.ConversationKey(networkA, "#desktop")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#desktop", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "Alice"}, reducerTimestamp)

	requireFalse(t, "omarchy self away", memberViewOf(t, reducer, omarchy, "omairc").IsAway())
	requireFalse(t, "desktop self away", memberViewOf(t, reducer, desktop, "omairc").IsAway())

	reducer.Apply(SelfAwayEvent{NetworkID: networkA, Away: true}, reducerTimestamp)
	requireTrue(t, "selfAway", reducer.SelfAway(networkA))
	requireTrue(t, "omarchy self away after", memberViewOf(t, reducer, omarchy, "omairc").IsAway())
	requireTrue(t, "desktop self away after", memberViewOf(t, reducer, desktop, "omairc").IsAway())
	requireFalse(t, "peer away", memberViewOf(t, reducer, omarchy, "alice").IsAway())

	// The overlay is a read of the network-level fact, so losing member
	// presence facts must not clear our own away state.
	reducer.ClearPresenceFacts(networkA, true, true)
	requireTrue(t, "self away after presence clear", memberViewOf(t, reducer, omarchy, "omairc").IsAway())

	reducer.Apply(SelfAwayEvent{NetworkID: networkA, Away: false}, reducerTimestamp)
	requireFalse(t, "omarchy self back", memberViewOf(t, reducer, omarchy, "omairc").IsAway())
	requireFalse(t, "desktop self back", memberViewOf(t, reducer, desktop, "omairc").IsAway())
}

func TestPeerPresenceFollowsSharedChannelAndAwayFacts(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)

	requireInt(t, "no shared channel", int(reducer.PeerPresence(networkA, "alice")), int(PeerUnknown))

	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "Alice"}, reducerTimestamp)
	requireInt(t, "shared channel", int(reducer.PeerPresence(networkA, "alice")), int(PeerOnline))

	reducer.Apply(AwayEvent{NetworkID: networkA, Nick: "Alice", Away: &Away{Reason: "lunch"}}, reducerTimestamp)
	requireInt(t, "away", int(reducer.PeerPresence(networkA, "alice")), int(PeerAway))

	reducer.Apply(AwayEvent{NetworkID: networkA, Nick: "Alice", Away: nil}, reducerTimestamp)
	requireInt(t, "back", int(reducer.PeerPresence(networkA, "alice")), int(PeerOnline))

	reducer.Apply(QuitEvent{NetworkID: networkA, Nick: "Alice"}, reducerTimestamp)
	requireInt(t, "after quit", int(reducer.PeerPresence(networkA, "alice")), int(PeerUnknown))

	requireInt(t, "other network", int(reducer.PeerPresence(networkB, "alice")), int(PeerUnknown))
}

func TestFalseyBotValuesAreNotBots(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(NamesEvent{
		NetworkID: networkA, Channel: "#room",
		Names: []Name{parsedName(t, "Alice")}, Complete: true,
	}, reducerTimestamp)

	for _, falsey := range []string{"0", "false", "no"} {
		reducer.Apply(MemberMetadataEvent{NetworkID: networkA, Nick: "Alice", Key: BotKey(), Value: falsey}, reducerTimestamp)
		member := memberViewOf(t, reducer, room, "alice")
		requireFalse(t, "bot "+falsey, member.Bot)
		requireFalse(t, "facts bot "+falsey, reducer.NickPresence(networkA, "Alice").IsBot())
		reducer.Apply(MemberMetadataEvent{NetworkID: networkA, Nick: "Alice", Key: BotKey(), Value: ""}, reducerTimestamp)
	}

	reducer.Apply(MemberMetadataEvent{NetworkID: networkA, Nick: "Alice", Key: BotKey(), Value: "PacketBot"}, reducerTimestamp)
	requireTrue(t, "real bot", memberViewOf(t, reducer, room, "alice").Bot)
}

func TestStaleNamesSyncReleasesAfterThirtySeconds(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	started := reducerTimestamp
	reducer.Apply(NamesEvent{
		NetworkID: networkA, Channel: "#room",
		Names: []Name{parsedName(t, "Alice")},
	}, started)
	key := reducer.ConversationKey(networkA, "#room")
	channel := stateOf(t, reducer, key).Channel()
	requireTrue(t, "namesSyncing", channel.NamesSyncing)
	requireFalse(t, "namesSyncStarted", channel.NamesSyncStarted.IsZero())

	requireFalse(t, "at 29s", reducer.ReleaseStaleNamesSync(&key, started.Add(29*time.Second)))
	requireTrue(t, "still syncing", channel.NamesSyncing)

	requireTrue(t, "at 31s", reducer.ReleaseStaleNamesSync(&key, started.Add(31*time.Second)))
	requireFalse(t, "released", channel.NamesSyncing)
}

func TestConsecutiveJoinsCollapseIntoOneEvent(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Alice"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Bob"}, reducerTimestamp)

	conversation := stateOf(t, reducer, reducer.ConversationKey(networkA, "#room"))
	requireInt(t, "messages", len(conversation.Messages), 1)
	requireInt(t, "kind", int(conversation.Messages[0].Kind), int(KindEvent))
	requireString(t, "body", conversation.Messages[0].Body, "Alice, Bob joined")
	requireInt(t, "trimmed", conversation.Trimmed, 0)
}

func TestPrivmsgBreaksJoinCollapse(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Alice"}, reducerTimestamp)
	reducer.Apply(MessageEvent{Conversation: room, Author: "Alice", Body: "hello", Timestamp: reducerTimestamp, Target: "#room"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Bob"}, reducerTimestamp)

	conversation := stateOf(t, reducer, room)
	requireInt(t, "messages", len(conversation.Messages), 3)
	requireString(t, "messages[0]", conversation.Messages[0].Body, "Alice joined")
	requireInt(t, "messages[1] kind", int(conversation.Messages[1].Kind), int(KindMessage))
	requireString(t, "messages[1]", conversation.Messages[1].Body, "hello")
	requireString(t, "messages[2]", conversation.Messages[2].Body, "Bob joined")
}

func TestKickAndModeStaySeparateFromJoinLine(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Alice"}, reducerTimestamp)
	reducer.Apply(KickEvent{NetworkID: networkA, Channel: "#room", Target: "Alice", Author: "op", Reason: "bye"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Bob"}, reducerTimestamp)
	reducer.Apply(ModeEvent{NetworkID: networkA, Target: "#room", Author: "op", Mode: "+v", Arguments: []string{"Bob"}}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Carol"}, reducerTimestamp)

	conversation := stateOf(t, reducer, reducer.ConversationKey(networkA, "#room"))
	requireInt(t, "messages", len(conversation.Messages), 5)
	requireString(t, "messages[0]", conversation.Messages[0].Body, "Alice joined")
	requireString(t, "messages[1]", conversation.Messages[1].Body, "Alice was kicked")
	requireString(t, "messages[2]", conversation.Messages[2].Body, "Bob joined")
	requireString(t, "messages[3]", conversation.Messages[3].Body, "op set mode +v")
	requireString(t, "messages[4]", conversation.Messages[4].Body, "Carol joined")
}

func TestMixedJoinPartQuitNickCollapse(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Alice"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Bob"}, reducerTimestamp)
	reducer.Apply(PartEvent{NetworkID: networkA, Channel: "#room", Nick: "Alice"}, reducerTimestamp)
	reducer.Apply(QuitEvent{NetworkID: networkA, Nick: "Bob", Reason: "gone"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Carol"}, reducerTimestamp)
	reducer.Apply(NickEvent{NetworkID: networkA, OldNick: "Carol", NewNick: "Caroline"}, reducerTimestamp)

	conversation := stateOf(t, reducer, reducer.ConversationKey(networkA, "#room"))
	requireInt(t, "messages", len(conversation.Messages), 1)
	requireString(t, "body", conversation.Messages[0].Body,
		"Alice, Bob joined, Alice left, Bob quit, Carol joined, Carol is now Caroline")
}

func TestForgetNetworkLeavesTheOtherNetwork(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	welcome(reducer, networkB)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#alpha", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(JoinEvent{NetworkID: networkB, Channel: "#lab", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(MessageEvent{
		Conversation: reducer.ConversationKey(networkB, "rio"), Author: "rio", Body: "ping",
		Timestamp: reducerTimestamp, Target: "rio",
	}, reducerTimestamp)
	reducer.MarkSelected(reducer.ConversationKey(networkB, "#lab"))

	reducer.ForgetNetwork(networkB)
	requireTrue(t, "networkA channel kept", reducer.Find(reducer.ConversationKey(networkA, "#alpha")) != nil)
	requireFalse(t, "networkB channel gone", reducer.Find(reducer.ConversationKey(networkB, "#lab")) != nil)
	requireFalse(t, "networkB direct gone", reducer.Find(reducer.ConversationKey(networkB, "rio")) != nil)
	_, selected := reducer.Selected()
	requireFalse(t, "selection cleared", selected)
	requireInt(t, "conversations", len(reducer.Conversations()), 1)
}

func TestMutedChatStillPlantsUnreadMark(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#room")
	reducer.SetMuted(room, true)
	reducer.Apply(MessageEvent{
		Conversation: room, Author: "Alice", Body: "omairc: ping",
		Timestamp: reducerTimestamp, Target: "#room",
	}, reducerTimestamp)

	muted := stateOf(t, reducer, room)
	requireTrue(t, "muted", muted.Muted)
	requireInt(t, "mentions", muted.Mentions, 0)
	requireInt(t, "unread", muted.Unread, 1)
	requireTrue(t, "unreadMark", muted.UnreadMark != nil)
	requireInt64(t, "unreadMark sequence", *muted.UnreadMark, muted.Messages[len(muted.Messages)-1].Sequence)
}

func TestNickMergeAdoptsOrKeepsUnreadMark(t *testing.T) {
	t.Run("adopt", func(t *testing.T) {
		reducer := NewEventReducer()
		welcome(reducer, networkA)
		alice := reducer.ConversationKey(networkA, "Alice")
		alicia := reducer.ConversationKey(networkA, "Alicia")
		reducer.MarkSelected(alicia)
		reducer.Apply(MessageEvent{Conversation: alicia, Author: "Alicia", Body: "dest-only", Timestamp: reducerTimestamp, Target: "Alicia"}, reducerTimestamp)
		requireFalse(t, "dest unreadMark", reducer.Find(alicia).UnreadMark != nil)
		reducer.ClearSelection()
		reducer.Apply(MessageEvent{Conversation: alice, Author: "Alice", Body: "moved-chat", Timestamp: reducerTimestamp, Target: "Alice"}, reducerTimestamp)
		requireTrue(t, "moved unreadMark", reducer.Find(alice).UnreadMark != nil)

		reducer.Apply(NickEvent{NetworkID: networkA, OldNick: "Alice", NewNick: "Alicia"}, reducerTimestamp)
		merged := reducer.Find(alicia)
		requireTrue(t, "merged unreadMark", merged.UnreadMark != nil)
		var destOnlySequence, movedChatSequence int64 = -1, -1
		for _, message := range merged.Messages {
			switch message.Body {
			case "dest-only":
				destOnlySequence = message.Sequence
			case "moved-chat":
				movedChatSequence = message.Sequence
			}
		}
		requireTrue(t, "destOnlySequence found", destOnlySequence >= 0)
		requireTrue(t, "movedChatSequence found", movedChatSequence >= 0)
		requireInt64(t, "adopted mark", *merged.UnreadMark, movedChatSequence)
		if *merged.UnreadMark == destOnlySequence {
			t.Fatal("merged mark must not point at the destination-only row")
		}
	})

	t.Run("keep", func(t *testing.T) {
		reducer := NewEventReducer()
		welcome(reducer, networkA)
		fromAlice := reducer.ConversationKey(networkA, "Alice")
		fromAlicia := reducer.ConversationKey(networkA, "Alicia")
		reducer.Apply(MessageEvent{Conversation: fromAlice, Author: "Alice", Body: "from alice", Timestamp: reducerTimestamp, Target: "Alice"}, reducerTimestamp)
		reducer.MarkSelected(fromAlicia)
		reducer.Apply(MessageEvent{Conversation: fromAlicia, Author: "Alicia", Body: "dest-prefix", Timestamp: reducerTimestamp, Target: "Alicia"}, reducerTimestamp)
		reducer.ClearSelection()
		reducer.Apply(MessageEvent{Conversation: fromAlicia, Author: "Alicia", Body: "from alicia", Timestamp: reducerTimestamp, Target: "Alicia"}, reducerTimestamp)
		requireTrue(t, "source unreadMark", reducer.Find(fromAlice).UnreadMark != nil)
		requireTrue(t, "dest unreadMark", reducer.Find(fromAlicia).UnreadMark != nil)
		destMark := *reducer.Find(fromAlicia).UnreadMark
		if destMark == *reducer.Find(fromAlice).UnreadMark {
			t.Fatal("the two conversations must have distinct marks")
		}

		reducer.Apply(NickEvent{NetworkID: networkA, OldNick: "Alice", NewNick: "Alicia"}, reducerTimestamp)
		merged := reducer.Find(fromAlicia)
		requireTrue(t, "merged unreadMark", merged.UnreadMark != nil)
		requireInt64(t, "kept mark", *merged.UnreadMark, destMark)
		var destUnreadSequence, movedUnreadSequence int64 = -1, -1
		for _, message := range merged.Messages {
			switch message.Body {
			case "from alicia":
				destUnreadSequence = message.Sequence
			case "from alice":
				movedUnreadSequence = message.Sequence
			}
		}
		requireInt64(t, "kept mark points at destination row", *merged.UnreadMark, destUnreadSequence)
		requireTrue(t, "destination sequence found", destUnreadSequence >= 0)
		if destUnreadSequence == movedUnreadSequence {
			t.Fatal("the two rows must have distinct sequences")
		}
	})
}

func TestMsgidDedupSkipsLiveThenReplay(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#omarchy")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(MessageEvent{
		Conversation: room, Author: "alice", Body: "first",
		Timestamp: reducerTimestamp, Target: "#omarchy", MsgID: MsgID{Value: "same"},
	}, reducerTimestamp)
	reducer.Apply(HistoryEvent{
		Conversation: room,
		Target:       "#omarchy",
		Lines:        []ReplayLine{replayLine("alice", "second", "same")},
	}, reducerTimestamp)

	conversation := stateOf(t, reducer, room)
	requireInt(t, "messages", len(conversation.Messages), 2)
	requireString(t, "messages[0]", conversation.Messages[0].Body, "omairc joined")
	requireString(t, "messages[1]", conversation.Messages[1].Body, "first")
	requireInt(t, "messages[1] origin", int(conversation.Messages[1].Origin), int(OriginLive))
}

func TestMsgidDedupSkipsReplayThenLive(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	room := reducer.ConversationKey(networkA, "#omarchy")
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "omairc"}, reducerTimestamp)
	reducer.Apply(HistoryEvent{
		Conversation: room,
		Target:       "#omarchy",
		Lines:        []ReplayLine{replayLine("alice", "first", "same")},
	}, reducerTimestamp)
	reducer.Apply(MessageEvent{
		Conversation: room, Author: "alice", Body: "second",
		Timestamp: reducerTimestamp, Target: "#omarchy", MsgID: MsgID{Value: "same"},
	}, reducerTimestamp)

	conversation := stateOf(t, reducer, room)
	requireInt(t, "messages", len(conversation.Messages), 2)
	requireString(t, "messages[0]", conversation.Messages[0].Body, "first")
	requireInt(t, "messages[0] origin", int(conversation.Messages[0].Origin), int(OriginReplay))
	requireString(t, "messages[1]", conversation.Messages[1].Body, "omairc joined")
}

func TestNickMergeDropsDuplicateMsgids(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	alice := reducer.ConversationKey(networkA, "Alice")
	alicia := reducer.ConversationKey(networkA, "Alicia")
	reducer.Apply(MessageEvent{Conversation: alice, Author: "Alice", Body: "shared", Timestamp: reducerTimestamp, Target: "Alice", MsgID: MsgID{Value: "same"}}, reducerTimestamp)
	reducer.Apply(MessageEvent{Conversation: alicia, Author: "Alicia", Body: "shared", Timestamp: reducerTimestamp, Target: "Alicia", MsgID: MsgID{Value: "same"}}, reducerTimestamp)
	reducer.Apply(MessageEvent{Conversation: alicia, Author: "Alicia", Body: "only here", Timestamp: reducerTimestamp, Target: "Alicia"}, reducerTimestamp)

	reducer.Apply(NickEvent{NetworkID: networkA, OldNick: "Alice", NewNick: "Alicia"}, reducerTimestamp)

	merged := stateOf(t, reducer, alicia)
	var bodies []string
	for _, message := range merged.Messages {
		bodies = append(bodies, message.Body)
	}
	requireStrings(t, "bodies", bodies, []string{"shared", "only here", "Alice is now Alicia"})
}

func TestNickCollisionMergesMessageIds(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	alice := reducer.ConversationKey(networkA, "Alice")
	alicia := reducer.ConversationKey(networkA, "Alicia")
	reducer.Apply(MessageEvent{Conversation: alice, Author: "Alice", Body: "from alice", Timestamp: reducerTimestamp, Target: "Alice", MsgID: MsgID{Value: "id-a"}}, reducerTimestamp)
	reducer.Apply(MessageEvent{Conversation: alicia, Author: "Alicia", Body: "from alicia", Timestamp: reducerTimestamp, Target: "Alicia", MsgID: MsgID{Value: "id-b"}}, reducerTimestamp)

	reducer.Apply(NickEvent{NetworkID: networkA, OldNick: "Alice", NewNick: "Alicia"}, reducerTimestamp)

	merged := stateOf(t, reducer, alicia)
	requireTrue(t, "merged has both rows", len(merged.Messages) >= 2)
	afterMerge := len(merged.Messages)

	reducer.Apply(MessageEvent{Conversation: alicia, Author: "Alicia", Body: "repeat a", Timestamp: reducerTimestamp, Target: "Alicia", MsgID: MsgID{Value: "id-a"}}, reducerTimestamp)
	reducer.Apply(MessageEvent{Conversation: alicia, Author: "Alicia", Body: "repeat b", Timestamp: reducerTimestamp, Target: "Alicia", MsgID: MsgID{Value: "id-b"}}, reducerTimestamp)
	requireInt(t, "messages after repeats", len(reducer.Find(alicia).Messages), afterMerge)

	reducer.Apply(MessageEvent{Conversation: alicia, Author: "Alicia", Body: "fresh", Timestamp: reducerTimestamp, Target: "Alicia", MsgID: MsgID{Value: "id-c"}}, reducerTimestamp)
	requireInt(t, "messages after fresh", len(reducer.Find(alicia).Messages), afterMerge+1)
	requireString(t, "fresh body", reducer.Find(alicia).Messages[len(reducer.Find(alicia).Messages)-1].Body, "fresh")
}

func TestConversationCauseInsertTable(t *testing.T) {
	cases := []struct {
		name            string
		cause           ConversationCause
		targetIsChannel bool
		service         bool
		want            bool
	}{
		{"UserOpen direct", CauseUserOpen, false, false, true},
		{"UserOpen service", CauseUserOpen, false, true, true},
		{"UserOpen channel", CauseUserOpen, true, false, false},
		{"ChannelState channel", CauseChannelState, true, false, true},
		{"ChannelState direct", CauseChannelState, false, false, false},
		{"InboundOther channel", CauseInboundOther, true, false, true},
		{"InboundOther direct", CauseInboundOther, false, false, true},
		{"InboundOther service", CauseInboundOther, false, true, false},
		{"InboundSelf direct", CauseInboundSelf, false, false, false},
		{"InboundSelf channel", CauseInboundSelf, true, false, false},
		{"QuietSend direct", CauseQuietSend, false, false, false},
		{"QuietSend channel", CauseQuietSend, true, false, false},
		{"Restore direct", CauseRestore, false, false, true},
		{"Restore service", CauseRestore, false, true, false},
		{"Restore channel", CauseRestore, true, false, false},
	}
	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			got := ConversationCauseInserts(testCase.cause, testCase.targetIsChannel, testCase.service)
			requireInt(t, "inserts", boolToInt(got), boolToInt(testCase.want))
		})
	}
}

func boolToInt(value bool) int {
	if value {
		return 1
	}
	return 0
}

func TestEnsureConversationHonorsCause(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	lena := reducer.ConversationKey(networkA, "lena")
	nickserv := reducer.ConversationKey(networkA, "NickServ")
	room := reducer.ConversationKey(networkA, "#room")
	ghost := reducer.ConversationKey(networkA, "ghost")

	if reducer.EnsureConversation(lena, "lena", CauseQuietSend) != nil {
		t.Fatal("QuietSend must not invent a direct message")
	}
	requireFalse(t, "lena absent", reducer.Find(lena) != nil)
	if reducer.EnsureConversation(lena, "lena", CauseInboundSelf) != nil {
		t.Fatal("InboundSelf must not invent a direct message")
	}
	requireFalse(t, "lena still absent", reducer.Find(lena) != nil)

	reducer.MarkSelected(ghost)
	if reducer.EnsureConversation(ghost, "ghost", CauseInboundSelf) != nil {
		t.Fatal("InboundSelf must not invent a selected conversation")
	}
	requireFalse(t, "ghost absent", reducer.Find(ghost) != nil)

	human := reducer.EnsureConversation(lena, "lena", CauseInboundOther)
	requireTrue(t, "human present", human != nil)
	requireFalse(t, "human not channel", human.IsChannel())

	if reducer.EnsureConversation(nickserv, "NickServ", CauseInboundOther) != nil {
		t.Fatal("InboundOther must not invent a service query")
	}
	requireFalse(t, "nickserv absent", reducer.Find(nickserv) != nil)
	queried := reducer.EnsureConversation(nickserv, "NickServ", CauseUserOpen)
	requireTrue(t, "queried present", queried != nil)
	requireFalse(t, "queried not channel", queried.IsChannel())

	if reducer.EnsureConversation(room, "#room", CauseUserOpen) != nil {
		t.Fatal("UserOpen must not invent a channel")
	}
	requireFalse(t, "room absent", reducer.Find(room) != nil)
	channel := reducer.EnsureConversation(room, "#room", CauseChannelState)
	requireTrue(t, "channel present", channel != nil)
	requireTrue(t, "channel is channel", channel.IsChannel())

	requireTrue(t, "quiet send existing", reducer.EnsureConversation(lena, "lena", CauseQuietSend) != nil)
	requireTrue(t, "inbound self existing", reducer.EnsureConversation(lena, "lena", CauseInboundSelf) != nil)

	requireFalse(t, "ghost still absent", reducer.Find(ghost) != nil)
	restored := reducer.EnsureConversation(ghost, "ghost", CauseRestore)
	requireTrue(t, "restored present", restored != nil)
	requireFalse(t, "restored not channel", restored.IsChannel())

	restoreOnly := NewEventReducer()
	welcome(restoreOnly, networkA)
	if restoreOnly.EnsureConversation(nickserv, "NickServ", CauseRestore) != nil {
		t.Fatal("Restore must not invent a service query")
	}
	requireFalse(t, "restore nickserv absent", restoreOnly.Find(nickserv) != nil)
	if restoreOnly.EnsureConversation(room, "#room", CauseRestore) != nil {
		t.Fatal("Restore must not invent a channel")
	}

	reducer.Apply(MessageEvent{Conversation: lena, Author: "omairc", Body: "hello", Timestamp: reducerTimestamp, Target: "lena"}, reducerTimestamp)
	requireInt(t, "human messages", len(human.Messages), 1)
	requireString(t, "human last body", human.Messages[0].Body, "hello")

	inbound := NewEventReducer()
	welcome(inbound, networkA)
	inbound.Apply(MessageEvent{Conversation: nickserv, Author: "NickServ", Body: "identify", Timestamp: reducerTimestamp, Target: "NickServ"}, reducerTimestamp)
	requireFalse(t, "inbound nickserv absent", inbound.Find(nickserv) != nil)
	inbound.Apply(MessageEvent{Conversation: lena, Author: "lena", Body: "hi", Timestamp: reducerTimestamp, Target: "lena"}, reducerTimestamp)
	opened := stateOf(t, inbound, lena)
	requireFalse(t, "opened not channel", opened.IsChannel())
	requireInt(t, "opened messages", len(opened.Messages), 1)
}

func TestNickShapedJoinDoesNotInventDirect(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "lena", Nick: "omairc"}, reducerTimestamp)
	requireFalse(t, "no direct invented", reducer.Find(reducer.ConversationKey(networkA, "lena")) != nil)
}

func TestExtendedJoinRecordsAccountOnOneLine(t *testing.T) {
	// The wire form is enough. These caps do not have to be enabled.
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	applyWire(t, reducer, ":Alice!a@h JOIN #room services :Alice Example")

	requireString(t, "presence account", reducer.NickPresence(networkA, "Alice").Account, "services")
	requireString(t, "display account", reducer.DisplayAccount(networkA, "Alice"), "services")
	room := roomOf(reducer)
	requireTrue(t, "room present", room != nil)
	requireInt(t, "messages", len(room.Messages), 1)
	requireString(t, "join body", room.Messages[0].Body, "Alice (services) joined")
	requireString(t, "member account", memberViewOf(t, reducer, room.Key, "alice").Account, "services")

	applyWire(t, reducer, ":Alice!a@h PRIVMSG #room :hi")
	applyWire(t, reducer, ":Alice!a@h JOIN :#room")
	requireString(t, "account kept", reducer.NickPresence(networkA, "Alice").Account, "services")
	requireString(t, "classic join body", lastBody(reducer), "Alice joined")

	applyWire(t, reducer, ":Alice!a@h PRIVMSG #room :again")
	applyWire(t, reducer, ":Alice!a@h JOIN #room * :Alice Example")
	requireString(t, "account cleared", reducer.NickPresence(networkA, "Alice").Account, "")
	requireString(t, "display cleared", reducer.DisplayAccount(networkA, "Alice"), "")
	requireString(t, "star join body", lastBody(reducer), "Alice joined")
}

func TestAccountCommandAndTagShareOneField(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	applyWire(t, reducer, ":Alice!a@h ACCOUNT services")
	requireString(t, "account command", reducer.NickPresence(networkA, "Alice").Account, "services")
	requireFalse(t, "no channel invented", reducer.Find(reducer.ConversationKey(networkA, "#room")) != nil)

	applyWire(t, reducer, ":Alice!a@h ACCOUNT *")
	requireString(t, "account logout", reducer.NickPresence(networkA, "Alice").Account, "")

	applyWire(t, reducer, "@account=services :Alice!a@h PRIVMSG #room :one")
	requireString(t, "account tag", reducer.NickPresence(networkA, "Alice").Account, "services")
	applyWire(t, reducer, ":Alice!a@h PRIVMSG #room :two")
	requireString(t, "account tag sticky", reducer.NickPresence(networkA, "Alice").Account, "services")
	applyWire(t, reducer, "@account=* :Alice!a@h PRIVMSG #room :three")
	requireString(t, "account tag logout", reducer.NickPresence(networkA, "Alice").Account, "")

	applyWire(t, reducer, "@account=services :Alice!a@h NOTICE #room :psst")
	viaCommand := NewEventReducer()
	welcome(viaCommand, networkA)
	applyWire(t, viaCommand, ":Alice!a@h ACCOUNT services")
	requireString(t, "notice account tag", reducer.NickPresence(networkA, "Alice").Account,
		viaCommand.NickPresence(networkA, "Alice").Account)

	applyWire(t, reducer, ":server 330 omairc Alice whoisacct :is logged in as")
	requireString(t, "whois account", reducer.NickPresence(networkA, "Alice").Account, "whoisacct")
	applyWire(t, reducer, ":server 311 omairc Alice user host * :Alice Example")
	requireString(t, "whois account kept", reducer.NickPresence(networkA, "Alice").Account, "whoisacct")
}

func TestNickChangeKeepsServicesAccount(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	applyWire(t, reducer, ":Alice!a@h JOIN #room services :Alice")
	applyWire(t, reducer, ":Alice!a@h NICK Alicia")
	requireString(t, "renamed account", reducer.NickPresence(networkA, "Alicia").Account, "services")
	requireString(t, "old nick account", reducer.NickPresence(networkA, "Alice").Account, "")
	requireString(t, "renamed display account", reducer.DisplayAccount(networkA, "Alicia"), "services")

	same := NewEventReducer()
	welcome(same, networkA)
	applyWire(t, same, ":Bob!b@h JOIN #room bob :Bob")
	requireString(t, "bob join body", lastBody(same), "Bob joined")
	requireString(t, "bob account", same.NickPresence(networkA, "Bob").Account, "bob")
	requireString(t, "bob display account", same.DisplayAccount(networkA, "Bob"), "")
	room := roomOf(same)
	requireTrue(t, "bob room present", room != nil)
	requireString(t, "bob member account", memberViewOf(t, same, room.Key, "bob").Account, "")
	applyWire(t, same, ":Bob!b@h NICK Robert")
	requireString(t, "robert account", same.NickPresence(networkA, "Robert").Account, "bob")
	requireString(t, "robert display account", same.DisplayAccount(networkA, "Robert"), "bob")

	folded := NewEventReducer()
	welcome(folded, networkA)
	applyWire(t, folded, ":a[b!u@h JOIN #room a{b :name")
	requireString(t, "folded join body", lastBody(folded), "a[b joined")
	requireString(t, "folded display account", folded.DisplayAccount(networkA, "a[b"), "")

	ascii := NewEventReducer()
	welcome(ascii, networkA)
	features := NewServerFeatures()
	features.ApplyTokens([]string{"CASEMAPPING=ascii"})
	ascii.SetServerFeatures(networkA, features)
	applyWire(t, ascii, ":a[b!u@h JOIN #room a{b :name")
	requireString(t, "ascii join body", lastBody(ascii), "a[b (a{b) joined")
	requireString(t, "ascii display account", ascii.DisplayAccount(networkA, "a[b"), "a{b")
}

func TestMetadataNotifyRefreshesOnlyAffectedSurfaces(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)

	metadataNotify := func(nick, key string) ViewNotify {
		return ClassifyViewNotify(MemberMetadataEvent{
			NetworkID: networkA, Nick: nick, Key: key, Value: "x",
		}, reducer, nil)
	}

	statusOnly := metadataNotify("Alice", StatusKey())
	requireInt(t, "status members", int(statusOnly.Members), int(MemberSurfaceRow))
	requireFalse(t, "status conversations", statusOnly.Conversations)
	requireFalse(t, "status messages", statusOnly.Messages)

	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "Alice"}, reducerTimestamp)
	avatarShared := metadataNotify("Alice", AvatarKey())
	requireTrue(t, "shared avatar messages", avatarShared.Messages)
	requireFalse(t, "shared avatar conversations", avatarShared.Conversations)

	alice := reducer.ConversationKey(networkA, "Alice")
	reducer.Apply(MessageEvent{Conversation: alice, Author: "Alice", Body: "hi", Timestamp: reducerTimestamp, Target: "Alice"}, reducerTimestamp)
	avatarDirect := metadataNotify("Alice", AvatarKey())
	requireTrue(t, "direct avatar messages", avatarDirect.Messages)
	requireTrue(t, "direct avatar conversations", avatarDirect.Conversations)
}

func TestAwayReloadsConversationsOnlyWhenItCanReachADirectRow(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)

	awayNotify := func(nick string, away *Away) ViewNotify {
		return ClassifyViewNotify(AwayEvent{NetworkID: networkA, Nick: nick, Away: away}, reducer, nil)
	}

	orphan := awayNotify("Alice", &Away{Reason: "lunch"})
	requireFalse(t, "orphan conversations", orphan.Conversations)
	requireInt(t, "orphan members", int(orphan.Members), int(MemberSurfaceRow))
	requireString(t, "orphan nick", orphan.Nick, "alice")

	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "Alice"}, reducerTimestamp)
	shared := awayNotify("Alice", &Away{Reason: "lunch"})
	requireTrue(t, "shared conversations", shared.Conversations)

	reducer.Apply(QuitEvent{NetworkID: networkA, Nick: "Alice"}, reducerTimestamp)
	alice := reducer.ConversationKey(networkA, "Alice")
	reducer.Apply(MessageEvent{Conversation: alice, Author: "Alice", Body: "hi", Timestamp: reducerTimestamp, Target: "Alice"}, reducerTimestamp)
	requireTrue(t, "direct row present", reducer.Find(alice) != nil)
	direct := awayNotify("Alice", &Away{Reason: "lunch"})
	requireTrue(t, "direct conversations", direct.Conversations)

	reducer.Apply(AwayEvent{NetworkID: networkA, Nick: "Alice", Away: &Away{Reason: "lunch"}}, reducerTimestamp)
	cleared := awayNotify("Alice", nil)
	requireTrue(t, "cleared conversations", cleared.Conversations)
	requireInt(t, "cleared members", int(cleared.Members), int(MemberSurfaceRow))
}

func TestAccountChangeRefreshesMemberRowAndTranscript(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#room", Nick: "Alice"}, reducerTimestamp)

	notify := ClassifyViewNotify(AccountEvent{
		NetworkID: networkA, Nick: "Alice", Account: "*",
	}, reducer, nil)
	requireInt(t, "members", int(notify.Members), int(MemberSurfaceRow))
	requireString(t, "nick", notify.Nick, "alice")
	requireTrue(t, "messages", notify.Messages)
	requireFalse(t, "conversations", notify.Conversations)
}
