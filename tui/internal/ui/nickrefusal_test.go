package ui

import (
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/session"
)

func plainSidebar(m *Model) string {
	return ansiPattern.ReplaceAllString(m.sidebarView(sidebarWidth(m.width), m.bodyHeight()), "")
}

func lineAfter(text, header string) string {
	lines := strings.Split(text, "\n")
	for index, line := range lines {
		if strings.Contains(line, header) && index+1 < len(lines) {
			return strings.TrimSpace(lines[index+1])
		}
	}
	return ""
}

// TestNickRefusalReasonOnNetworkAndOpenPane shows the server sentence under
// that network and in the open Status pane, and keeps another network's pane
// on its own error.
func TestNickRefusalReasonOnNetworkAndOpenPane(t *testing.T) {
	m := seededModel(t)
	const omarchyReason = "Erroneous nickname"
	const oftcReason = "Nickname is already in use."
	m.ctrl.ErrorOccurred("omarchy", session.ErrorRegistration, omarchyReason)
	m.ctrl.ErrorOccurred("oftc", session.ErrorRegistration, oftcReason)

	sidebar := plainSidebar(m)
	omarchyHeader := "▾ " + m.sidebarNetworkDisplayName("omarchy")
	oftcHeader := "▾ " + m.sidebarNetworkDisplayName("oftc")
	if got := lineAfter(sidebar, omarchyHeader); got != omarchyReason {
		t.Fatalf("omarchy status = %q, want %q\n%s", got, omarchyReason, sidebar)
	}
	if got := lineAfter(sidebar, oftcHeader); got != oftcReason {
		t.Fatalf("oftc status = %q, want %q\n%s", got, oftcReason, sidebar)
	}
	if strings.Contains(sidebar, "IRC registration was refused") {
		t.Fatal("the generic numeric refusal must not replace the server sentence")
	}

	m.toggleStatus()
	if !m.ctrl.ConsoleOpen() {
		t.Fatal("Status must be the open pane")
	}
	header := ansiPattern.ReplaceAllString(strings.Join(m.transcriptHeader(), "\n"), "")
	if !strings.Contains(header, omarchyReason) {
		t.Fatalf("open pane = %q, want %q", header, omarchyReason)
	}
	if strings.Contains(header, oftcReason) {
		t.Fatalf("open pane shows the other network's reason: %q", header)
	}

	m.ctrl.OpenStatus("oftc")
	header = ansiPattern.ReplaceAllString(strings.Join(m.transcriptHeader(), "\n"), "")
	if !strings.Contains(header, oftcReason) {
		t.Fatalf("oftc pane = %q, want %q", header, oftcReason)
	}
	if strings.Contains(header, omarchyReason) {
		t.Fatalf("oftc pane shows the other network's reason: %q", header)
	}
}
