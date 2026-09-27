package ui

import (
	"strings"

	tea "charm.land/bubbletea/v2"
)

// This file is the Ctrl+Shift+A session inbox sheet. It mirrors the Qt
// IrcInboxModel-backed sheet: it opens, walks, activates, and dismisses the
// controller's waiting list. The rows come from the controller snapshots as
// plain labels; it is a no-op while Connect is visible.

// inboxState is the inbox sheet. The selected row index is clamped to the
// waiting list so the highlight always names a real row.
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

// inboxEntries returns the waiting rows' labels, newest first, for the sheet.
// It reads the controller snapshot; a nil controller has no rows.
func (m *Model) inboxEntries() []string {
	if m.ctrl == nil {
		return nil
	}
	items := m.ctrl.InboxItems()
	entries := make([]string, 0, len(items))
	for _, item := range items {
		entries = append(entries, item.Label)
	}
	return entries
}

// dismissInboxRow drops one waiting row and keeps the highlight on the same
// logical row: a dismissal above the selection shifts it up. It mirrors
// inboxDismissAboveSelectionKeepsHighlight.
func (m *Model) dismissInboxRow(index int) {
	if m.ctrl == nil {
		return
	}
	before := len(m.inboxEntries())
	m.ctrl.DismissInboxItem(index)
	after := len(m.inboxEntries())
	if after == before {
		return
	}
	if index < m.inbox.selected {
		m.inbox.selected--
	}
	m.clampInboxSelection()
}

// clampInboxSelection keeps the highlight inside the waiting list: 0 when the
// list is empty, else clamped to [0, count-1].
func (m *Model) clampInboxSelection() {
	count := len(m.inboxEntries())
	if count == 0 {
		m.inbox.selected = 0
		return
	}
	if m.inbox.selected < 0 {
		m.inbox.selected = 0
	}
	if m.inbox.selected >= count {
		m.inbox.selected = count - 1
	}
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

// handleInboxKey folds one key while the sheet is open. Ctrl+Shift+A toggles
// the sheet closed, Up/Down walk rows, Enter activates the highlighted row and
// closes, Delete dismisses it and keeps the sheet open, and Escape closes.
func (m *Model) handleInboxKey(key string, _ tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	switch key {
	case "ctrl+shift+a":
		// The overlay owns every key while it is open, so a second chord must
		// be handled here rather than in the chord table.
		m.toggleInbox()
		return m, nil
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
		if m.ctrl != nil {
			m.ctrl.ActivateInboxItem(m.inbox.selected)
		}
		m.closeInbox()
		return m, nil
	case "delete":
		m.dismissInboxRow(m.inbox.selected)
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
