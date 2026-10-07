package controller

// This file ports the composer-typing boundary of
// tests/session/tst_typing.cpp: IrcController::notifyComposerText and the
// done-flush on a selection change. It is the outbound half of the typing
// contract; presencetyping_test.go covers the inbound reads.

import (
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// countTagmsg counts the TAGMSG frames written to the transport.
func countTagmsg(transport *session.LoopbackTransport) int {
	count := 0
	for _, frame := range transport.WrittenFrames() {
		if strings.Contains(string(frame), "TAGMSG") {
			count++
		}
	}
	return count
}

// selectChannelWithTags starts one network, grants message-tags, and selects a
// joined channel, the precondition every composer-typing slot needs.
func selectChannelWithTags(t *testing.T) (*Controller, *session.FakeClock, *session.LoopbackTransport) {
	t.Helper()
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("freenode", "omairc"))
	registerNetwork(t, transport, "omairc", "message-tags")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n")
	c.SelectConversation("freenode", "#omarchy")
	if !c.HasTyping() {
		t.Fatal("HasTyping = false after message-tags")
	}
	return c, clock, transport
}

// TestNotifyComposerTextThrottles ports
// TypingTest::controllerNotifyComposerTextThrottles: a live composer line
// publishes typing=active once per target pace, and a non-live line withdraws
// the hint without a second frame while the pace still throttles.
func TestNotifyComposerTextThrottles(t *testing.T) {
	c, clock, transport := selectChannelWithTags(t)

	c.NotifyComposerText("hello")
	if got := lastWrittenFrame(t, transport); got != "@+typing=active TAGMSG #omarchy\r\n" {
		t.Fatalf("last frame = %q", got)
	}
	first := countTagmsg(transport)

	c.NotifyComposerText("hello there")
	if got := countTagmsg(transport); got != first {
		t.Fatalf("TAGMSG frames = %d, want %d (throttled)", got, first)
	}

	// A slash command is not a live message, so it withdraws the hint. The
	// done is inside the pace here, so nothing new reaches the wire.
	c.NotifyComposerText("/join #other")
	if got := countTagmsg(transport); got != first {
		t.Fatalf("TAGMSG frames = %d, want %d (done throttled)", got, first)
	}
	if c.typingTarget != "" {
		t.Fatalf("typingTarget = %q, want cleared after the withdrawal", c.typingTarget)
	}

	// The target was cleared, so a second non-live line cannot send again.
	clock.Advance(time.Duration(irc.TypingSendIntervalMs) * time.Millisecond)
	c.NotifyComposerText("/join #other")
	if got := countTagmsg(transport); got != first {
		t.Fatalf("TAGMSG frames = %d, want %d (target cleared)", got, first)
	}

	// A fresh live line re-arms the target and writes an active hint.
	c.NotifyComposerText("next")
	if got := countTagmsg(transport); got != first+1 {
		t.Fatalf("TAGMSG frames = %d, want %d", got, first+1)
	}
	if got := lastWrittenFrame(t, transport); got != "@+typing=active TAGMSG #omarchy\r\n" {
		t.Fatalf("last frame = %q", got)
	}
}

// TestNotifyComposerTextSuppressedDoneAfterChat ports the chat-then-clear path:
// after a sent message the session suppresses the done, so clearing the
// composer writes nothing extra.
func TestNotifyComposerTextSuppressedDoneAfterChat(t *testing.T) {
	c, clock, transport := selectChannelWithTags(t)

	c.NotifyComposerText("hello")
	if got := lastWrittenFrame(t, transport); got != "@+typing=active TAGMSG #omarchy\r\n" {
		t.Fatalf("last frame = %q", got)
	}
	clock.Advance(time.Duration(irc.TypingSendIntervalMs) * time.Millisecond)
	if !c.SendMessage("hello") {
		t.Fatal("SendMessage(hello) = false")
	}
	before := countTagmsg(transport)
	c.NotifyComposerText("")
	if got := countTagmsg(transport); got != before {
		t.Fatalf("TAGMSG frames = %d, want %d (done suppressed after chat)", got, before)
	}
	if c.typingTarget != "" {
		t.Fatalf("typingTarget = %q, want cleared after the suppressed done", c.typingTarget)
	}
}

// TestNotifyComposerTextWithdrawsOnSelectionChange ports the done flush in
// IrcController::selectConversation.
func TestNotifyComposerTextWithdrawsOnSelectionChange(t *testing.T) {
	c, clock, transport := selectChannelWithTags(t)
	inject(t, transport, ":omairc!u@h JOIN :#desktop\r\n")

	c.NotifyComposerText("hello")
	if c.typingTarget != "#omarchy" {
		t.Fatalf("typingTarget = %q, want #omarchy", c.typingTarget)
	}
	clock.Advance(time.Duration(irc.TypingSendIntervalMs) * time.Millisecond)
	c.SelectConversation("freenode", "#desktop")
	if got := lastWrittenFrame(t, transport); got != "@+typing=done TAGMSG #omarchy\r\n" {
		t.Fatalf("last frame = %q, want the previous target's done", got)
	}
	if c.typingTarget != "" {
		t.Fatalf("typingTarget = %q, want cleared after the selection change", c.typingTarget)
	}
}

// TestNotifyComposerTextRequiresMessageTags pins that a network without
// message-tags never publishes, and the Status console never publishes.
func TestNotifyComposerTextRequiresMessageTags(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("freenode", "omairc"))
	registerNetwork(t, transport, "omairc")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n")
	c.SelectConversation("freenode", "#omarchy")
	if c.HasTyping() {
		t.Fatal("HasTyping = true without message-tags")
	}

	c.NotifyComposerText("hello")
	if got := countTagmsg(transport); got != 0 {
		t.Fatalf("TAGMSG frames = %d, want 0 without message-tags", got)
	}
	if c.typingTarget != "" {
		t.Fatalf("typingTarget = %q, want empty", c.typingTarget)
	}
}
