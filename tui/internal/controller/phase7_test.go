package controller

// This file is the Phase 7 parity suite: the slash verbs, the channel-list
// request/cache round trip, and the Status-surface outcome. It drives the
// controller through SendMessage/ConsoleSubmit exactly as the shell does and
// reuses the Phase 2 harness in controller_test.go for the transport and
// registration fixtures.

import (
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/session"
)

// phase7Setup starts one registered network with #omarchy joined and selected.
// Every Phase 7 slash verb resolves its network through the selected
// conversation, so each test needs a live selection before it can dispatch.
func phase7Setup(t *testing.T) (*Controller, *session.LoopbackTransport) {
	t.Helper()
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/metadata-2", "batch", "echo-message")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n"+
		":server 353 omairc = #omarchy :@omairc dax\r\n"+
		":server 366 omairc #omarchy :End of NAMES\r\n")
	c.SelectConversation("libera", "#omarchy")
	return c, transport
}

// TestPhase7JoinOpensChannelAndConsumes pins that /join writes the JOIN and
// reveals the channel it just joined.
func TestPhase7JoinOpensChannelAndConsumes(t *testing.T) {
	c, transport := phase7Setup(t)

	if !c.SendMessage("/join #help") {
		t.Fatalf("SendMessage(/join #help) = false")
	}
	if got := c.SelectedTarget(); got != "#help" {
		t.Fatalf("SelectedTarget = %q, want #help", got)
	}
	if !writtenFramesContain(transport, "JOIN #help") {
		t.Fatalf("no JOIN #help frame: %v", transport.WrittenFrames())
	}
}

// TestPhase7CloseRefusedOnChannel pins that /close cannot close a channel: the
// command is refused and the selection stays where it was.
func TestPhase7CloseRefusedOnChannel(t *testing.T) {
	c, _ := phase7Setup(t)
	if !c.IsChannel() || c.SelectedTarget() != "#omarchy" {
		t.Fatalf("precondition: target=%q isChannel=%v", c.SelectedTarget(), c.IsChannel())
	}

	if c.SendMessage("/close") {
		t.Fatalf("SendMessage(/close) = true on a channel")
	}
	if got := c.SelectedTarget(); got != "#omarchy" {
		t.Fatalf("SelectedTarget = %q after refused /close, want #omarchy", got)
	}
}

// TestPhase7QueryOpensDirect pins that /query invents and selects a direct
// message rather than a channel.
func TestPhase7QueryOpensDirect(t *testing.T) {
	c, _ := phase7Setup(t)

	if !c.SendMessage("/query dax") {
		t.Fatalf("SendMessage(/query dax) = false")
	}
	if got := c.SelectedTarget(); got != "dax" {
		t.Fatalf("SelectedTarget = %q, want dax", got)
	}
	if !hasConversation(c, "libera", "dax") {
		t.Fatalf("no direct row for dax: %v", conversationTargets(c))
	}
	if c.IsChannel() {
		t.Fatalf("IsChannel = true for a /query direct")
	}
}

// TestPhase7ListStreamsAndCaches pins the /list request state machine: 321/322
// rows stream into the overlay in users-descending order, a completed request
// for the same mask is reused from cache without another LIST frame, and
// DismissChannelList only drops the presented flag.
func TestPhase7ListStreamsAndCaches(t *testing.T) {
	c, transport := phase7Setup(t)

	if !c.SendMessage("/list") {
		t.Fatalf("SendMessage(/list) = false")
	}
	snapshot := c.ChannelListSnapshot()
	if !snapshot.Open {
		t.Fatalf("ChannelListSnapshot().Open = false after /list")
	}
	if !writtenFramesContain(transport, "LIST") {
		t.Fatalf("no LIST frame: %v", transport.WrittenFrames())
	}

	inject(t, transport, ":server 321 omairc Channel :Users  Name\r\n"+
		":server 322 omairc #alpha 5 :A first channel\r\n"+
		":server 322 omairc #beta 10 :A busier channel\r\n"+
		":server 323 omairc :End of /LIST\r\n")

	snapshot = c.ChannelListSnapshot()
	if !snapshot.Open || !snapshot.Complete || snapshot.Loading {
		t.Fatalf("snapshot after 323 = %+v, want open and complete", snapshot)
	}
	if len(snapshot.Rows) != 2 {
		t.Fatalf("Rows = %+v, want 2 rows", snapshot.Rows)
	}
	if snapshot.Rows[0].Channel != "#beta" || snapshot.Rows[0].Users != 10 {
		t.Fatalf("Rows[0] = %+v, want #beta/10 (users descending)", snapshot.Rows[0])
	}
	if snapshot.Rows[1].Channel != "#alpha" || snapshot.Rows[1].Users != 5 {
		t.Fatalf("Rows[1] = %+v, want #alpha/5", snapshot.Rows[1])
	}

	// The overlay must be dismissed before the second /list so the same mask
	// resolves through the cache; while it is still presented, /list is a
	// deliberate refresh and would reissue LIST.
	c.DismissChannelList()
	if c.ChannelListSnapshot().Open {
		t.Fatalf("Open = true after DismissChannelList")
	}

	framesBeforeSecondList := len(transport.WrittenFrames())
	if !c.SendMessage("/list") {
		t.Fatalf("second SendMessage(/list) = false")
	}
	snapshot = c.ChannelListSnapshot()
	if !snapshot.Open || !snapshot.Cached {
		t.Fatalf("second snapshot = %+v, want open and cached", snapshot)
	}
	if len(snapshot.Rows) != 2 {
		t.Fatalf("cached Rows = %+v, want the 2 cached rows", snapshot.Rows)
	}
	if got := len(transport.WrittenFrames()); got != framesBeforeSecondList {
		t.Fatalf("cached /list wrote a frame: %d -> %d", framesBeforeSecondList, got)
	}

	c.DismissChannelList()
	if c.ChannelListSnapshot().Open {
		t.Fatalf("Open = true after final DismissChannelList")
	}
}

