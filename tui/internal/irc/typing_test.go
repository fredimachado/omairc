package irc

import (
	"testing"
	"time"
)

// typingT0 is the fixed clock the ported typing tests use.
var typingT0 = time.Date(2026, time.September, 4, 12, 0, 0, 0, time.UTC)

func TestTypingStoredClocks(t *testing.T) {
	active, ok := StoredTypingHint(TypingActive, typingT0, "alice")
	requireTrue(t, "active stored", ok)
	requireInt(t, "active clock", int(active.Clock), int(TypingClockActive))
	requireString(t, "active nick", active.DisplayNick, "alice")
	requireTrue(t, "active receivedAt", active.ReceivedAt.Equal(typingT0))

	paused, ok := StoredTypingHint(TypingPaused, typingT0, "bob")
	requireTrue(t, "paused stored", ok)
	requireInt(t, "paused clock", int(paused.Clock), int(TypingClockPaused))

	if _, ok := StoredTypingHint(TypingDone, typingT0, "carol"); ok {
		t.Fatal("done must never store a hint")
	}
}

func TestTypingShowsIndicatorUntilClockHold(t *testing.T) {
	active, ok := StoredTypingHint(TypingActive, typingT0, "alice")
	requireTrue(t, "active stored", ok)
	requireTrue(t, "active at t0", TypingShowsIndicator(active, typingT0))
	requireTrue(t, "active at 5999", TypingShowsIndicator(active, typingT0.Add(5999*time.Millisecond)))
	requireFalse(t, "active at 6000", TypingShowsIndicator(active, typingT0.Add(6000*time.Millisecond)))
	requireTrue(t, "active retained at 5999", TypingHintRetained(active, typingT0.Add(5999*time.Millisecond)))
	requireFalse(t, "active retained at 6000", TypingHintRetained(active, typingT0.Add(6000*time.Millisecond)))

	paused, ok := StoredTypingHint(TypingPaused, typingT0, "bob")
	requireTrue(t, "paused stored", ok)
	requireTrue(t, "paused at t0", TypingShowsIndicator(paused, typingT0))
	requireTrue(t, "paused at 5999", TypingShowsIndicator(paused, typingT0.Add(5999*time.Millisecond)))
	requireTrue(t, "paused at 6000", TypingShowsIndicator(paused, typingT0.Add(6000*time.Millisecond)))
	requireTrue(t, "paused at 29999", TypingShowsIndicator(paused, typingT0.Add(29999*time.Millisecond)))
	requireFalse(t, "paused at 30000", TypingShowsIndicator(paused, typingT0.Add(30000*time.Millisecond)))
	requireTrue(t, "paused retained at 29999", TypingHintRetained(paused, typingT0.Add(29999*time.Millisecond)))
	requireFalse(t, "paused retained at 30000", TypingHintRetained(paused, typingT0.Add(30000*time.Millisecond)))

	// Active and paused differ only in hold; both are still typing hints.
	requireTrue(t, "active still typing", TypingShowsIndicator(active, typingT0.Add(1*time.Second)))
	requireTrue(t, "paused still typing", TypingShowsIndicator(paused, typingT0.Add(1*time.Second)))

	invalidStamp := TypingHint{Clock: TypingClockActive}
	requireFalse(t, "invalid stamp shows", TypingShowsIndicator(invalidStamp, typingT0))
	requireFalse(t, "invalid stamp retained", TypingHintRetained(invalidStamp, typingT0))
	requireFalse(t, "zero now shows", TypingShowsIndicator(active, time.Time{}))
	requireFalse(t, "zero now retained", TypingHintRetained(active, time.Time{}))
}

