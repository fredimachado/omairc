package ui

import (
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// lastTagmsg returns the most recent typing TAGMSG written to the transport.
func lastTagmsg(transport *session.LoopbackTransport) string {
	frames := transport.WrittenFrames()
	for index := len(frames) - 1; index >= 0; index-- {
		if strings.Contains(string(frames[index]), "TAGMSG") {
			return string(frames[index])
		}
	}
	return ""
}

// TestComposerKeystrokePublishesTyping proves the shell reports composer text
// to the controller: a keystroke in a channel that negotiated message-tags
// writes typing=active, and a later non-live edit withdraws it with
// typing=done. Without the shell wiring, the controller API alone would never
// reach the wire.
func TestComposerKeystrokePublishesTyping(t *testing.T) {
	clock := session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC))
	ctrl := controller.New()
	ctrl.SetClock(clock)
	transport := session.NewLoopbackTransport()
	config := session.DefaultSessionConfig("libera", "libera", "irc.example", "omairc")
	if _, err := ctrl.AddSession(config, transport, clock); err != nil {
		t.Fatalf("AddSession: %v", err)
	}
	if !ctrl.Start("libera") {
		t.Fatal("Start(libera) = false")
	}
	transport.CompleteConnect()
	transport.InjectBytes([]byte(":server CAP omairc LS :message-tags\r\n" +
		":server CAP omairc ACK :message-tags\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n"))
	ctrl.SelectConversation("libera", "#omarchy")

	m := New(ctrl, nil)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)

	m = press(t, m, tea.KeyPressMsg{Code: 'h', Text: "h"})
	if got := lastTagmsg(transport); got != "@+typing=active TAGMSG #omarchy\r\n" {
		t.Fatalf("last TAGMSG after a keystroke = %q", got)
	}

	// Advance exactly one typing interval so the withdrawal is allowed without
	// tripping the 60s ping watchdog.
	clock.Advance(time.Duration(irc.TypingSendIntervalMs) * time.Millisecond)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyBackspace})
	if got := m.composer.Value(); got != "" {
		t.Fatalf("composer after backspace = %q, want empty", got)
	}
	if got := lastTagmsg(transport); got != "@+typing=done TAGMSG #omarchy\r\n" {
		t.Fatalf("last TAGMSG after clearing the live line = %q", got)
	}
}
