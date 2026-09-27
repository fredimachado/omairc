package ui

// This file is the Phase 7 parity suite for the Bubble Tea shell: the slash
// completion session, its Tab/Escape/Up/Down key semantics, and the composer
// behavior of /close, /query, /join, and /list. It reuses the demo-seeded
// harness in phase5_test.go.

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// TestSlashCompleteOpensForSlashJ pins that a bare "/j" opens the completion
// list with /join first and that the rendered view shows it.
func TestSlashCompleteOpensForSlashJ(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("/j")
	m.syncSlash()

	if !m.slash.open() {
		t.Fatalf("slash list closed for /j")
	}
	if len(m.slash.probe.Hits) == 0 || m.slash.probe.Hits[0].Label != "/join" {
		t.Fatalf("first hit = %+v, want /join", m.slash.probe.Hits)
	}
	if len(m.slashLines()) == 0 {
		t.Fatalf("slashLines() is empty for an open list")
	}
	if !strings.Contains(m.View().Content, "/join") {
		t.Fatalf("View().Content missing /join:\n%s", m.View().Content)
	}
}

// TestSlashCompleteTabInsertsAndCloses pins that Tab accepts the highlighted
// row and closes the list.
func TestSlashCompleteTabInsertsAndCloses(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("/j")
	m.syncSlash()

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
	if got := m.composer.Value(); !strings.HasPrefix(got, "/join ") {
		t.Fatalf("composer = %q, want a /join insertion", got)
	}
	if m.slash.open() {
		t.Fatalf("slash list stayed open after Tab")
	}
}

// TestSlashCompleteEscapeDismisses pins that Escape closes the list while
// keeping the typed text, and that a second Escape changes nothing.
func TestSlashCompleteEscapeDismisses(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("/j")
	m.syncSlash()
	title := m.View().WindowTitle

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.slash.open() {
		t.Fatalf("slash list stayed open after Escape")
	}
	if got := m.composer.Value(); got != "/j" {
		t.Fatalf("composer = %q after Escape, want /j", got)
	}

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if got := m.View().WindowTitle; got != title {
		t.Fatalf("second Escape changed the title: %q -> %q", title, got)
	}
	if m.ctrl.ConsoleOpen() {
		t.Fatalf("second Escape opened the Status surface")
	}
}

// TestSlashCompleteUpDownWraps pins the wrapping selection arithmetic.
func TestSlashCompleteUpDownWraps(t *testing.T) {
	m := seededModel(t)
	m.slash.sync(controller.SlashProbe{
		Open:   true,
		Needle: "j",
		Hits: []controller.SlashHit{
			{Label: "/join"},
			{Label: "/nick"},
		},
	}, "/j")

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyUp})
	if m.slash.selected != 1 {
		t.Fatalf("selected = %d after Up, want 1 (wrapped)", m.slash.selected)
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyDown})
	if m.slash.selected != 0 {
		t.Fatalf("selected = %d after Down, want 0 (wrapped)", m.slash.selected)
	}
}

// TestSlashCompleteHistoryKeepsListClosed pins that browsing history keeps the
// completion list closed even when the recalled line is a bare slash token.
func TestSlashCompleteHistoryKeepsListClosed(t *testing.T) {
	m := seededModel(t)
	m.composerHistory = []string{"/j"}

	m.recallHistory(-1)
	m.syncSlash()

	if m.slash.open() {
		t.Fatalf("slash list opened while browsing history")
	}
	if got := m.composer.Value(); got != "/j" {
		t.Fatalf("composer = %q, want the recalled /j", got)
	}
}

// TestCloseRefusedStaysInComposer pins that a refused /close keeps its text and
// leaves the selection alone.
func TestCloseRefusedStaysInComposer(t *testing.T) {
	m := seededModel(t)
	title := m.View().WindowTitle

	m.composer.SetValue("/close")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})

	if got := m.composer.Value(); got != "/close" {
		t.Fatalf("composer = %q after refused /close, want /close", got)
	}
	if got := m.View().WindowTitle; got != title {
		t.Fatalf("title = %q after refused /close, want %q", got, title)
	}
}

// TestQueryOpensDirectAndClearsComposer pins that /query selects the direct and
// the shell consumes the composer.
func TestQueryOpensDirectAndClearsComposer(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("/query dax")

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})

	if got := m.composer.Value(); got != "" {
		t.Fatalf("composer = %q after /query, want empty", got)
	}
	if got := m.ctrl.SelectedTarget(); got != "dax" {
		t.Fatalf("SelectedTarget = %q after /query, want dax", got)
	}
}

// TestJoinOpensChannelAndConsumesComposer pins that /join selects the joined
// channel and clears the composer.
func TestJoinOpensChannelAndConsumesComposer(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("/join #help")

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})

	if got := m.composer.Value(); got != "" {
		t.Fatalf("composer = %q after /join, want empty", got)
	}
	if got := m.View().WindowTitle; got != "#help - Omairc" {
		t.Fatalf("title = %q after /join #help, want %q", got, "#help - Omairc")
	}
}

// TestListOpensOverlayAndJoins pins the demo /list round trip: the answer opens
// the overlay, and Enter on the highlighted row joins that channel.
func TestListOpensOverlayAndJoins(t *testing.T) {
	m := seededModel(t)
	m.composer.Reset()
	m.composer.SetValue("/list")

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if !m.list.open {
		t.Fatalf("list overlay not open after /list")
	}
	snapshot := m.ctrl.ChannelListSnapshot()
	if !snapshot.Open || len(snapshot.Rows) == 0 {
		t.Fatalf("channel-list snapshot = %+v, want open with rows", snapshot)
	}
	if snapshot.Rows[0].Channel != "#linux" {
		t.Fatalf("first /list row = %q, want #linux", snapshot.Rows[0].Channel)
	}

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.list.open {
		t.Fatalf("list overlay stayed open after Enter")
	}
	if !m.ctrl.IsChannel() {
		t.Fatalf("SelectedTarget = %q after joining a /list row, not a channel",
			m.ctrl.SelectedTarget())
	}
	if got := m.ctrl.SelectedTarget(); got != "#linux" {
		t.Fatalf("SelectedTarget = %q after Enter on /list, want #linux", got)
	}
}
