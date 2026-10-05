package ui

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
)

// TestRejoinClosedChannelsFitsTheFrame is the 118×30 close-then-rejoin
// sequence. Part keeps each row, close removes it, and /join #desktop,#omarchy
// from #help brings both channels back. The joined frame stays the terminal
// height: the #omarchy topic is in the header, #help keeps its own sidebar
// row, and the typed command is gone.
func TestRejoinClosedChannelsFitsTheFrame(t *testing.T) {
	m := seededModel(t)
	if m.width != 118 || m.height != 30 {
		t.Fatalf("seeded size = %dx%d, want 118x30", m.width, m.height)
	}
	send := func(target, text string) {
		t.Helper()
		m.switchSelection(func() { m.ctrl.SelectConversation("omarchy", target) })
		m.composer.SetValue(text)
		m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	}
	send("#desktop", "/part")
	send("#desktop", "/close")
	send("#omarchy", "/part")
	send("#omarchy", "/close")
	send("#help", "/join #desktop,#omarchy")

	view := m.View()
	if view.WindowTitle != "#omarchy · irc.example · fred - Omairc" {
		t.Fatalf("title = %q", view.WindowTitle)
	}
	plain := ansiPattern.ReplaceAllString(view.Content, "")
	lines := strings.Split(plain, "\n")
	if len(lines) != 30 {
		t.Fatalf("rendered rows = %d, want 30", len(lines))
	}
	if !strings.Contains(plain, "A cozy corner for Omarchy users and builders.") {
		t.Fatalf("view missing the #omarchy topic:\n%s", plain)
	}
	helpRow := false
	for _, line := range lines {
		if strings.Contains(line, "#help") && !strings.Contains(line, "CHAN") {
			helpRow = true
		}
		if strings.Contains(line, "CHANhelp") {
			t.Fatalf("CHANNELS and #help share a row: %q", line)
		}
	}
	if !helpRow {
		t.Fatalf("#help is not on its own row:\n%s", plain)
	}
	if strings.Contains(plain, "/join #desktop") {
		t.Fatalf("the typed join stayed on screen:\n%s", plain)
	}
	if view.Cursor == nil || view.Cursor.Position.Y < 0 || view.Cursor.Position.Y > m.height-1 {
		t.Fatalf("cursor = %v, want a row inside 0..%d", view.Cursor, m.height-1)
	}
	if m.ctrl.PeopleCount() != 1 {
		t.Fatalf("people = %d, want 1", m.ctrl.PeopleCount())
	}
}
