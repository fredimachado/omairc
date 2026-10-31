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
		ServerTime:   &when,
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
		ServerTime:   &when,
	})
	if len(transport.WrittenFrames()) != before {
		t.Fatal("equal timestamp must not send another MARKREAD")
	}
}

func TestStatusOpenDoesNotPublishReadMarker(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, true)
	key := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(key, "#room", irc.CauseChannelState)
	c.SelectConversation("net", "#room")
	first := time.Date(2024, 6, 1, 12, 0, 0, 123000000, time.UTC)
	c.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       "#room",
		Author:       "peer",
		Body:         "one",
		Timestamp:    first,
		ServerTime:   &first,
	})
	if !writtenFramesContain(transport, "MARKREAD #room timestamp=") {
		t.Fatalf("frames = %v, want MARKREAD when caught up", transport.WrittenFrames())
	}
	before := countFramesContaining(transport, "MARKREAD")
	c.OpenStatus("net")
	second := first.Add(time.Minute)
	c.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       "#room",
		Author:       "peer",
		Body:         "two",
		Timestamp:    second,
		ServerTime:   &second,
	})
	if countFramesContaining(transport, "MARKREAD") != before {
		t.Fatalf("Status must not publish read markers, frames=%v", transport.WrittenFrames())
	}
	c.NoteTranscriptViewport(true, true)
	if countFramesContaining(transport, "MARKREAD") != before {
		t.Fatalf("viewport sync on Status must not publish, frames=%v", transport.WrittenFrames())
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
		ServerTime:   &when,
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
		ServerTime:   &first,
	})
	inject(t, transport, ":server FAIL MARKREAD RATE_LIMITED #room :slow\r\n")
	c.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       "#room",
		Author:       "peer",
		Body:         "two",
		Timestamp:    second,
		ServerTime:   &second,
	})
	if countFramesContaining(transport, "MARKREAD") != 2 {
		t.Fatalf("expected two MARKREAD frames, got %v", transport.WrittenFrames())
	}
}

func TestSelectExistingDirectRequestsReadMarkerGet(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	roomKey := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(roomKey, "#room", irc.CauseChannelState)
	c.SelectConversation("net", "#room")
	key := c.reducer.ConversationKey("net", "alice")
	c.reducer.EnsureConversation(key, "alice", irc.CauseInboundOther)
	c.SelectConversation("net", "alice")
	if !writtenFramesContain(transport, "MARKREAD alice\r\n") {
		t.Fatalf("selecting existing DM must send MARKREAD get, frames=%v", transport.WrittenFrames())
	}
}

func TestReselectDirectSendsReadMarkerGetOnce(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	roomKey := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(roomKey, "#room", irc.CauseChannelState)
	c.SelectConversation("net", "#room")
	key := c.reducer.ConversationKey("net", "alice")
	c.reducer.EnsureConversation(key, "alice", irc.CauseInboundOther)
	c.SelectConversation("net", "alice")
	if countFramesContaining(transport, "MARKREAD alice\r\n") != 1 {
		t.Fatalf("first select must send one MARKREAD get, frames=%v", transport.WrittenFrames())
	}
	c.SelectConversation("net", "#room")
	c.SelectConversation("net", "alice")
	if countFramesContaining(transport, "MARKREAD alice\r\n") != 1 {
		t.Fatalf("reselect must not send another MARKREAD get, frames=%v", transport.WrittenFrames())
	}
}

func TestDirectMarkreadGetWaitsForReadMarkerCap(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc")
	roomKey := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(roomKey, "#room", irc.CauseChannelState)
	c.SelectConversation("net", "#room")
	key := c.reducer.ConversationKey("net", "alice")
	c.reducer.EnsureConversation(key, "alice", irc.CauseInboundOther)
	c.SelectConversation("net", "alice")
	if writtenFramesContain(transport, "MARKREAD alice\r\n") {
		t.Fatal("must not mark get sent before the cap is available")
	}
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.SelectConversation("net", "#room")
	c.SelectConversation("net", "alice")
	if !writtenFramesContain(transport, "MARKREAD alice\r\n") {
		t.Fatalf("must retry MARKREAD get once the cap is available, frames=%v", transport.WrittenFrames())
	}
}

