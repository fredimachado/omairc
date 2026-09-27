package session

import (
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// typingTestNow is the fixed instant the C++ publisher tests use as t0.
var typingTestNow = time.Unix(1700000000, 0)

// publisherWasPublishing mirrors IrcTypingPublisher::wasPublishing: a target
// is "publishing" when its last recorded phase was Active.
func publisherWasPublishing(publisher *typingPublisher, target string) bool {
	clock, ok := publisher.targets[target]
	return ok && clock.lastPhase == irc.TypingActive
}

// TestTypingPublisherThrottlesPerTarget ports
// TypingTest::publisherThrottlesPerTarget.
func TestTypingPublisherThrottlesPerTarget(t *testing.T) {
	publisher := newTypingPublisher()

	if !publisher.shouldSend("#omarchy", irc.TypingActive, typingTestNow) {
		t.Fatal("the first active hint must be allowed")
	}
	publisher.recordSent("#omarchy", irc.TypingActive, typingTestNow)
	if !publisherWasPublishing(&publisher, "#omarchy") {
		t.Fatal("the target must be publishing")
	}
	if publisher.shouldSend("#omarchy", irc.TypingActive, typingTestNow.Add(2999*time.Millisecond)) {
		t.Fatal("a hint inside the interval must be throttled")
	}
	if !publisher.shouldSend("#omarchy", irc.TypingActive, typingTestNow.Add(3000*time.Millisecond)) {
		t.Fatal("a hint at the interval must be allowed")
	}
	if !publisher.shouldSend("#desktop", irc.TypingActive, typingTestNow.Add(1000*time.Millisecond)) {
		t.Fatal("a fresh target must not be throttled")
	}
	if publisher.shouldSend("#omarchy", irc.TypingDone, typingTestNow.Add(1000*time.Millisecond)) {
		t.Fatal("done inside the interval must be throttled")
	}
	if !publisher.shouldSend("#omarchy", irc.TypingDone, typingTestNow.Add(3000*time.Millisecond)) {
		t.Fatal("done at the interval must be allowed")
	}
}

// TestTypingPublisherSuppressesDoneAfterChat ports
// TypingTest::publisherSuppressesDoneAfterChat.
func TestTypingPublisherSuppressesDoneAfterChat(t *testing.T) {
	publisher := newTypingPublisher()
	publisher.recordSent("#omarchy", irc.TypingActive, typingTestNow)
	publisher.noteMessageSent("#omarchy")

	if publisher.shouldSend("#omarchy", irc.TypingDone, typingTestNow.Add(4000*time.Millisecond)) {
		t.Fatal("done must be suppressed after a chat message")
	}
	if !publisherWasPublishing(&publisher, "#omarchy") {
		t.Fatal("the target must still be publishing")
	}
	if !publisher.shouldSend("#omarchy", irc.TypingActive, typingTestNow.Add(4000*time.Millisecond)) {
		t.Fatal("an active hint must still be allowed")
	}
	publisher.recordSent("#omarchy", irc.TypingActive, typingTestNow.Add(4000*time.Millisecond))
	if !publisher.shouldSend("#omarchy", irc.TypingDone, typingTestNow.Add(7000*time.Millisecond)) {
		t.Fatal("done must be allowed after a fresh active hint")
	}

	publisher.reset()
	if publisherWasPublishing(&publisher, "#omarchy") {
		t.Fatal("reset must clear the target")
	}
	if !publisher.shouldSend("#omarchy", irc.TypingActive, typingTestNow) {
		t.Fatal("a reset target must accept an active hint")
	}
}

// TestTypingPublisherRefusesPausedAndUnknownTargets ports
// TypingTest::publisherRefusesPausedAndUnknownTargets.
func TestTypingPublisherRefusesPausedAndUnknownTargets(t *testing.T) {
	publisher := newTypingPublisher()
	if publisher.shouldSend("#omarchy", irc.TypingPaused, typingTestNow) {
		t.Fatal("paused must never be sent")
	}
	if publisher.shouldSend("#omarchy", irc.TypingDone, typingTestNow) {
		t.Fatal("done for an unknown target must not be sent")
	}

	if got := string(irc.TypingTagmsg("#omarchy", irc.TypingActive)); got != "@+typing=active TAGMSG #omarchy\r\n" {
		t.Fatalf("TAGMSG = %q", got)
	}
	if got := irc.TypingTagmsg("bad target", irc.TypingActive); got != nil {
		t.Fatalf("TAGMSG for a target with a space = %q, want nil", got)
	}
	if got := irc.TypingTagmsg("", irc.TypingDone); got != nil {
		t.Fatalf("TAGMSG for an empty target = %q, want nil", got)
	}
}

// TestSessionSendTypingRequiresRegisteredAndMessageTags ports the session-level
// gating of IrcSession::sendTyping.
func TestSessionSendTypingRequiresRegisteredAndMessageTags(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	if fixture.session.SendTyping("#omarchy", irc.TypingActive) {
		t.Fatal("typing must be refused before registration")
	}
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n")
	if fixture.session.SendTyping("#omarchy", irc.TypingActive) {
		t.Fatal("typing must be refused without message-tags")
	}
	if framesContain(fixture.frames(), "TAGMSG") {
		t.Fatal("no TAGMSG must be written")
	}
}

// TestSessionSendTypingWritesTagmsgWhenRegistered ports the successful
// session-level typing send.
func TestSessionSendTypingWritesTagmsgWhenRegistered(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :message-tags\r\n" +
		":server CAP omairc ACK :message-tags\r\n" +
		":server 001 omairc :Welcome\r\n")
	if !fixture.session.SendTyping("#omarchy", irc.TypingActive) {
		t.Fatal("typing must be sent")
	}
	if got := fixture.lastFrame(); got != "@+typing=active TAGMSG #omarchy\r\n" {
		t.Fatalf("last frame = %q", got)
	}
}

