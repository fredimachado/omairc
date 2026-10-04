package irc

import (
	"testing"
	"time"
)

func TestParseReadMarkerTimeAcceptsVariableFraction(t *testing.T) {
	cases := []struct {
		input string
		want  time.Time
	}{
		{"2024-06-01T12:00:00Z", time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)},
		{"2024-06-01T12:00:00.1Z", time.Date(2024, 6, 1, 12, 0, 0, 100000000, time.UTC)},
		{"2024-06-01T12:00:00.12Z", time.Date(2024, 6, 1, 12, 0, 0, 120000000, time.UTC)},
		{"2024-06-01T12:00:00.123Z", time.Date(2024, 6, 1, 12, 0, 0, 123000000, time.UTC)},
		{"2024-06-01T12:00:00.123456Z", time.Date(2024, 6, 1, 12, 0, 0, 123000000, time.UTC)},
	}
	for _, tc := range cases {
		got, ok := ParseReadMarkerTime(tc.input)
		if !ok {
			t.Fatalf("ParseReadMarkerTime(%q) failed", tc.input)
		}
		if !got.Equal(tc.want) {
			t.Fatalf("ParseReadMarkerTime(%q) = %v, want %v", tc.input, got, tc.want)
		}
	}
}

func TestReadMarkerTimeAfterUsesMilliseconds(t *testing.T) {
	full := time.Date(2024, 6, 1, 12, 0, 0, 123456789, time.UTC)
	trunc := time.Date(2024, 6, 1, 12, 0, 0, 123000000, time.UTC)
	if ReadMarkerTimeAfter(full, trunc) {
		t.Fatal("sub-millisecond tag must not be newer than millisecond marker")
	}
	if !ReadMarkerTimeAfter(trunc.Add(time.Millisecond), trunc) {
		t.Fatal("next millisecond must be strictly newer")
	}
}

func TestLatestServerMessageTimeIgnoresWallClockFallback(t *testing.T) {
	wall := time.Date(2099, 1, 1, 0, 0, 0, 0, time.UTC)
	server := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	conv := &ConversationState{
		Messages: []ReducedMessage{{
			Author:    "peer",
			Body:      "hi",
			Timestamp: wall,
			Kind:      KindMessage,
		}},
	}
	if _, ok := LatestServerMessageTime(conv); ok {
		t.Fatal("line without server-time tag must not publish a marker time")
	}
	conv.Messages[0].ServerTime = &server
	when, ok := LatestServerMessageTime(conv)
	if !ok || !when.Equal(server) {
		t.Fatalf("LatestServerMessageTime = %v, ok=%v", when, ok)
	}
}

func TestCoveredReadMarkerSkipsMentionArrival(t *testing.T) {
	reducer := NewEventReducer()
	welcome(reducer, networkA)
	key := reducer.ConversationKey(networkA, "#room")
	conv := reducer.EnsureConversation(key, "#room", CauseChannelState)
	marker := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	conv.ReadMarker = &marker
	server := marker
	reducer.noteChatArrival(conv, key, "peer", "omairc: ping", KindMessage, MsgID{}, 1, &server, OriginLive, nil)
	if _, ok := reducer.TakeMentionArrival(); ok {
		t.Fatal("covered mention must not plant MentionArrival")
	}
}
