package ui

import (
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
)

// This file holds the phase-5 conversation navigation: the walk and unread
// chords, the Ctrl+K jump overlay, the Status toggle, and the per-conversation
// composer drafts. It reads only controller snapshots, never internal/irc.

// Jump entry kinds.
const (
	jumpKindConversation = iota
	jumpKindStatus
)

// jumpEntry is one row of the jump overlay: a conversation or a network's
// Status surface.
type jumpEntry struct {
	kind           int
	label          string
	networkID      string
	target         string
	conversationID string
}

// jumpState is the Ctrl+K filter overlay.
type jumpState struct {
	open     bool
	input    textinput.Model
	selected int
}

func newJumpState() jumpState {
	input := textinput.New()
	input.Placeholder = "Jump to"
	input.Prompt = "› "
	return jumpState{input: input}
}

// jumpVisible reports whether the jump overlay is open.
func (m *Model) jumpVisible() bool { return m != nil && m.jump.open }

// openJump opens the jump overlay with an empty query and the composer
// blurred, so typed keys reach the filter.
func (m *Model) openJump() {
	if m.ctrl == nil {
		return
	}
	m.jump.open = true
	m.jump.input.SetValue("")
	m.jump.selected = 0
	m.jump.input.SetWidth(m.overlayInputWidth())
	m.composer.Blur()
	_ = m.jump.input.Focus()
}

// closeJump dismisses the overlay and returns focus to the composer.
func (m *Model) closeJump() {
	if !m.jump.open {
		return
	}
	m.jump.open = false
	m.jump.input.Blur()
	_ = m.composer.Focus()
	m.loadDraft()
}

// jumpEntries builds the filtered overlay rows: every network's conversations
// in sidebar order, then that network's Status row, matching
// OmaircWindow.qml's refreshJumpMatches.
func (m *Model) jumpEntries() []jumpEntry {
	if m.ctrl == nil {
		return nil
	}
	query := strings.ToLower(strings.TrimSpace(m.jump.input.Value()))
	conversations := m.ctrl.Conversations()
	var entries []jumpEntry
	for _, networkID := range m.ctrl.NetworkOrder() {
		networkName := m.ctrl.NetworkDisplayName(networkID)
		for _, row := range conversations {
			if row.NetworkID != networkID {
				continue
			}
			label := m.jumpConversationLabel(row.ConversationName, networkName)
			if !jumpMatches(query, label) {
				continue
			}
			entries = append(entries, jumpEntry{
				kind:           jumpKindConversation,
				label:          label,
				networkID:      networkID,
				target:         row.ConversationName,
				conversationID: row.ConversationID,
			})
		}
		statusLabel := statusJumpLabel(networkName)
		if jumpMatches(query, statusLabel) {
			entries = append(entries, jumpEntry{
				kind:      jumpKindStatus,
				label:     statusLabel,
				networkID: networkID,
			})
		}
	}
	return entries
}

// jumpConversationLabel mirrors OmaircWindow.qml's jumpTargetLabel for a
// conversation: a duplicate channel name carries its network display name.
func (m *Model) jumpConversationLabel(name, networkName string) string {
	if name == "" {
		return ""
	}
	if duplicateTargetName(m.ctrl, name) && networkName != "" {
		return name + " · " + networkName
	}
	return name
}

// jumpMatches reports whether the lower-cased query is a substring of the
// label.
func jumpMatches(query, label string) bool {
	if query == "" {
		return true
	}
	return strings.Contains(strings.ToLower(label), query)
}

// moveJump moves the highlighted row by delta, wrapping at both ends.
func (m *Model) moveJump(delta int) {
	entries := m.jumpEntries()
	if len(entries) == 0 {
		m.jump.selected = 0
		return
	}
	m.jump.selected = ((m.jump.selected+delta)%len(entries) + len(entries)) % len(entries)
}

// activateJump opens the highlighted entry and dismisses the overlay.
func (m *Model) activateJump() {
	entries := m.jumpEntries()
	if len(entries) == 0 {
		return
	}
	index := m.jump.selected
	if index < 0 || index >= len(entries) {
		index = 0
	}
	entry := entries[index]
	m.switchSelection(func() {
		if entry.kind == jumpKindStatus {
			m.ctrl.OpenStatus(entry.networkID)
			return
		}
		m.ctrl.SelectConversation(entry.networkID, entry.target)
	})
	m.closeJump()
}

// handleJumpKey folds one key while the overlay is open. Escape dismisses,
// Up/Down highlight, Enter activates, and everything else filters.
func (m *Model) handleJumpKey(key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	switch key {
	case "esc", "escape":
		m.closeJump()
		return m, nil
	case "up":
		m.moveJump(-1)
		return m, nil
	case "down":
		m.moveJump(1)
		return m, nil
	case "enter":
		m.activateJump()
		return m, nil
	}
	var cmd tea.Cmd
	m.jump.input, cmd = m.jump.input.Update(msg)
	m.jump.selected = 0
	return m, cmd
}