func TestReconnectSelectedDirectGetsReadMarkerOnWelcome(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	key := c.reducer.ConversationKey("net", "alice")
	c.reducer.EnsureConversation(key, "alice", irc.CauseInboundOther)
	c.SelectConversation("net", "alice")
	if countFramesContaining(transport, "MARKREAD alice\r\n") != 1 {
		t.Fatalf("first select must send one get, frames=%v", transport.WrittenFrames())
	}
	c.Apply(irc.WelcomeEvent{NetworkID: "net", CurrentNick: "omairc"})
	if countFramesContaining(transport, "MARKREAD alice\r\n") != 2 {
		t.Fatalf("welcome while DM selected must send one new get, frames=%v", transport.WrittenFrames())
	}
	c.SelectConversation("net", "#room")
	roomKey := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(roomKey, "#room", irc.CauseChannelState)
	c.SelectConversation("net", "alice")
	if countFramesContaining(transport, "MARKREAD alice\r\n") != 2 {
		t.Fatalf("reselect after welcome must not send again, frames=%v", transport.WrittenFrames())
	}
}

func TestSelectDirectBeforeWelcomeRetriesGetAfterRegistered(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	inject(t, transport, ":server CAP omairc LS :multi-prefix draft/read-marker\r\n")
	inject(t, transport, ":server CAP omairc ACK :multi-prefix draft/read-marker\r\n")
	key := c.reducer.ConversationKey("net", "alice")
	c.reducer.EnsureConversation(key, "alice", irc.CauseInboundOther)
	c.SelectConversation("net", "alice")
	if writtenFramesContain(transport, "MARKREAD alice\r\n") {
		t.Fatalf("must not send MARKREAD get before 001, frames=%v", transport.WrittenFrames())
	}
	inject(t, transport, ":server 001 omairc :Welcome\r\n")
	if countFramesContaining(transport, "MARKREAD alice\r\n") != 1 {
		t.Fatalf("001 while DM selected must send one MARKREAD get, frames=%v", transport.WrittenFrames())
	}
}

func TestWelcomeWithSelectedChannelNoReadMarkerGet(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	roomKey := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(roomKey, "#room", irc.CauseChannelState)
	c.SelectConversation("net", "#room")
	before := countFramesContaining(transport, "MARKREAD")
	c.Apply(irc.WelcomeEvent{NetworkID: "net", CurrentNick: "omairc"})
	if countFramesContaining(transport, "MARKREAD") != before {
		t.Fatalf("welcome with channel selected must not send MARKREAD get, frames=%v", transport.WrittenFrames())
	}
}

func TestWelcomeOnOtherNetworkDoesNotFetchSelectedDirect(t *testing.T) {
	c, clock := newController(t)
	transportNet := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	transportOther := addAndStart(t, c, clock, baseConfig("other", "bob"))
	registerNetwork(t, transportNet, "omairc", "draft/read-marker")
	registerNetwork(t, transportOther, "bob", "draft/read-marker")
	key := c.reducer.ConversationKey("net", "alice")
	c.reducer.EnsureConversation(key, "alice", irc.CauseInboundOther)
	c.SelectConversation("net", "alice")
	before := countFramesContaining(transportNet, "MARKREAD alice\r\n")
	c.Apply(irc.WelcomeEvent{NetworkID: "other", CurrentNick: "bob"})
	if countFramesContaining(transportNet, "MARKREAD alice\r\n") != before {
		t.Fatalf("welcome on other network must not fetch net direct, frames=%v", transportNet.WrittenFrames())
	}
}