func TestTypingHintRetainedHonorsClockHold(t *testing.T) {
	active, _ := StoredTypingHint(TypingActive, typingT0, "alice")
	requireTrue(t, "active just before hold", TypingHintRetained(active, typingT0.Add(5999*time.Millisecond)))
	requireFalse(t, "active at hold", TypingHintRetained(active, typingT0.Add(6000*time.Millisecond)))

	paused, _ := StoredTypingHint(TypingPaused, typingT0, "bob")
	requireTrue(t, "paused just before hold", TypingHintRetained(paused, typingT0.Add(29999*time.Millisecond)))
	requireFalse(t, "paused at hold", TypingHintRetained(paused, typingT0.Add(30000*time.Millisecond)))
}

func TestTypingExpiresAt(t *testing.T) {
	active, _ := StoredTypingHint(TypingActive, typingT0, "alice")
	if got, want := TypingExpiresAt(active), typingT0.Add(6*time.Second); !got.Equal(want) {
		t.Fatalf("active expires at %v, want %v", got, want)
	}

	paused, _ := StoredTypingHint(TypingPaused, typingT0, "bob")
	if got, want := TypingExpiresAt(paused), typingT0.Add(30*time.Second); !got.Equal(want) {
		t.Fatalf("paused expires at %v, want %v", got, want)
	}
}

func TestTypingPhaseFromTag(t *testing.T) {
	cases := []struct {
		value string
		want  TypingPhase
		ok    bool
	}{
		{"active", TypingActive, true},
		{"ACTIVE", TypingActive, true},
		{"paused", TypingPaused, true},
		{"PAUSED", TypingPaused, true},
		{"done", TypingDone, true},
		{"DONE", TypingDone, true},
		{"draft", TypingActive, false},
		{"", TypingActive, false},
	}
	for _, testCase := range cases {
		phase, ok := TypingPhaseFromTag(testCase.value)
		requireInt(t, "phase "+testCase.value, int(phase), int(testCase.want))
		requireInt(t, "ok "+testCase.value, boolToInt(ok), boolToInt(testCase.ok))
	}
}

func TestTypingTagmsg(t *testing.T) {
	requireString(t, "active frame", string(TypingTagmsg("#omarchy", TypingActive)),
		"@+typing=active TAGMSG #omarchy\r\n")
	requireString(t, "paused frame", string(TypingTagmsg("#omarchy", TypingPaused)),
		"@+typing=paused TAGMSG #omarchy\r\n")
	requireString(t, "done frame", string(TypingTagmsg("#omarchy", TypingDone)),
		"@+typing=done TAGMSG #omarchy\r\n")

	requireTrue(t, "space rejected", TypingTagmsg("bad target", TypingActive) == nil)
	requireTrue(t, "empty rejected", TypingTagmsg("", TypingDone) == nil)
	requireTrue(t, "carriage return rejected", TypingTagmsg("#a\rb", TypingActive) == nil)
	requireTrue(t, "line feed rejected", TypingTagmsg("#a\nb", TypingActive) == nil)
}

