package ui

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
)

func TestSeededViewContent(t *testing.T) {
	ctrl := controller.New()
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := New(ctrl)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)

	view := m.View()
	for _, wanted := range []string{
		"#omarchy",
		"#ricing",
		"anna",
		"dax",
		"12",
		"irc.example · fred",
		"irc.example · oak",
	} {
		if !strings.Contains(view.Content, wanted) {
			t.Fatalf("View content missing %q:\n%s", wanted, view.Content)
		}
	}
	if view.WindowTitle != Title(ctrl) {
		t.Fatalf("WindowTitle = %q, want %q", view.WindowTitle, Title(ctrl))
	}
	if !view.AltScreen {
		t.Fatal("AltScreen = false, want true")
	}
}

func TestSeededSidebarUsesNetworkOrder(t *testing.T) {
	ctrl := controller.New()
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := New(ctrl)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)

	content := m.View().Content
	omarchy := strings.Index(content, "irc.example · fred")
	oftc := strings.Index(content, "irc.example · oak")
	if omarchy < 0 || oftc < 0 {
		t.Fatalf("View content missing a demo display name (omarchy=%d, oftc=%d):\n%s", omarchy, oftc, content)
	}
	if omarchy > oftc {
		t.Fatalf("omarchy rendered after oftc (omarchy=%d, oftc=%d):\n%s", omarchy, oftc, content)
	}
}

func TestEmptyViewAtTinySizeDoesNotPanic(t *testing.T) {
	m := New(controller.New())
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 10, Height: 4})
	m = updated.(*Model)
	if content := m.View().Content; content == "" {
		t.Fatal("View content empty at tiny size")
	}
}
