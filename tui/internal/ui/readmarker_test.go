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

func TestStatusToggleDoesNotPublishReadMarker(t *testing.T) {
	clock := session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC))
	ctrl := controller.New()
	ctrl.SetClock(clock)
	transport := session.NewLoopbackTransport()
	config := session.DefaultSessionConfig("freenode", "irc.example", "irc.example", "fred")
	config.TLSEnabled = true
	config.ReconnectEnabled = false
	if _, err := ctrl.AddSession(config, transport, clock); err != nil {
		t.Fatalf("AddSession: %v", err)
	}
	m := New(ctrl, nil)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)
	if !ctrl.Start("freenode") {
		t.Fatal("Start(freenode) = false")
	}
	transport.CompleteConnect()
	transport.InjectBytes([]byte(":server CAP fred LS :draft/read-marker multi-prefix\r\n" +
		":server CAP fred ACK :draft/read-marker multi-prefix\r\n" +
		":server 001 fred :Welcome\r\n" +
		":fred!u@h JOIN :#omarchy\r\n" +
		":server 353 fred = #omarchy :@fred anna\r\n" +
		":server 366 fred #omarchy :End of NAMES\r\n"))
	key := ctrl.Reducer().ConversationKey("freenode", "#omarchy")
	if ctrl.SelectedTarget() != "#omarchy" {
		t.Fatalf("SelectedTarget = %q, want #omarchy", ctrl.SelectedTarget())
	}
	m.transcriptFollowEnd = true
	m.syncReadMarkerViewport()
	first := time.Date(2024, 6, 1, 12, 0, 0, 123000000, time.UTC)
	ctrl.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       "#omarchy",
		Author:       "anna",
		Body:         "hello",
		Timestamp:    first,
		ServerTime:   &first,
	})
	if !framesContain(transport, "MARKREAD #omarchy timestamp=") {
		t.Fatalf("expected MARKREAD when caught up, frames=%v", transport.WrittenFrames())
	}
	before := countMarkerFrames(transport)
	m = press(t, m, tea.KeyPressMsg{Code: '`', Mod: tea.ModCtrl})
	if !ctrl.ConsoleOpen() {
		t.Fatal("Ctrl+` must open Status")
	}
	second := first.Add(time.Minute)
	ctrl.Apply(irc.MessageEvent{
		Conversation: key,
		Target:       "#omarchy",
		Author:       "anna",
		Body:         "again",
		Timestamp:    second,
		ServerTime:   &second,
	})
	if countMarkerFrames(transport) != before {
		t.Fatalf("Status must not publish read markers, frames=%v", transport.WrittenFrames())
	}
}

func framesContain(transport *session.LoopbackTransport, needle string) bool {
	for _, frame := range transport.WrittenFrames() {
		if strings.Contains(string(frame), needle) {
			return true
		}
	}
	return false
}

func countMarkerFrames(transport *session.LoopbackTransport) int {
	count := 0
	for _, frame := range transport.WrittenFrames() {
		if strings.Contains(string(frame), "MARKREAD") {
			count++
		}
	}
	return count
}