func TestReadMarkerGetForgottenWhenDirectDropped(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	key := c.reducer.ConversationKey("net", "alice")
	c.reducer.EnsureConversation(key, "alice", irc.CauseInboundOther)
	c.SelectConversation("net", "alice")
	if countFramesContaining(transport, "MARKREAD alice\r\n") != 1 {
		t.Fatalf("frames=%v", transport.WrittenFrames())
	}
	c.DropConversationAndReselect(key, true)
	c.reducer.EnsureConversation(key, "alice", irc.CauseInboundOther)
	c.SelectConversation("net", "alice")
	if countFramesContaining(transport, "MARKREAD alice\r\n") != 2 {
		t.Fatalf("dropping a direct must allow one MARKREAD get again, frames=%v", transport.WrittenFrames())
	}
}

func TestReadMarkerServerPushAfterReconnect(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	roomKey := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(roomKey, "#room", irc.CauseChannelState)
	c.SelectConversation("net", "#room")
	key := c.reducer.ConversationKey("net", "alice")
	c.reducer.EnsureConversation(key, "alice", irc.CauseInboundOther)
	when := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	c.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       "alice",
		Author:       "alice",
		Body:         "hi",
		Timestamp:    when,
		ServerTime:   &when,
	})
	if c.reducer.Find(key).Unread != 1 {
		t.Fatal("expected unread before server marker")
	}
	inject(t, transport, ":server MARKREAD alice timestamp=2024-06-01T12:00:00.000Z\r\n")
	if c.reducer.Find(key).Unread != 0 {
		t.Fatalf("server marker must clear unread, got %d", c.reducer.Find(key).Unread)
	}
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	later := when.Add(time.Minute)
	c.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       "alice",
		Author:       "alice",
		Body:         "again",
		Timestamp:    later,
		ServerTime:   &later,
	})
	if c.reducer.Find(key).Unread != 1 {
		t.Fatal("expected unread after new line")
	}
	inject(t, transport, ":server MARKREAD alice timestamp=2024-06-01T12:01:00.000Z\r\n")
	if c.reducer.Find(key).Unread != 0 {
		t.Fatalf("server marker after reconnect must clear unread, got %d", c.reducer.Find(key).Unread)
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
	if countFramesContaining(transport, "MARKREAD alice\r\n") != 1 {
		t.Fatalf("opening existing DM must send one MARKREAD get, frames=%v", transport.WrittenFrames())
	}
}

func TestFirstConversationClaimRequestsReadMarkerGetForDirect(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	key := c.reducer.ConversationKey("net", "alice")
	when := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	c.reducer.EnsureConversation(key, "alice", irc.CauseInboundOther)
	c.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       "alice",
		Author:       "alice",
		Body:         "hi",
		Timestamp:    when,
		ServerTime:   &when,
	})
	if c.SelectedTarget() != "alice" {
		t.Fatalf("selected = %q, want alice", c.SelectedTarget())
	}
	if countFramesContaining(transport, "MARKREAD alice\r\n") != 1 {
		t.Fatalf("first conversation claim must send one MARKREAD get, frames=%v", transport.WrittenFrames())
	}
}

func applyMessage(c *Controller, key irc.ConversationKey, author, body string, when time.Time) {
	c.reducer.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       key.NormalizedTarget,
		Author:       author,
		Body:         body,
		Timestamp:    when,
		ServerTime:   &when,
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

func TestSelectDirectGetUsesDisplayName(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	room := c.reducer.ConversationKey("net", "#room")
	c.reducer.EnsureConversation(room, "#room", irc.CauseChannelState)
	c.SelectConversation("net", "#room")
	before := len(transport.WrittenFrames())
	alice := c.reducer.ConversationKey("net", "Alice")
	c.reducer.EnsureConversation(alice, "Alice", irc.CauseInboundOther)
	c.SelectConversation("net", "alice")
	frames := transport.WrittenFrames()[before:]
	if countFramesIn(frames, "MARKREAD Alice\r\n") != 1 {
		t.Fatalf("get must use display name, frames=%v", frames)
	}
	if framesContainBytes(frames, "MARKREAD alice\r\n") {
		t.Fatalf("folded form must not be used on the wire, frames=%v", frames)
	}
}

func TestReconnectSelectedDirectDoesNotSetOlderThanGetReply(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, true)
	alice := c.reducer.ConversationKey("net", "Alice")
	c.reducer.EnsureConversation(alice, "Alice", irc.CauseInboundOther)
	older := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	applyMessage(c, alice, "Alice", "hi", older)
	c.SelectConversation("net", "Alice")
	before := len(transport.WrittenFrames())
	c.Apply(irc.WelcomeEvent{NetworkID: "net", CurrentNick: "omairc"})
	frames := transport.WrittenFrames()[before:]
	if countFramesIn(frames, "MARKREAD Alice\r\n") != 1 {
		t.Fatalf("welcome must send one get, frames=%v", frames)
	}
	if framesContainBytes(frames, "timestamp=") {
		t.Fatalf("welcome must not publish a set before the get reply, frames=%v", frames)
	}
	newer := older.Add(time.Hour)
	inject(t, transport, ":server MARKREAD Alice :timestamp="+irc.FormatReadMarkerTime(newer)+"\r\n")
	frames = transport.WrittenFrames()[before:]
	if framesContainBytes(frames, "timestamp=2024-06-01T12:00:00.000Z") {
		t.Fatalf("must not set an older marker after newer get reply, frames=%v", frames)
	}
}

