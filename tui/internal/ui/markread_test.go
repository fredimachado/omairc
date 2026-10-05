package ui

import (
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
)

func altShiftA() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: 'a', Mod: tea.ModAlt | tea.ModShift}
}

func TestMarkAllReadClearsBadgesAndBackgroundTitle(t *testing.T) {
	m := seededModel(t)
	m.noteTitleMark(false, "alice", "hey fred", "omarchy", "#ricing")
	if got := m.View().WindowTitle; got != "alice: hey fred · #ricing - Omairc" {
		t.Fatalf("background title = %q, want the title mark", got)
	}
	if m.ctrl.UnreadCountFor("omarchy") == 0 {
		t.Fatal("the seeded omarchy network must start with unread")
	}
	before := m.ctrl.SelectedTarget()

	m = press(t, m, altShiftA())

	if got := m.ctrl.SelectedTarget(); got != before {
		t.Fatalf("mark all read moved the selection to %q", got)
	}
	if got := m.ctrl.UnreadCountFor("omarchy"); got != 0 {
		t.Fatalf("omarchy unread = %d, want 0", got)
	}
	if got := m.ctrl.UnreadCountFor("oftc"); got != 0 {
		t.Fatalf("oftc unread = %d, want 0", got)
	}
	if m.ctrl.MentionFor("omarchy") || m.ctrl.MentionFor("oftc") {
		t.Fatal("mentions must clear with the badges")
	}
	for _, row := range m.ctrl.Conversations() {
		if row.Unread != 0 || row.Mention {
			t.Fatalf("row %s/%s still unread=%d mention=%v",
				row.NetworkID, row.Conversation, row.Unread, row.Mention)
		}
	}
	if m.titleMark != nil {
		t.Fatal("mark all read must clear the attention title")
	}
	if got := m.View().WindowTitle; got != "#omarchy · irc.example · fred - Omairc" {
		t.Fatalf("title after mark all read = %q", got)
	}
}

func TestUnreadLandsOnStatusWhenNothingIsUnread(t *testing.T) {
	m := seededModel(t)
	m.switchSelection(func() {
		m.ctrl.SelectConversation("oftc", "#lab")
	})
	if got := m.ctrl.SelectedTarget(); got != "#lab" {
		t.Fatalf("selection = %q, want #lab", got)
	}

	m = press(t, m, altShiftA())
	m = press(t, m, tea.KeyPressMsg{Code: 'a', Mod: tea.ModAlt})

	if !m.ctrl.ConsoleOpen() {
		t.Fatal("Alt+A must open Status when nothing is unread")
	}
	if got := m.View().WindowTitle; got != "irc.example · oak Status" {
		t.Fatalf("status title = %q, want oftc Status", got)
	}
	if got := m.ctrl.SelectedTarget(); got != "#lab" {
		t.Fatalf("Status replaced the conversation with %q", got)
	}
}

func TestShortcutsSheetBlocksMarkAllRead(t *testing.T) {
	m := seededModel(t)
	before := m.ctrl.UnreadCountFor("omarchy")
	m = press(t, m, tea.KeyPressMsg{Code: '/', Mod: tea.ModCtrl})
	if !m.shortcutsOpen {
		t.Fatal("Ctrl+/ must open the shortcuts sheet")
	}

	m = press(t, m, altShiftA())

	if got := m.ctrl.UnreadCountFor("omarchy"); got != before {
		t.Fatalf("shortcuts sheet let mark all read change unread from %d to %d", before, got)
	}
}

func TestConnectBlocksMarkAllRead(t *testing.T) {
	ctrl := controller.New()
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	conn := connection.New(ctrl, nil)
	conn.SetStoredProfiles([]connection.NetworkProfile{
		storedProfile("omarchy", "irc.example"),
	})
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)
	if m.connectVisible() {
		t.Fatal("a stored profile must leave Connect closed")
	}
	before := ctrl.UnreadCountFor("omarchy")
	if before == 0 {
		t.Fatal("omarchy must start unread")
	}

	m = press(t, m, tea.KeyPressMsg{Code: ',', Mod: tea.ModCtrl})
	if !m.connectVisible() {
		t.Fatal("Ctrl+, must open Connect")
	}
	m = press(t, m, altShiftA())

	if got := ctrl.UnreadCountFor("omarchy"); got != before {
		t.Fatalf("Connect let mark all read change unread from %d to %d", before, got)
	}
	if !m.connectVisible() {
		t.Fatal("Connect must stay open")
	}
}
