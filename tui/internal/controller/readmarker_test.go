package controller

import (
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

func TestReadMarkerServerMarkerClearsUnreadOnJoin(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc")
	other := c.reducer.ConversationKey("net", "#other")
	c.reducer.EnsureConversation(other, "#other", irc.CauseChannelState)
	c.SelectConversation("net", "#other")
	when := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	key := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(key, "#room", irc.CauseChannelState)
	applyMessage(c, key, "peer", "old", when.Add(-time.Minute))
	applyMessage(c, key, "peer", "new", when.Add(time.Minute))
	if c.reducer.Find(key).Unread != 2 {
		t.Fatal("expected unread before marker")
	}
	marker := when
	c.ReadMarkerReceived("net", "#room", &marker)
	if c.reducer.Find(key).Unread != 1 {
		t.Fatalf("unread = %d, want 1 after marker", c.reducer.Find(key).Unread)
	}
}

func TestReadMarkerStarLeavesUnread(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc")
	other := c.reducer.ConversationKey("net", "#other")
	c.reducer.EnsureConversation(other, "#other", irc.CauseChannelState)
	c.SelectConversation("net", "#other")
	key := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(key, "#room", irc.CauseChannelState)
	when := time.Now().UTC()
	applyMessage(c, key, "peer", "hi", when)
	c.ReadMarkerReceived("net", "#room", nil)
	if c.reducer.Find(key).Unread != 1 {
		t.Fatal("* must not clear unread")
	}
}

func TestReadMarkerOlderMarkerDoesNotMoveBadgeBack(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc")
	key := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(key, "#room", irc.CauseChannelState)
	newer := time.Date(2024, 6, 2, 12, 0, 0, 0, time.UTC)
	older := newer.Add(-time.Hour)
	applyMessage(c, key, "peer", "line", newer)
	c.ReadMarkerReceived("net", "#room", &newer)
	c.ReadMarkerReceived("net", "#room", &older)
	conv := c.reducer.Find(key)
	if conv.ReadMarker == nil || !conv.ReadMarker.Equal(newer) {
		t.Fatal("older marker must not replace newer stored marker")
	}
}

func TestCaughtUpSelectionSendsReadMarkerOnce(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, true)
	key := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(key, "#room", irc.CauseChannelState)
	c.SelectConversation("net", "#room")
	when := time.Date(2024, 6, 1, 12, 0, 0, 123000000, time.UTC)
	c.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       "#room",
		Author:       "peer",
		Body:         "hi",
		Timestamp:    when,
	})
	want := "MARKREAD #room timestamp=2024-06-01T12:00:00.123Z\r\n"
	if !writtenFramesContain(transport, want) {
		t.Fatalf("frames = %v, want %q", transport.WrittenFrames(), want)
	}
	before := len(transport.WrittenFrames())
	c.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       "#room",
		Author:       "peer",
		Body:         "again",
		Timestamp:    when,
	})
	if len(transport.WrittenFrames()) != before {
		t.Fatal("equal timestamp must not send another MARKREAD")
	}
}

func TestReadMarkerScrolledUpDoesNotSend(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, false)
	key := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(key, "#room", irc.CauseChannelState)
	c.SelectConversation("net", "#room")
	when := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	c.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       "#room",
		Author:       "peer",
		Body:         "hi",
		Timestamp:    when,
	})
	if writtenFramesContain(transport, "MARKREAD") {
		t.Fatal("scrolled up must not publish a read marker")
	}
}

func TestReadMarkerLaterTimestampSendsOnceMore(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, true)
	key := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(key, "#room", irc.CauseChannelState)
	c.SelectConversation("net", "#room")
	first := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	second := first.Add(time.Minute)
	c.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       "#room",
		Author:       "peer",
		Body:         "one",
		Timestamp:    first,
	})
	inject(t, transport, ":server FAIL MARKREAD RATE_LIMITED #room :slow\r\n")
	c.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       "#room",
		Author:       "peer",
		Body:         "two",
		Timestamp:    second,
	})
	if countFramesContaining(transport, "MARKREAD") != 2 {
		t.Fatalf("expected two MARKREAD frames, got %v", transport.WrittenFrames())
	}
}

func TestOpenExistingDirectRequestsReadMarkerGet(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	roomKey := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(roomKey, "#room", irc.CauseChannelState)
	c.SelectConversation("net", "#room")
	key := c.reducer.ConversationKey("net", "alice")
	c.reducer.EnsureConversation(key, "alice", irc.CauseInboundOther)
	if !c.OpenDirectMessage("alice") {
		t.Fatal("OpenDirectMessage failed")
	}
	if !writtenFramesContain(transport, "MARKREAD alice\r\n") {
		t.Fatalf("opening existing DM must send MARKREAD get, frames=%v", transport.WrittenFrames())
	}
}

func applyMessage(c *Controller, key irc.ConversationKey, author, body string, when time.Time) {
	c.reducer.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       key.NormalizedTarget,
		Author:       author,
		Body:         body,
		Timestamp:    when,
	}, when)
}

func countFramesContaining(transport *session.LoopbackTransport, needle string) int {
	count := 0
	for _, frame := range transport.WrittenFrames() {
		if strings.Contains(string(frame), needle) {
			count++
		}
	}
	return count
}