// TestPhase7WhoisWritesWhoisFrame pins the /whois wire shape.
func TestPhase7WhoisWritesWhoisFrame(t *testing.T) {
	c, transport := phase7Setup(t)

	if !c.SendMessage("/whois dax") {
		t.Fatalf("SendMessage(/whois dax) = false")
	}
	if !writtenFramesContain(transport, "WHOIS dax") {
		t.Fatalf("no WHOIS frame: %v", transport.WrittenFrames())
	}
}

// TestPhase7ModeWritesModeFrame pins /mode and the /op //ban wrappers, which
// all funnel through one MODE frame.
func TestPhase7ModeWritesModeFrame(t *testing.T) {
	c, transport := phase7Setup(t)

	if !c.SendMessage("/mode #omarchy +o dax") {
		t.Fatalf("SendMessage(/mode) = false")
	}
	if !writtenFramesContain(transport, "MODE #omarchy +o dax") {
		t.Fatalf("no MODE frame for /mode: %v", transport.WrittenFrames())
	}

	if !c.SendMessage("/op dax") {
		t.Fatalf("SendMessage(/op dax) = false")
	}
	if !writtenFramesContain(transport, "MODE #omarchy +o dax") {
		t.Fatalf("no MODE frame for /op: %v", transport.WrittenFrames())
	}

	if !c.SendMessage("/ban dax") {
		t.Fatalf("SendMessage(/ban dax) = false")
	}
	if !writtenFramesContain(transport, "MODE #omarchy +b dax!*@*") {
		t.Fatalf("no MODE +b frame for /ban: %v", transport.WrittenFrames())
	}
}

// TestPhase7IgnoreOutcomeOnStatus pins /ignore and /ignored feedback to the
// Status console.
func TestPhase7IgnoreOutcomeOnStatus(t *testing.T) {
	c, _ := phase7Setup(t)

	if !c.SendMessage("/ignore dax") {
		t.Fatalf("SendMessage(/ignore dax) = false")
	}
	if !phase7ConsoleContains(c, "libera", "Ignoring dax") {
		t.Fatalf("Status console = %v, want Ignoring dax", c.ConsoleText("libera"))
	}

	if !c.SendMessage("/ignored") {
		t.Fatalf("SendMessage(/ignored) = false")
	}
	if !phase7ConsoleContains(c, "libera", "Ignoring: dax") {
		t.Fatalf("Status console = %v, want Ignoring: dax", c.ConsoleText("libera"))
	}
}

// TestPhase7PrefOnStatus pins that /pref on the Status surface lists every
// toggle label.
func TestPhase7PrefOnStatus(t *testing.T) {
	c, _ := phase7Setup(t)

	if !c.ConsoleSubmit("/pref") {
		t.Fatalf("ConsoleSubmit(/pref) = false")
	}
	text := strings.Join(c.ConsoleText("libera"), "\n")
	for _, label := range []string{
		"Reopen direct messages on startup",
		"Show peer avatars",
		"Open conversations at unread",
	} {
		if !strings.Contains(text, label) {
			t.Fatalf("/pref console text missing %q:\n%s", label, text)
		}
	}
}

// TestPhase7HelpTranscript pins that /help on a conversation stays local: it
// writes a whois transcript row, keeps the selection, and sends no frame.
func TestPhase7HelpTranscript(t *testing.T) {
	c, transport := phase7Setup(t)
	before := len(c.Messages())
	framesBefore := len(transport.WrittenFrames())

	if !c.SendMessage("/help") {
		t.Fatalf("SendMessage(/help) = false")
	}
	if got := c.SelectedTarget(); got != "#omarchy" {
		t.Fatalf("SelectedTarget = %q after /help, want #omarchy", got)
	}
	if got := len(transport.WrittenFrames()); got != framesBefore {
		t.Fatalf("/help wrote a frame: %d -> %d", framesBefore, got)
	}
	messages := c.Messages()
	if len(messages) <= before {
		t.Fatalf("/help added no transcript row: %d -> %d", before, len(messages))
	}
	last := messages[len(messages)-1]
	if last.Kind != "whois" || !strings.Contains(last.Body, "Commands:") {
		t.Fatalf("last /help row = %+v, want a whois command catalog", last)
	}
}

// phase7ConsoleContains reports whether a network's Status console has a line
// containing needle.
func phase7ConsoleContains(c *Controller, networkID, needle string) bool {
	for _, line := range c.ConsoleText(networkID) {
		if strings.Contains(line, needle) {
			return true
		}
	}
	return false
}
