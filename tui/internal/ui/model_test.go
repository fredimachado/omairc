package ui

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
)

func TestSeededViewContent(t *testing.T) {
	ctrl := controller.New()
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := New(ctrl, nil)
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
	if view.WindowTitle != Title(ctrl, nil) {
		t.Fatalf("WindowTitle = %q, want %q", view.WindowTitle, Title(ctrl, nil))
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
	m := New(ctrl, nil)
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
	m := New(controller.New(), nil)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 10, Height: 4})
	m = updated.(*Model)
	view := m.View()
	plain := ansiPattern.ReplaceAllString(view.Content, "")
	if !strings.Contains(plain, "Minimum 20") || !strings.Contains(plain, "10x4") {
		t.Fatalf("tiny size must show the notice, got:\n%s", plain)
	}
	lines := strings.Split(view.Content, "\n")
	if len(lines) != 4 {
		t.Fatalf("frame height = %d, want 4", len(lines))
	}
	for index, line := range lines {
		if got := lipgloss.Width(line); got != 10 {
			t.Fatalf("row %d width = %d, want 10", index, got)
		}
	}

	zero, _ := m.Update(tea.WindowSizeMsg{Width: 0, Height: 0})
	zeroView := zero.(*Model).View()
	if zeroView.Content != "" {
		t.Fatalf("0x0 content = %q, want empty frame", zeroView.Content)
	}
	if strings.Contains(ansiPattern.ReplaceAllString(zeroView.Content, ""), "T") {
		t.Fatal("0x0 must not render fitBlock's single T cell")
	}
}

func TestStartupMsgRunsCallbackOnce(t *testing.T) {
	m := New(controller.New(), nil)

	cmd := m.Init()
	if cmd == nil {
		t.Fatal("Init() = nil, want a StartupMsg command")
	}
	if msg := cmd(); msg != (StartupMsg{}) {
		t.Fatalf("Init() command message = %#v, want StartupMsg{}", msg)
	}

	calls := 0
	m.SetOnStartup(func() { calls++ })
	updated, next := m.Update(StartupMsg{})
	if next != nil {
		t.Fatalf("Update(StartupMsg) cmd = %v, want nil", next)
	}
	if updated.(*Model) != m {
		t.Fatal("Update(StartupMsg) returned a different model")
	}
	if calls != 1 {
		t.Fatalf("startup callback calls = %d, want 1", calls)
	}
}