func TestReducerStoresTypingClocksAndExpiresAtRead(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "omairc"}, typingT0)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "alice"}, typingT0)
	key := reducer.ConversationKey(networkA, "#omarchy")

	reducer.Apply(TypingEvent{Conversation: key, Nick: "alice", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)
	requireStrings(t, "active at 5999", reducer.TypingNicks(key, typingT0.Add(5999*time.Millisecond)), []string{"alice"})
	requireStrings(t, "active at 6000", reducer.TypingNicks(key, typingT0.Add(6000*time.Millisecond)), nil)
	requireInt(t, "hint still stored", len(reducer.Find(key).Typing), 1)

	reducer.Apply(TypingEvent{Conversation: key, Nick: "alice", Phase: TypingActive, ReceivedAt: typingT0.Add(4000 * time.Millisecond)}, typingT0.Add(4000*time.Millisecond))
	requireStrings(t, "refreshed at 9000", reducer.TypingNicks(key, typingT0.Add(9000*time.Millisecond)), []string{"alice"})

	reducer.Apply(TypingEvent{Conversation: key, Nick: "bob", Phase: TypingPaused, ReceivedAt: typingT0}, typingT0)
	requireStrings(t, "paused at 5000", reducer.TypingNicks(key, typingT0.Add(5000*time.Millisecond)), []string{"alice", "bob"})
	requireStrings(t, "paused at 10000", reducer.TypingNicks(key, typingT0.Add(10000*time.Millisecond)), []string{"bob"})
	requireStrings(t, "paused at 29999", reducer.TypingNicks(key, typingT0.Add(29999*time.Millisecond)), []string{"bob"})
	requireStrings(t, "paused at 30000", reducer.TypingNicks(key, typingT0.Add(30000*time.Millisecond)), nil)

	reducer.Apply(TypingEvent{Conversation: key, Nick: "alice", Phase: TypingDone, ReceivedAt: typingT0.Add(5000 * time.Millisecond)}, typingT0.Add(5000*time.Millisecond))
	reducer.Apply(TypingEvent{Conversation: key, Nick: "bob", Phase: TypingDone, ReceivedAt: typingT0.Add(5000 * time.Millisecond)}, typingT0.Add(5000*time.Millisecond))
	requireStrings(t, "done clears", reducer.TypingNicks(key, typingT0.Add(5000*time.Millisecond)), nil)
}

func TestReducerPausedTypingHintHoldsUntilPausedDeadline(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "omairc"}, typingT0)
	key := reducer.ConversationKey(networkA, "#omarchy")
	aliceDm := reducer.ConversationKey(networkA, "Alice")

	reducer.Apply(TypingEvent{Conversation: key, Nick: "bob", Phase: TypingPaused, ReceivedAt: typingT0}, typingT0)
	requireStrings(t, "paused at t0", reducer.TypingNicks(key, typingT0), []string{"bob"})
	requireStrings(t, "paused at 1000", reducer.TypingNicks(key, typingT0.Add(1000*time.Millisecond)), []string{"bob"})
	requireStrings(t, "paused at 29000", reducer.TypingNicks(key, typingT0.Add(29000*time.Millisecond)), []string{"bob"})
	requireStrings(t, "paused at 30000", reducer.TypingNicks(key, typingT0.Add(30000*time.Millisecond)), nil)
	if _, ok := reducer.Find(key).Typing["bob"]; !ok {
		t.Fatal("an expired hint must stay stored until pruned by a new hint")
	}

	reducer.Apply(TypingEvent{Conversation: key, Nick: "alice", Phase: TypingActive, ReceivedAt: typingT0.Add(29000 * time.Millisecond)}, typingT0.Add(29000*time.Millisecond))
	if _, ok := reducer.Find(key).Typing["bob"]; !ok {
		t.Fatal("bob must survive a prune at 29000")
	}
	reducer.Apply(TypingEvent{Conversation: key, Nick: "alice", Phase: TypingActive, ReceivedAt: typingT0.Add(30001 * time.Millisecond)}, typingT0.Add(30001*time.Millisecond))
	if _, ok := reducer.Find(key).Typing["bob"]; ok {
		t.Fatal("bob must be pruned at 30001")
	}

	reducer.Apply(MessageEvent{Conversation: aliceDm, Author: "Alice", Body: "hi", Timestamp: typingT0, Target: "Alice"}, typingT0)
	reducer.Apply(TypingEvent{Conversation: aliceDm, Nick: "Alice", Phase: TypingPaused, ReceivedAt: typingT0}, typingT0)
	requireTrue(t, "direct paused at t0", reducer.DirectPeerIsTyping(aliceDm, typingT0))
	requireTrue(t, "direct paused at 29999", reducer.DirectPeerIsTyping(aliceDm, typingT0.Add(29999*time.Millisecond)))
	requireFalse(t, "direct paused at 30000", reducer.DirectPeerIsTyping(aliceDm, typingT0.Add(30000*time.Millisecond)))

	reducer.Apply(TypingEvent{Conversation: aliceDm, Nick: "Alice", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)
	requireTrue(t, "direct active at t0", reducer.DirectPeerIsTyping(aliceDm, typingT0))
	requireTrue(t, "direct active at 5999", reducer.DirectPeerIsTyping(aliceDm, typingT0.Add(5999*time.Millisecond)))
	requireFalse(t, "direct active at 6000", reducer.DirectPeerIsTyping(aliceDm, typingT0.Add(6000*time.Millisecond)))
}

