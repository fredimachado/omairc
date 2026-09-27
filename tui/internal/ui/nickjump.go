package ui

import (
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
)

// This file is the Ctrl+Shift+K nick jump overlay: it lists the channel's
// members in panel order (already PREFIX-rank then nick from the controller),
// filters by nick substring, and opens or creates the highlighted member's
// direct message. It is a no-op on a direct message or Status.

// nickJumpState is the Ctrl+Shift+K filter overlay.
type nickJumpState struct {
	open     bool
	input    textinput.Model
	selected int
}

const nickPlaceholder = "Nick"

func newNickJumpState() nickJumpState {
	return nickJumpState{input: newTextInput(defaultStyles(), nickPlaceholder, overlayFilterPrompt)}
}

// nickVisible reports whether the nick jump overlay is open.
func (m *Model) nickVisible() bool { return m != nil && m.nick.open }

// openNickJump opens the overlay on a channel. It is a no-op on a direct
// message or Status, matching the chord's gate.
func (m *Model) openNickJump() {
	if m.ctrl == nil || !m.ctrl.IsChannel() || m.ctrl.ConsoleOpen() {
		return
	}
	m.closeAllOverlays()
	m.nick.open = true
	m.nick.input = newTextInput(m.styles, nickPlaceholder, overlayFilterPrompt)
	m.nick.selected = 0
	m.nick.input.SetWidth(m.overlayInputWidth())
	m.composer.Blur()
	_ = m.nick.input.Focus()
}

// closeNickJump dismisses the overlay and returns focus to the composer.
func (m *Model) closeNickJump() {
	if !m.nick.open {
		return
	}
	m.nick.open = false
	m.nick.input.Blur()
	_ = m.composer.Focus()
	m.loadDraft()
}

// nickEntries is the filtered member list: a nick substring filter, in member
// panel order.
func (m *Model) nickEntries() []string {
	if m.ctrl == nil {
		return nil
	}
	query := strings.ToLower(strings.TrimSpace(m.nick.input.Value()))
	members := m.ctrl.Members()
	nicks := make([]string, 0, len(members))
	for _, member := range members {
		if member.Nick == "" {
			continue
		}
		if query != "" && !strings.Contains(strings.ToLower(member.Nick), query) {
			continue
		}
		nicks = append(nicks, member.Nick)
	}
	return nicks
}

// moveNick moves the highlighted nick by delta, wrapping at both ends.
func (m *Model) moveNick(delta int) {
	entries := m.nickEntries()
	if len(entries) == 0 {
		m.nick.selected = 0
		return
	}
	m.nick.selected = ((m.nick.selected+delta)%len(entries) + len(entries)) % len(entries)
}

// activateNick opens or creates a direct message with the highlighted member.
// Entering on your own nick is a no-op.
func (m *Model) activateNick() {
	if m.ctrl == nil {
		return
	}
	entries := m.nickEntries()
	if len(entries) == 0 {
		return
	}
	index := m.nick.selected
	if index < 0 || index >= len(entries) {
		index = 0
	}
	nick := entries[index]
	if nick == "" || nick == m.ctrl.CurrentNick() {
		return
	}
	m.saveDraft()
	m.nick.open = false
	m.nick.input.Blur()
	previousID := m.selectedConversationID()
	m.ctrl.OpenDirectMessage(nick)
	m.loadDraft()
	m.afterSelectionChange(previousID)
}

// handleNickKey folds one key while the overlay is open.
func (m *Model) handleNickKey(key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	switch key {
	case "esc", "escape":
		m.closeNickJump()
		return m, nil
	case "up":
		m.moveNick(-1)
		return m, nil
	case "down":
		m.moveNick(1)
		return m, nil
	case "enter", "return":
		m.activateNick()
		return m, nil
	}
	var cmd tea.Cmd
	m.nick.input, cmd = m.nick.input.Update(msg)
	m.nick.selected = 0
	return m, cmd
}

// nickCard renders the nick overlay as one card block through the shared
// overlay frame in model.go.
func (m *Model) nickCard(width int) string {
	return m.overlayCardBlock(width, m.nickCardBody)
}

// nickCardBody is the nick overlay's content, before the shared frame. inner is
// the content width inside the border.
func (m *Model) nickCardBody(inner int) []string {
	entries := m.nickEntries()
	lines := m.overlaySheetHeader(inner, "Nick", len(entries))
	lines = append(lines, truncateLine(m.nick.input.View(), inner))
	if len(entries) == 0 {
		return append(lines, m.overlaySheetEmpty("No matches"))
	}
	for index, nick := range entries {
		lines = append(lines, m.overlayToggleRow(inner, index == m.nick.selected, nick))
	}
	return lines
}