func TestStarGetReplyReleasesHoldAndSendsOneSet(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, true)
	alice := c.reducer.ConversationKey("net", "Alice")
	c.reducer.EnsureConversation(alice, "Alice", irc.CauseInboundOther)
	at := time.Date(2024, 6, 1, 12, 0, 0, 123000000, time.UTC)
	applyMessage(c, alice, "Alice", "hi", at)
	before := len(transport.WrittenFrames())
	c.SelectConversation("net", "Alice")
	frames := transport.WrittenFrames()[before:]
	if framesContainBytes(frames, "timestamp=") {
		t.Fatalf("must hold the set until the get reply, frames=%v", frames)
	}
	inject(t, transport, ":server MARKREAD Alice :*\r\n")
	frames = transport.WrittenFrames()[before:]
	want := "MARKREAD Alice timestamp=2024-06-01T12:00:00.123Z\r\n"
	if countFramesIn(frames, want) != 1 {
		t.Fatalf("star reply must release the hold and send exactly one set, frames=%v", frames)
	}
}

func TestOlderGetReplyReleasesHoldAndSendsOneNewerSet(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, true)
	alice := c.reducer.ConversationKey("net", "Alice")
	c.reducer.EnsureConversation(alice, "Alice", irc.CauseInboundOther)
	newer := time.Date(2024, 6, 1, 13, 0, 0, 0, time.UTC)
	applyMessage(c, alice, "Alice", "hi", newer)
	before := len(transport.WrittenFrames())
	c.SelectConversation("net", "Alice")
	inject(t, transport, ":server MARKREAD Alice :timestamp=2024-06-01T12:00:00.000Z\r\n")
	frames := transport.WrittenFrames()[before:]
	want := "MARKREAD Alice timestamp=2024-06-01T13:00:00.000Z\r\n"
	if countFramesIn(frames, want) != 1 {
		t.Fatalf("older get reply must release hold and send newer stamp once, frames=%v", frames)
	}
}

func TestFailGetReplyReleasesHoldAndSendsSet(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, true)
	alice := c.reducer.ConversationKey("net", "Alice")
	c.reducer.EnsureConversation(alice, "Alice", irc.CauseInboundOther)
	at := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	applyMessage(c, alice, "Alice", "hi", at)
	before := len(transport.WrittenFrames())
	c.SelectConversation("net", "Alice")
	frames := transport.WrittenFrames()[before:]
	if framesContainBytes(frames, "timestamp=") {
		t.Fatalf("must hold until FAIL releases, frames=%v", frames)
	}
	inject(t, transport, ":server FAIL MARKREAD INTERNAL_ERROR Alice :oops\r\n")
	frames = transport.WrittenFrames()[before:]
	want := "MARKREAD Alice timestamp=2024-06-01T12:00:00.000Z\r\n"
	if countFramesIn(frames, want) != 1 {
		t.Fatalf("FAIL must release the hold and allow one set, frames=%v", frames)
	}
}

