package ui

import (
	tea "charm.land/bubbletea/v2"
)

// This file is the Ctrl+Shift+A session inbox sheet. It mirrors the Qt
// IrcInboxModel-backed sheet: it opens, walks, activates, and dismisses the
// controller's waiting list. The rows come from the controller snapshots as
// plain labels; it is a no-op while Connect is visible.

// inboxState is the inbox sheet. The selected row index is clamped to the
// waiting list so the highlight always names a real row.
type inboxState struct {
	selected int
	scroll   int
}

// inboxVisible reports whether the inbox sheet is open.
func (m *Model) inboxVisible() bool { return m.inboxSheet() != nil }

// toggleInbox opens or closes the inbox sheet.
func (m *Model) toggleInbox() {
	if m.inboxSheet() != nil {
		m.closeInbox()
		return
	}
	if m.ctrl == nil {
		return
	}
	m.setOverlay(&inboxState{})
	m.composer.Blur()
}

// closeInbox dismisses the sheet and returns focus to the composer.
func (m *Model) closeInbox() {
	if m.inboxSheet() == nil {
		return
	}
	m.clearOverlay()
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

// activateInboxSelection consumes the highlighted row and lands like the Qt
// sheet: a mention, highlight, or direct keeps its own destination (the unread
// mark, or the msgid when open-at-unread is off) instead of a saved scroll
// place. The conversation being left is remembered first.
func (m *Model) activateInboxSelection() {
	sheet := m.inboxSheet()
	if m.ctrl == nil || sheet == nil {
		return
	}
	items := m.ctrl.InboxItems()
	index := sheet.selected
	if index < 0 || index >= len(items) {
		return
	}
	item := items[index]
	previousID, wasConsole := m.beginTranscriptSwitch()
	m.saveDraft()
	m.ctrl.ActivateInboxItem(index)
	m.loadDraft()
	m.afterSelectionChange(previousID, wasConsole)
	if m.ctrl.ConsoleOpen() {
		return
	}
	same := previousID != "" && previousID == m.ctrl.SelectedConversationID()
	scrollKind := item.Kind == "mention" || item.Kind == "highlight" || item.Kind == "direct"
	if !scrollKind {
		return
	}
	// No msgid keeps the afterSelectionChange landing, including a saved
	// place. Qt only takes the special landing when the msgid is non-empty.
	if item.MsgID == "" {
		return
	}
	if same && !m.ctrl.OpenAtUnread() {
		if row := m.msgidRow(item.MsgID); row >= 0 {
			m.revealTranscriptRow(row)
			m.rememberOpenTranscript()
		}
		return
	}
	if same {
		return
	}
	if m.ctrl.OpenAtUnread() {
		m.setTranscriptFollowEnd(true)
		m.transcriptScroll = 0
		m.placeTranscriptAfterSelect()
		m.rememberOpenTranscript()
		return
	}
	if row := m.msgidRow(item.MsgID); row >= 0 {
		m.revealTranscriptRow(row)
		m.rememberOpenTranscript()
		return
	}
	m.setTranscriptFollowEnd(true)
	m.transcriptScroll = 0
	m.rememberOpenTranscript()
}

// dismissInboxRow drops one waiting row and keeps the highlight on the same
// logical row: a dismissal above the selection shifts it up. It mirrors
// inboxDismissAboveSelectionKeepsHighlight.
func (m *Model) dismissInboxRow(index int) {
	sheet := m.inboxSheet()
	if m.ctrl == nil || sheet == nil {
		return
	}
	before := len(m.inboxEntries())
	m.ctrl.DismissInboxItem(index)
	after := len(m.inboxEntries())
	if after == before {
		return
	}
	if index < sheet.selected {
		sheet.selected--
	}
	m.clampInboxSelection()
}

// clampInboxSelection keeps the highlight inside the waiting list: 0 when the
// list is empty, else clamped to [0, count-1].
func (m *Model) clampInboxSelection() {
	sheet := m.inboxSheet()
	if sheet == nil {
		return
	}
	count := len(m.inboxEntries())
	if count == 0 {
		sheet.selected = 0
		return
	}
	if sheet.selected < 0 {
		sheet.selected = 0
	}
	if sheet.selected >= count {
		sheet.selected = count - 1
	}
}

// moveInbox moves the highlighted row by delta, wrapping at both ends.
func (m *Model) moveInbox(delta int) {
	sheet := m.inboxSheet()
	if sheet == nil {
		return
	}
	count := len(m.inboxEntries())
	if count == 0 {
		sheet.selected = 0
		return
	}
	sheet.selected = ((sheet.selected+delta)%count + count) % count
	m.revealList(&sheet.scroll, sheet.selected, count, 0)
}

// handleInboxKey folds one key while the sheet is open. Ctrl+Shift+A toggles
// the sheet closed, Up/Down walk rows, Enter activates the highlighted row and
// closes, Delete dismisses it and keeps the sheet open, and Escape closes.
func (m *Model) handleInboxKey(key string, _ tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	sheet := m.inboxSheet()
	if sheet == nil {
		return m, nil
	}
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
		m.activateInboxSelection()
		m.closeInbox()
		return m, nil
	case "delete":
		m.dismissInboxRow(sheet.selected)
		return m, nil
	}
	return m, nil
}

// inboxCard renders the inbox sheet as one card block through the shared
// overlay frame in model.go. The name and match count sit on the top edge and
// the highlighted row's position sits on the bottom edge.
func (m *Model) inboxCard(width int) string {
	sheet := m.inboxSheet()
	if sheet == nil {
		return ""
	}
	entries := m.inboxEntries()
	return m.overlayCardFramed(width,
		overlayCountTitle("Inbox", len(entries)),
		overlayCountLabel(sheet.selected, len(entries)),
		m.inboxCardBody)
}

// inboxCardBody is the inbox sheet's content, before the shared frame. inner is
// the content width inside the border.
func (m *Model) inboxCardBody(inner int) []string {
	return overlayTexts(m.inboxOverlayLines(inner))
}

// inboxOverlayLines is the inbox card body. There is no filter line, so the
// first content line is a row.
func (m *Model) inboxOverlayLines(inner int) []overlayLine {
	sheet := m.inboxSheet()
	if sheet == nil {
		return nil
	}
	entries := m.inboxEntries()
	if len(entries) == 0 {
		return []overlayLine{{text: m.overlaySheetEmpty("No mentions"), row: -1}}
	}
	start, end := listWindow(len(entries), sheet.scroll, m.overlayRowBudget(0))
	lines := make([]overlayLine, 0, end-start)
	for index := start; index < end; index++ {
		lines = append(lines, overlayLine{
			text: m.overlayToggleRow(inner, index == sheet.selected, entries[index]),
			row:  index,
		})
	}
	return lines
}
