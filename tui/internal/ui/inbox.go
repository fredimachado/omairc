package ui

import (
	"strings"

	tea "charm.land/bubbletea/v2"
)

// This file is the Ctrl+Shift+A session inbox sheet. The InboxStore is a Phase
// 9 seam: the sheet opens, walks, and closes and the chord is real, but the row
// list is empty today. It is a no-op while Connect is visible.

// inboxState is the inbox sheet. The selected row index is kept even though
// the store is empty, so the row walk lands correctly once Phase 9 fills it.
type inboxState struct {
	open     bool
	selected int
}

func newInboxState() inboxState { return inboxState{} }

// inboxVisible reports whether the inbox sheet is open.
func (m *Model) inboxVisible() bool { return m != nil && m.inbox.open }

// toggleInbox opens or closes the inbox sheet.
func (m *Model) toggleInbox() {
	if m.inbox.open {
		m.closeInbox()
		return
	}
	if m.ctrl == nil {
		return
	}
	m.closeAllOverlays()
	m.inbox.open = true
	m.inbox.selected = 0
	m.composer.Blur()
}

// closeInbox dismisses the sheet and returns focus to the composer.
func (m *Model) closeInbox() {
	if !m.inbox.open {
		return
	}
	m.inbox.open = false
	_ = m.composer.Focus()
	m.loadDraft()
}

// inboxEntries is the Phase 9 InboxStore seam. It is empty today.
func (m *Model) inboxEntries() []string {
	return nil
}

// moveInbox moves the highlighted row by delta, wrapping at both ends.
func (m *Model) moveInbox(delta int) {
	count := len(m.inboxEntries())
	if count == 0 {
		m.inbox.selected = 0
		return
	}
	m.inbox.selected = ((m.inbox.selected+delta)%count + count) % count
}

// handleInboxKey folds one key while the sheet is open. Up/Down walk rows,
// Enter activates, Delete dismisses, and Escape closes.
func (m *Model) handleInboxKey(key string, _ tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	switch key {
	case "esc", "escape":
		m.closeInbox()
		return m, nil
	case "up":
		m.moveInbox(-1)
		return m, nil
	case "down":
		m.moveInbox(1)
		return m, nil
	case "enter", "return":
		// Enter activates the selected arrival once the Phase 9 store lands.
		return m, nil
	case "delete":
		// Delete dismisses the selected arrival once the Phase 9 store lands.
		return m, nil
	}
	return m, nil
}

// inboxCardLines renders the sheet into a bordered block exactly width cells
// wide.
func (m *Model) inboxCardLines(width int) []string {
	inner := width - 4
	if inner < 8 {
		inner = 8
	}
	lines := []string{m.styles.JumpQuery.Render("Inbox")}
	entries := m.inboxEntries()
	if len(entries) == 0 {
		lines = append(lines, m.styles.JumpEmpty.Render("No mentions"))
	}
	for index, entry := range entries {
		style := m.styles.JumpRow
		if index == m.inbox.selected {
			style = m.styles.JumpSelected
		}
		lines = append(lines, style.Render(truncateLine(entry, inner)))
	}
	rendered := m.styles.JumpCard.Width(inner).Render(strings.Join(lines, "\n"))
	return strings.Split(rendered, "\n")
}