func setAsciiCaseMapping(c *Controller, networkID string) {
	features := irc.NewServerFeatures()
	features.ApplyToken("CASEMAPPING=ascii")
	c.reducer.SetServerFeatures(networkID, features)
}

func TestReconnectAsciiDmHoldsUntilGetReplyAfterFeatureReset(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, true)
	setAsciiCaseMapping(c, "net")
	peer := "foo[bar]"
	key := c.reducer.ConversationKey("net", peer)
	if key.NormalizedTarget != "foo[bar]" {
		t.Fatalf("ascii must keep brackets, got %q", key.NormalizedTarget)
	}
	c.reducer.EnsureConversation(key, peer, irc.CauseInboundOther)
	marker := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	local := time.Date(2024, 6, 1, 12, 30, 0, 0, time.UTC)
	c.ReadMarkerReceived("net", peer, &marker)
	applyMessage(c, key, peer, "hi", local)
	c.SelectConversation("net", peer)
	before := len(transport.WrittenFrames())
	// Registered resets features to rfc1459 before Welcome; CASEMAPPING returns with 005.
	c.reducer.SetServerFeatures("net", irc.NewServerFeatures())
	c.Apply(irc.WelcomeEvent{NetworkID: "net", CurrentNick: "omairc"})
	frames := transport.WrittenFrames()[before:]
	if countFramesIn(frames, "MARKREAD "+peer+"\r\n") != 1 {
		t.Fatalf("welcome must send one get under the selected key, frames=%v", frames)
	}
	if framesContainBytes(frames, "timestamp=") {
		t.Fatalf("hold must match after the rfc1459 feature reset, frames=%v", frames)
	}
	setAsciiCaseMapping(c, "net")
	inject(t, transport, ":server MARKREAD "+peer+" :timestamp="+irc.FormatReadMarkerTime(marker)+"\r\n")
	frames = transport.WrittenFrames()[before:]
	want := "MARKREAD " + peer + " timestamp=2024-06-01T12:30:00.000Z\r\n"
	if countFramesIn(frames, want) != 1 {
		t.Fatalf("after 005 and the get reply, exactly one newer set must go out, frames=%v", frames)
	}
}

func TestUnparseableGetReplyReleasesHoldAndSendsSet(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, true)
	alice := c.reducer.ConversationKey("net", "Alice")
	c.reducer.EnsureConversation(alice, "Alice", irc.CauseInboundOther)
	at := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	applyMessage(c, alice, "Alice", "hi", at)
	before := len(transport.WrittenFrames())
	c.SelectConversation("net", "Alice")
	frames := transport.WrittenFrames()[before:]
	if framesContainBytes(frames, "timestamp=") {
		t.Fatalf("must hold until the unparseable reply, frames=%v", frames)
	}
	inject(t, transport, ":server MARKREAD Alice :timestamp=not-a-time\r\n")
	frames = transport.WrittenFrames()[before:]
	want := "MARKREAD Alice timestamp=2024-06-01T12:00:00.000Z\r\n"
	if countFramesIn(frames, want) != 1 {
		t.Fatalf("unparseable reply must release the hold and allow one set, frames=%v", frames)
	}
}

func TestFailWithoutTargetReleasesHoldAndSendsSet(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, true)
	alice := c.reducer.ConversationKey("net", "Alice")
	c.reducer.EnsureConversation(alice, "Alice", irc.CauseInboundOther)
	at := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	applyMessage(c, alice, "Alice", "hi", at)
	before := len(transport.WrittenFrames())
	c.SelectConversation("net", "Alice")
	frames := transport.WrittenFrames()[before:]
	if framesContainBytes(frames, "timestamp=") {
		t.Fatalf("must hold until the untargeted FAIL, frames=%v", frames)
	}
	inject(t, transport, ":server FAIL MARKREAD NEED_MORE_PARAMS :Missing parameters\r\n")
	frames = transport.WrittenFrames()[before:]
	want := "MARKREAD Alice timestamp=2024-06-01T12:00:00.000Z\r\n"
	if countFramesIn(frames, want) != 1 {
		t.Fatalf("FAIL without a target must release the hold and allow one set, frames=%v", frames)
	}
}

