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

func newNickJumpState() nickJumpState {
	input := textinput.New()
	input.Placeholder = "Nick"
	input.Prompt = "› "
	return nickJumpState{input: input}
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
	m.nick.input.SetValue("")
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
	m.ctrl.OpenDirectMessage(nick)
	m.loadDraft()
	m.afterSelectionChange()
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

// nickCardLines renders the overlay into a bordered block exactly width cells
// wide.
func (m *Model) nickCardLines(width int) []string {
	inner := width - 4
	if inner < 8 {
		inner = 8
	}
	lines := []string{m.styles.JumpQuery.Render("Nick") + "  " + m.nick.input.View()}
	entries := m.nickEntries()
	if len(entries) == 0 {
		lines = append(lines, m.styles.JumpEmpty.Render("No matches"))
	} else {
		for index, nick := range entries {
			style := m.styles.JumpRow
			if index == m.nick.selected {
				style = m.styles.JumpSelected
			}
			lines = append(lines, style.Render(truncateLine(nick, inner)))
		}
	}
	rendered := m.styles.JumpCard.Width(inner).Render(strings.Join(lines, "\n"))
	return strings.Split(rendered, "\n")
}
