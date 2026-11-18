package ui

import (
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
)

// This file is the Ctrl+Shift+K nick jump overlay: it lists
// members in panel order (already PREFIX-rank then nick from the controller)
// filters by nick substring, and opens or creates the highlighted member's
// direct message. It is a no-op on a direct message or Status.

// nickJumpState is the Ctrl+Shift+K filter overlay.
type nickJumpState struct {
	input    textinput.Model
	selected int
	scroll   int
}

const nickPlaceholder = "Nick"

// nickVisible reports whether the nick jump overlay is open.
func (m *Model) nickVisible() bool { return m.nickSheet() != nil }

// openNickJump opens the overlay on a channel you are in. It is a no-op on a
// direct message, Status, or a channel you have left, matching the chord's gate.
func (m *Model) openNickJump() {
	if m.ctrl == nil || !m.ctrl.IsChannel() || !m.ctrl.ChannelJoined() || m.ctrl.ConsoleOpen() {
		return
	}
	sheet := &nickJumpState{input: newTextInput(m.styles, nickPlaceholder, overlayFilterPrompt)}
	sheet.input.SetWidth(m.overlayInputWidth())
	m.setOverlay(sheet)
	m.composer.Blur()
	_ = sheet.input.Focus()
}

// closeNickJump dismisses the overlay and returns focus to the composer.
func (m *Model) closeNickJump() {
	if m.nickSheet() == nil {
		return
	}
	m.clearOverlay()
	_ = m.composer.Focus()
	m.loadDraft()
}

// nickEntries is the filtered member list: a nick substring filter, in member
// panel order.
func (m *Model) nickEntries() []string {
	if m.ctrl == nil {
		return nil
	}
	query := ""
	if sheet := m.nickSheet(); sheet != nil {
		query = strings.ToLower(strings.TrimSpace(sheet.input.Value()))
	}
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
	sheet := m.nickSheet()
	if sheet == nil {
		return
	}
	entries := m.nickEntries()
	if len(entries) == 0 {
		sheet.selected = 0
		return
	}
	sheet.selected = ((sheet.selected+delta)%len(entries) + len(entries)) % len(entries)
	m.revealList(&sheet.scroll, sheet.selected, len(entries), 1)
}

// activateNick opens or creates a direct message with the highlighted member.
// Entering on your own nick is a no-op.
func (m *Model) activateNick() {
	sheet := m.nickSheet()
	if m.ctrl == nil || sheet == nil {
		return
	}
	entries := m.nickEntries()
	if len(entries) == 0 {
		return
	}
	index := sheet.selected
	if index < 0 || index >= len(entries) {
		index = 0
	}
	nick := entries[index]
	if nick == "" || nick == m.ctrl.CurrentNick() {
		return
	}
	m.saveDraft()
	m.clearOverlay()
	previousID, wasConsole := m.beginTranscriptSwitch()
	m.ctrl.OpenDirectMessage(nick)
	m.loadDraft()
	m.afterSelectionChange(previousID, wasConsole)
}

// handleNickKey folds one key while the overlay is open.
func (m *Model) handleNickKey(key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	sheet := m.nickSheet()
	if sheet == nil {
		return m, nil
	}
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
	cmd := sheet.updateInput(msg)
	sheet.selected = 0
	sheet.scroll = 0
	return m, cmd
}

// nickCard renders the nick overlay as one card block through the shared
// overlay frame in model.go. The name and match count sit on the top edge and
// the highlighted nick's position sits on the bottom edge.
func (m *Model) nickCard(width int) string {
	sheet := m.nickSheet()
	if sheet == nil {
		return ""
	}
	entries := m.nickEntries()
	return m.overlayCardFramed(width,
		overlayCountTitle("Nick", len(entries)),
		overlayCountLabel(sheet.selected, len(entries)),
		m.nickCardBody)
}

// nickCardBody is the nick overlay's content, before the shared frame. inner is
// the content width inside the border.
func (m *Model) nickCardBody(inner int) []string {
	return overlayTexts(m.nickOverlayLines(inner))
}

// nickOverlayLines is the nick card body. The filter is not a row.
func (m *Model) nickOverlayLines(inner int) []overlayLine {
	sheet := m.nickSheet()
	if sheet == nil {
		return nil
	}
	entries := m.nickEntries()
	lines := []overlayLine{{text: truncateLine(sheet.input.View(), inner), row: -1}}
	if len(entries) == 0 {
		return append(lines, overlayLine{text: m.overlaySheetEmpty("No matches"), row: -1})
	}
	start, end := listWindow(len(entries), sheet.scroll, m.overlayRowBudget(1))
	for index := start; index < end; index++ {
		lines = append(lines, overlayLine{
			text: m.overlayToggleRow(inner, index == sheet.selected, entries[index]),
			row:  index,
		})
	}
	return lines
}