func TestCapDropReleasesHoldAndAllowsSetWhenCapReturns(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, true)
	alice := c.reducer.ConversationKey("net", "Alice")
	c.reducer.EnsureConversation(alice, "Alice", irc.CauseInboundOther)
	at := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	applyMessage(c, alice, "Alice", "hi", at)
	before := len(transport.WrittenFrames())
	c.SelectConversation("net", "Alice")
	frames := transport.WrittenFrames()[before:]
	if framesContainBytes(frames, "timestamp=") {
		t.Fatalf("must hold before the cap drop, frames=%v", frames)
	}
	c.CapabilitiesChanged("net", irc.CapabilitySet(0))
	frames = transport.WrittenFrames()[before:]
	want := "MARKREAD Alice timestamp=2024-06-01T12:00:00.000Z\r\n"
	if countFramesIn(frames, want) != 1 {
		t.Fatalf("dropping the read-marker cap must release the hold, frames=%v", frames)
	}
}

func TestReceiveFallbackReleasesHoldWhenReplyKeyMismatchesSelected(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, true)
	setAsciiCaseMapping(c, "net")
	peer := "foo[bar]"
	key := c.reducer.ConversationKey("net", peer)
	c.reducer.EnsureConversation(key, peer, irc.CauseInboundOther)
	at := time.Date(2024, 6, 1, 12, 30, 0, 0, time.UTC)
	applyMessage(c, key, peer, "hi", at)
	c.SelectConversation("net", peer)
	before := len(transport.WrittenFrames())
	c.reducer.SetServerFeatures("net", irc.NewServerFeatures())
	c.Apply(irc.WelcomeEvent{NetworkID: "net", CurrentNick: "omairc"})
	frames := transport.WrittenFrames()[before:]
	if framesContainBytes(frames, "timestamp=") {
		t.Fatalf("must hold after welcome under the selected ascii key, frames=%v", frames)
	}
	// Reply while still on rfc1459: ConversationKey folds to foo{bar}, so the
	// primary clear misses and the selected-key fallback must release the hold.
	inject(t, transport, ":server MARKREAD "+peer+" :*\r\n")
	frames = transport.WrittenFrames()[before:]
	want := "MARKREAD " + peer + " timestamp=2024-06-01T12:30:00.000Z\r\n"
	if countFramesIn(frames, want) != 1 {
		t.Fatalf("receive fallback must release the hold before 005, frames=%v", frames)
	}
}

func countFramesIn(frames [][]byte, needle string) int {
	count := 0
	for _, frame := range frames {
		if strings.Contains(string(frame), needle) {
			count++
		}
	}
	return count
}

func framesContainBytes(frames [][]byte, needle string) bool {
	for _, frame := range frames {
		if strings.Contains(string(frame), needle) {
			return true
		}
	}
	return false
}

func TestFailSetDoesNotImmediatelyResendSameStamp(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, true)
	alice := c.reducer.ConversationKey("net", "Alice")
	c.reducer.EnsureConversation(alice, "Alice", irc.CauseInboundOther)
	at := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	applyMessage(c, alice, "Alice", "hi", at)
	before := len(transport.WrittenFrames())
	c.SelectConversation("net", "Alice")
	inject(t, transport, ":server MARKREAD Alice :*\r\n")
	want := "MARKREAD Alice timestamp=2024-06-01T12:00:00.000Z\r\n"
	if countFramesIn(transport.WrittenFrames()[before:], want) != 1 {
		t.Fatalf("want one set after star, frames=%v", transport.WrittenFrames()[before:])
	}
	inject(t, transport, ":server FAIL MARKREAD RATE_LIMITED Alice :slow\r\n")
	// The shell's NotifyMsg path re-checks the viewport after a status FAIL line.
	c.NoteTranscriptViewport(true, true)
	if countFramesIn(transport.WrittenFrames()[before:], want) != 1 {
		t.Fatalf("FAIL must not immediately resend the same stamp, frames=%v", transport.WrittenFrames()[before:])
	}
}