func TestReducerClearsTypingOnChatLeaveAndQuit(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "omairc"}, typingT0)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#desktop", Nick: "omairc"}, typingT0)
	omarchy := reducer.ConversationKey(networkA, "#omarchy")
	desktop := reducer.ConversationKey(networkA, "#desktop")
	reducer.Apply(TypingEvent{Conversation: omarchy, Nick: "alice", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)
	reducer.Apply(TypingEvent{Conversation: desktop, Nick: "alice", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)

	reducer.Apply(MessageEvent{Conversation: omarchy, Author: "alice", Body: "here", Timestamp: typingT0, Target: "#omarchy"}, typingT0)
	requireStrings(t, "chat clears typing", reducer.TypingNicks(omarchy, typingT0), nil)
	requireStrings(t, "other channel keeps typing", reducer.TypingNicks(desktop, typingT0), []string{"alice"})

	reducer.Apply(TypingEvent{Conversation: omarchy, Nick: "alice", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)
	reducer.Apply(PartEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "alice"}, typingT0)
	requireStrings(t, "part clears typing", reducer.TypingNicks(omarchy, typingT0), nil)

	reducer.Apply(TypingEvent{Conversation: desktop, Nick: "bob", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)
	reducer.Apply(QuitEvent{NetworkID: networkA, Nick: "alice", Reason: "gone"}, typingT0)
	requireStrings(t, "quit clears only alice", reducer.TypingNicks(desktop, typingT0), []string{"bob"})

	reducer.Apply(WelcomeEvent{NetworkID: networkA, CurrentNick: "omairc"}, typingT0)
	requireStrings(t, "welcome clears typing", reducer.TypingNicks(desktop, typingT0), nil)
}

func TestReducerRemapsTypingNickIncludingDirectMessage(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "omairc"}, typingT0)
	channel := reducer.ConversationKey(networkA, "#omarchy")
	aliceDm := reducer.ConversationKey(networkA, "Alice")
	reducer.Apply(MessageEvent{Conversation: aliceDm, Author: "Alice", Body: "hi", Timestamp: typingT0, Target: "Alice"}, typingT0)
	reducer.Apply(TypingEvent{Conversation: channel, Nick: "Alice", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)
	reducer.Apply(TypingEvent{Conversation: aliceDm, Nick: "Alice", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)

	reducer.Apply(NickEvent{NetworkID: networkA, OldNick: "Alice", NewNick: "Alicia"}, typingT0)

	requireStrings(t, "channel remapped", reducer.TypingNicks(channel, typingT0), []string{"Alicia"})
	requireFalse(t, "old direct dropped", reducer.Find(aliceDm) != nil)
	aliciaDm := reducer.ConversationKey(networkA, "Alicia")
	requireStrings(t, "direct remapped", reducer.TypingNicks(aliciaDm, typingT0), []string{"Alicia"})
}

func TestReducerMergesTypingWhenDirectMessagesCollide(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	aliceDm := reducer.ConversationKey(networkA, "Alice")
	aliciaDm := reducer.ConversationKey(networkA, "Alicia")
	reducer.Apply(MessageEvent{Conversation: aliceDm, Author: "Alice", Body: "hi", Timestamp: typingT0, Target: "Alice"}, typingT0)
	reducer.Apply(MessageEvent{Conversation: aliciaDm, Author: "Alicia", Body: "yo", Timestamp: typingT0, Target: "Alicia"}, typingT0)
	reducer.Apply(TypingEvent{Conversation: aliceDm, Nick: "Alice", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)

	reducer.Apply(NickEvent{NetworkID: networkA, OldNick: "Alice", NewNick: "Alicia"}, typingT0)

	requireFalse(t, "old direct dropped", reducer.Find(aliceDm) != nil)
	requireStrings(t, "merged typing", reducer.TypingNicks(aliciaDm, typingT0), []string{"Alicia"})
}

func TestReducerHidesSelfTypingAndSkipsMissingConversation(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	missing := reducer.ConversationKey(networkA, "#omarchy")
	reducer.Apply(TypingEvent{Conversation: missing, Nick: "alice", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)
	requireFalse(t, "missing conversation not invented", reducer.Find(missing) != nil)
	requireStrings(t, "missing typing", reducer.TypingNicks(missing, typingT0), nil)

	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "omairc"}, typingT0)
	reducer.Apply(TypingEvent{Conversation: missing, Nick: "omairc", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)
	requireStrings(t, "self typing hidden", reducer.TypingNicks(missing, typingT0), nil)
}

func TestReducerDirectPeerIsTypingForExistingDirectOnly(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	reducer.Apply(JoinEvent{NetworkID: networkA, Channel: "#omarchy", Nick: "omairc"}, typingT0)
	channel := reducer.ConversationKey(networkA, "#omarchy")
	aliceDm := reducer.ConversationKey(networkA, "Alice")
	ghost := reducer.ConversationKey(networkA, "ghost")
	reducer.Apply(MessageEvent{Conversation: aliceDm, Author: "Alice", Body: "hi", Timestamp: typingT0, Target: "Alice"}, typingT0)

	requireFalse(t, "channel is not direct", reducer.DirectPeerIsTyping(channel, typingT0))
	requireFalse(t, "idle direct", reducer.DirectPeerIsTyping(aliceDm, typingT0))
	requireFalse(t, "missing direct", reducer.DirectPeerIsTyping(ghost, typingT0))
	requireFalse(t, "ghost not invented", reducer.Find(ghost) != nil)

	reducer.Apply(TypingEvent{Conversation: aliceDm, Nick: "alice", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)
	reducer.Apply(TypingEvent{Conversation: channel, Nick: "alice", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)
	reducer.Apply(TypingEvent{Conversation: ghost, Nick: "ghost", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)
	requireTrue(t, "direct peer typing", reducer.DirectPeerIsTyping(aliceDm, typingT0))
	requireFalse(t, "channel peer is not a direct peer", reducer.DirectPeerIsTyping(channel, typingT0))
	requireFalse(t, "ghost peer typing", reducer.DirectPeerIsTyping(ghost, typingT0))
	requireFalse(t, "ghost still absent", reducer.Find(ghost) != nil)

	reducer.Apply(TypingEvent{Conversation: aliceDm, Nick: "Alice", Phase: TypingPaused, ReceivedAt: typingT0}, typingT0)
	requireTrue(t, "paused peer typing", reducer.DirectPeerIsTyping(aliceDm, typingT0))

	reducer.Apply(TypingEvent{Conversation: aliceDm, Nick: "Alice", Phase: TypingDone, ReceivedAt: typingT0}, typingT0)
	requireFalse(t, "done clears peer typing", reducer.DirectPeerIsTyping(aliceDm, typingT0))

	reducer.Apply(TypingEvent{Conversation: aliceDm, Nick: "Alice", Phase: TypingActive, ReceivedAt: typingT0}, typingT0)
	requireTrue(t, "active peer typing", reducer.DirectPeerIsTyping(aliceDm, typingT0))
	reducer.Apply(MessageEvent{Conversation: aliceDm, Author: "Alice", Body: "here", Timestamp: typingT0, Target: "Alice"}, typingT0)
	requireFalse(t, "chat clears peer typing", reducer.DirectPeerIsTyping(aliceDm, typingT0))
}
