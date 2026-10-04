package controller

import (
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

func TestOldestHistoryCursorPrefersFirstMsgidRegardlessOfKind(t *testing.T) {
	when := time.Date(2011, 10, 19, 16, 40, 50, 0, time.UTC)
	messages := []irc.ReducedMessage{
		{Kind: irc.KindNotice, MsgID: irc.MsgID{Value: "notice-id"}, Timestamp: when, Body: "note"},
		{Kind: irc.KindMessage, MsgID: irc.MsgID{Value: "chat-id"}, Body: "chat"},
	}
	msgid, _, ok := oldestHistoryCursor(messages)
	if !ok || msgid != "notice-id" {
		t.Fatalf("cursor = %q ok=%v, want notice-id", msgid, ok)
	}
}

func TestOldestHistoryCursorUsesFirstTimestampWithoutMsgid(t *testing.T) {
	when := time.Date(2011, 10, 19, 16, 40, 50, 0, time.UTC)
	messages := []irc.ReducedMessage{
		{Kind: irc.KindEvent, Timestamp: when, Body: "joined"},
		{Kind: irc.KindMessage, Timestamp: when.Add(time.Minute), Body: "chat"},
	}
	_, timestamp, ok := oldestHistoryCursor(messages)
	if !ok || !timestamp.Equal(when) {
		t.Fatalf("timestamp = %v ok=%v, want %v", timestamp, ok, when)
	}
}