func TestQuickReconnectAfterFailedSetGetBeforeStaleSet(t *testing.T) {
	c, clock := newController(t)
	config := baseConfig("net", "omairc")
	config.ReconnectEnabled = true
	transport := addAndStart(t, c, clock, config)
	registerNetwork(t, transport, "omairc", "draft/read-marker")
	c.NoteTranscriptViewport(true, true)
	alice := c.reducer.ConversationKey("net", "Alice")
	c.reducer.EnsureConversation(alice, "Alice", irc.CauseInboundOther)
	failedAt := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	applyMessage(c, alice, "Alice", "hi", failedAt)
	c.SelectConversation("net", "Alice")
	inject(t, transport, ":server MARKREAD Alice :*\r\n")
	wantFail := "MARKREAD Alice timestamp=2024-06-01T12:00:00.000Z\r\n"
	if !writtenFramesContain(transport, wantFail) {
		t.Fatalf("want initial set, frames=%v", transport.WrittenFrames())
	}
	inject(t, transport, ":server FAIL MARKREAD RATE_LIMITED Alice :slow\r\n")
	// A shell Publish during Welcome must not beat the get: sync as OnViewChanged would.
	c.OnViewChanged = func() { c.syncReadMarkerForSelection() }

	transport.RemoteClose()
	if clock.Pending() == 0 {
		t.Fatal("reconnect timer not armed")
	}
	clock.Advance(time.Hour)
	transport.CompleteConnect()
	// Negotiate caps before the 001 race so CAP handlers are not blocked on our lock.
	inject(t, transport, ":server CAP omairc LS :draft/read-marker multi-prefix\r\n")
	inject(t, transport, ":server CAP omairc ACK :draft/read-marker multi-prefix\r\n")

	// Race: shell holds the controller lock while 001 registers the session.
	// Viewport sync must not publish the failed stamp before Welcome's get.
	c.Lock()
	registered := make(chan struct{})
	go func() {
		defer close(registered)
		inject(t, transport, ":server 001 omairc :Welcome\r\n")
	}()
	deadline := time.Now().Add(2 * time.Second)
	for c.Session("net").State() != session.StateRegistered {
		if time.Now().After(deadline) {
			c.Unlock()
			t.Fatal("timed out waiting for Registered under lock")
		}
		time.Sleep(time.Millisecond)
	}
	before := len(transport.WrittenFrames())
	c.NoteTranscriptViewport(true, true)
	c.syncReadMarkerForSelection()
	if framesContainBytes(transport.WrittenFrames()[before:], "timestamp=") {
		c.Unlock()
		t.Fatalf("Registered-before-Welcome sync must not set, frames=%v", transport.WrittenFrames()[before:])
	}
	c.Unlock()
	<-registered

	frames := transport.WrittenFrames()[before:]
	getIdx := -1
	setIdx := -1
	for i, frame := range frames {
		line := string(frame)
		if !strings.Contains(line, "MARKREAD Alice") {
			continue
		}
		if strings.Contains(line, "timestamp=") {
			if setIdx < 0 {
				setIdx = i
			}
			continue
		}
		if getIdx < 0 {
			getIdx = i
		}
	}
	if getIdx < 0 {
		t.Fatalf("welcome must send a get, frames=%v", frames)
	}
	if setIdx >= 0 && setIdx < getIdx {
		t.Fatalf("get must go out before any set, frames=%v", frames)
	}
	if framesContainBytes(frames, "timestamp=2024-06-01T12:00:00.000Z") {
		t.Fatalf("must not send the failed older stamp before the get reply, frames=%v", frames)
	}
	serverNow := failedAt.Add(2 * time.Second)
	inject(t, transport, ":server MARKREAD Alice :timestamp="+irc.FormatReadMarkerTime(serverNow)+"\r\n")
	frames = transport.WrittenFrames()[before:]
	if framesContainBytes(frames, "timestamp=2024-06-01T12:00:00.000Z") {
		t.Fatalf("nothing older than the server marker may go out, frames=%v", frames)
	}
}