// walk moves to the next or previous visible conversation over the sidebar
// order, wrapping. Status is not in the walk. It mirrors
// OmaircWindow.qml's stepConversation.
func (m *Model) walk(delta int) {
	rows := m.ctrl.Conversations()
	if len(rows) == 0 {
		return
	}
	current := -1
	for index, row := range rows {
		if row.ConversationID == m.ctrl.SelectedConversationID() && row.ConversationID != "" {
			current = index
			break
		}
	}
	start := current
	if start < 0 {
		if delta > 0 {
			start = -1
		} else {
			start = len(rows)
		}
	}
	for step := 1; step <= len(rows); step++ {
		index := ((start+delta*step)%len(rows) + len(rows)) % len(rows)
		row := rows[index]
		m.switchSelection(func() {
			m.ctrl.SelectConversation(row.NetworkID, row.ConversationName)
		})
		return
	}
}

// jumpUnread selects the next unread conversation, mentions first, skipping
// muted rows while hunting a mention. It mirrors OmaircWindow.qml's
// jumpToNextUnread.
func (m *Model) jumpUnread() {
	rows := m.ctrl.Conversations()
	if len(rows) == 0 {
		return
	}
	current := -1
	for index, row := range rows {
		if row.ConversationID == m.ctrl.SelectedConversationID() && row.ConversationID != "" {
			current = index
			break
		}
	}
	start := 0
	if current >= 0 {
		start = (current + 1) % len(rows)
	}
	mention := -1
	unread := -1
	for step := 0; step < len(rows); step++ {
		index := (start + step) % len(rows)
		if index == current {
			continue
		}
		row := rows[index]
		if mention < 0 && row.Mention && !row.Muted {
			mention = index
			break
		}
		if unread < 0 && row.Unread > 0 {
			unread = index
		}
	}
	target := mention
	if target < 0 {
		target = unread
	}
	if target < 0 {
		return
	}
	row := rows[target]
	m.switchSelection(func() {
		m.ctrl.SelectConversation(row.NetworkID, row.ConversationName)
	})
}

// toggleStatus opens or closes the Status console. Opening remembers the
// conversation id so Escape can return to it. It mirrors the Ctrl+` shortcut in
// OmaircWindow.qml.
func (m *Model) toggleStatus() {
	if m.ctrl == nil {
		return
	}
	if m.ctrl.ConsoleOpen() {
		m.switchSelection(func() {
			if m.statusReturnID != "" && m.ctrl.SelectConversationByID(m.statusReturnID) {
				return
			}
			m.ctrl.ClearConversationSelection()
		})
		return
	}
	networkID := m.ctrl.FocusedNetworkID()
	if networkID == "" {
		networkID = m.ctrl.SelectedNetworkID()
	}
	if networkID == "" {
		return
	}
	m.statusReturnID = m.ctrl.SelectedConversationID()
	m.switchSelection(func() {
		m.ctrl.OpenStatus(networkID)
	})
}

// dismissOverlay is the Escape handler when no overlay owns the key. It closes
// Status when it is open and leaves everything else alone.
func (m *Model) dismissOverlay() {
	if m.ctrl != nil && m.ctrl.ConsoleOpen() {
		m.toggleStatus()
	}
}

// --- Per-conversation drafts ----------------------------------------------

// switchSelection stashes the current composer draft, runs apply (which changes
// the selection or Status surface), then restores the draft for the new key.
func (m *Model) switchSelection(apply func()) {
	m.saveDraft()
	apply()
	m.loadDraft()
}

// saveDraft records the composer text under the current conversation key.
func (m *Model) saveDraft() {
	if m.draftKey == "" {
		return
	}
	m.drafts[m.draftKey] = m.composer.Value()
}

// loadDraft restores the composer text for the current conversation key.
func (m *Model) loadDraft() {
	key := m.composerDraftKey()
	m.draftKey = key
	m.composer.SetValue(m.drafts[key])
	m.composer.CursorEnd()
}

// composerDraftKey is the stable key unsent text belongs to: the Status key for
// the focused network, else the selected conversation id. It never follows the
// user across targets. It mirrors OmaircWindow.qml's composerHistoryKey.
func (m *Model) composerDraftKey() string {
	if m.ctrl == nil {
		return ""
	}
	if m.ctrl.ConsoleOpen() {
		return "status\n" + m.ctrl.FocusedNetworkID()
	}
	return m.ctrl.SelectedConversationID()
}

// clearCurrentDraft drops the stored draft for the current key, used after the
// composer sends.
func (m *Model) clearCurrentDraft() {
	if m.draftKey == "" {
		return
	}
	m.drafts[m.draftKey] = ""
}

// jumpCardLines renders the jump overlay into a bordered block exactly width
// cells wide.
func (m *Model) jumpCardLines(width int) []string {
	inner := width - 4
	if inner < 8 {
		inner = 8
	}
	rendered := m.styles.JumpCard.Width(inner).Render(strings.Join(m.jumpCard(inner), "\n"))
	return strings.Split(rendered, "\n")
}

// jumpCard is the overlay's content, before the border.
func (m *Model) jumpCard(inner int) []string {
	lines := []string{m.styles.JumpQuery.Render("Jump") + "  " + m.jump.input.View()}
	entries := m.jumpEntries()
	if len(entries) == 0 {
		return append(lines, m.styles.JumpEmpty.Render("No matches"))
	}
	for index, entry := range entries {
		style := m.styles.JumpRow
		if index == m.jump.selected {
			style = m.styles.JumpSelected
		}
		lines = append(lines, style.Render(truncateLine(entry.label, inner)))
	}
	return lines
}

// overlayInputWidth is the width the composer, sheet, and jump filters use
// inside the current window.
func (m *Model) overlayInputWidth() int {
	width := m.width - 8
	if width < 1 {
		width = 1
	}
	return width
}