// TestSessionSendTypingThrottlesPerTarget ports the per-target pacing of
// IrcSession::sendTyping.
func TestSessionSendTypingThrottlesPerTarget(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :message-tags\r\n" +
		":server CAP omairc ACK :message-tags\r\n" +
		":server 001 omairc :Welcome\r\n")

	if !fixture.session.SendTyping("#omarchy", irc.TypingActive) {
		t.Fatal("the first active hint must be sent")
	}
	if fixture.session.SendTyping("#omarchy", irc.TypingPaused) {
		t.Fatal("paused must never be sent")
	}
	if fixture.session.SendTyping("#omarchy", irc.TypingActive) {
		t.Fatal("a hint inside the interval must be throttled")
	}
	if !fixture.session.SendTyping("#desktop", irc.TypingActive) {
		t.Fatal("a fresh target must not be throttled")
	}

	fixture.clock.Advance(time.Duration(irc.TypingSendIntervalMs) * time.Millisecond)
	if !fixture.session.SendTyping("#omarchy", irc.TypingDone) {
		t.Fatal("done at the interval must be sent")
	}
}

// TestSessionSendTypingSuppressesDoneAfterChat ports the chat-then-done
// suppression of IrcSession::sendTyping.
func TestSessionSendTypingSuppressesDoneAfterChat(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :message-tags\r\n" +
		":server CAP omairc ACK :message-tags\r\n" +
		":server 001 omairc :Welcome\r\n")

	if !fixture.session.SendTyping("#omarchy", irc.TypingActive) {
		t.Fatal("the active hint must be sent")
	}
	if !fixture.session.SendPrivmsg("#omarchy", "hi") {
		t.Fatal("the message must be sent")
	}
	if fixture.session.SendTyping("#omarchy", irc.TypingDone) {
		t.Fatal("done must be suppressed right after a chat message")
	}

	fixture.clock.Advance(time.Duration(irc.TypingSendIntervalMs) * time.Millisecond)
	if fixture.session.SendTyping("#omarchy", irc.TypingDone) {
		t.Fatal("done must stay suppressed until a fresh active hint")
	}
	if !fixture.session.SendTyping("#omarchy", irc.TypingActive) {
		t.Fatal("an active hint must clear the suppression")
	}
	fixture.clock.Advance(time.Duration(irc.TypingSendIntervalMs) * time.Millisecond)
	if !fixture.session.SendTyping("#omarchy", irc.TypingDone) {
		t.Fatal("done must be sent after a fresh active hint")
	}
}

// TestSessionSendTypingRefusesInvalidTargets ports the target validation of
// IrcSession::sendTyping.
func TestSessionSendTypingRefusesInvalidTargets(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :message-tags\r\n" +
		":server CAP omairc ACK :message-tags\r\n" +
		":server 001 omairc :Welcome\r\n")

	before := len(fixture.frames())
	if fixture.session.SendTyping("", irc.TypingActive) {
		t.Fatal("an empty target must be refused")
	}
	if fixture.session.SendTyping("bad target", irc.TypingActive) {
		t.Fatal("a target with a space must be refused")
	}
	if len(fixture.frames()) != before {
		t.Fatal("invalid typing targets must not write frames")
	}
}
