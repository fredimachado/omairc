package ui

import (
	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
)

// This file is the Ctrl+Shift+O link sheet. The URL extraction and the
// allowlisted open path live in urllinks.go; the sheet lists http/https URLs
// from the current transcript (conversation or Status), newest row first and
// last-in-text first within a row. It is gated while Connect is visible and
// stays enabled while it is already open so the chord can toggle it closed.

// linkState is the link sheet. The filter input narrows the matches and
// selected is the highlighted row.
type linkState struct {
	input    textinput.Model
	selected int
	scroll   int
}

const linkPlaceholder = "Filter links"

// linkVisible reports whether the link sheet is open.
func (m *Model) linkVisible() bool { return m.linkSheet() != nil }

// toggleLink opens or closes the link sheet. It is only reached while Connect
// is closed; while the sheet is open the overlay owns the keys, so Alt+Down
// and Ctrl+/ never run.
func (m *Model) toggleLink() {
	if m.linkSheet() != nil {
		m.closeLink()
		return
	}
	if m.ctrl == nil {
		return
	}
	sheet := &linkState{input: newTextInput(m.styles, linkPlaceholder, overlayFilterPrompt)}
	sheet.input.SetWidth(m.overlayInputWidth())
	m.setOverlay(sheet)
	m.composer.Blur()
	_ = sheet.input.Focus()
}

// closeLink dismisses the sheet and returns focus to the composer without
// changing the draft.
func (m *Model) closeLink() {
	if m.linkSheet() == nil {
		return
	}
	m.clearOverlay()
	if m.channelPrompt.open {
		m.composer.Blur()
		return
	}
	_ = m.composer.Focus()
	m.loadDraft()
}

// moveLink moves the highlight by delta, wrapping at both ends, and reveals
// the source row so the match stays visible.
func (m *Model) moveLink(delta int) {
	sheet := m.linkSheet()
	if sheet == nil {
		return
	}
	matches := m.linkMatches()
	if len(matches) == 0 {
		sheet.selected = 0
		return
	}
	sheet.selected = ((sheet.selected+delta)%len(matches) + len(matches)) % len(matches)
	m.revealList(&sheet.scroll, sheet.selected, len(matches), 1)
	m.revealTranscriptRow(matches[sheet.selected].row)
}

// activateLink opens the selected URL or joins the selected invite channel
// then dismisses the sheet. A URL row that is not allowed opens nothing.
func (m *Model) activateLink() {
	sheet := m.linkSheet()
	if sheet == nil {
		return
	}
	matches := m.linkMatches()
	if len(matches) == 0 {
		m.closeLink()
		return
	}
	index := sheet.selected
	if index < 0 || index >= len(matches) {
		index = 0
	}
	entry := matches[index]
	switch entry.kind {
	case linkKindInvite:
		if m.ctrl != nil {
			m.ctrl.ConsoleSubmit("/join " + entry.value)
		}
	case linkKindChannel:
		m.openChannelName(entry.value)
	default:
		m.openAllowedURL(entry.value)
	}
	m.closeLink()
}

// handleLinkKey folds one key while the sheet is open. Escape and the toggle
// chord close it; Up/Down move and reveal; Enter activates; everything else
// filters the list.
func (m *Model) handleLinkKey(key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	sheet := m.linkSheet()
	if sheet == nil {
		return m, nil
	}
	switch key {
	case "esc", "escape", "ctrl+shift+o":
		m.closeLink()
		return m, nil
	case "up":
		m.moveLink(-1)
		return m, nil
	case "down":
		m.moveLink(1)
		return m, nil
	case "enter", "return":
		m.activateLink()
		return m, nil
	}
	cmd := sheet.updateInput(msg)
	sheet.selected = 0
	sheet.scroll = 0
	return m, cmd
}

// linkCard renders the link sheet as one card block through the shared overlay
// frame in model.go. The name and match count sit on the top edge and the
// highlighted row's position sits on the bottom edge.
func (m *Model) linkCard(width int) string {
	sheet := m.linkSheet()
	if sheet == nil {
		return ""
	}
	matches := m.linkMatches()
	return m.overlayCardFramed(width,
		overlayCountTitle("Links", len(matches)),
		overlayCountLabel(sheet.selected, len(matches)),
		m.linkCardBody)
}

// linkCardBody is the link sheet's content, before the shared frame. inner is
// the content width inside the border.
func (m *Model) linkCardBody(inner int) []string {
	return overlayTexts(m.linkOverlayLines(inner))
}

// linkOverlayLines is the link card body. The filter is not a row.
func (m *Model) linkOverlayLines(inner int) []overlayLine {
	sheet := m.linkSheet()
	if sheet == nil {
		return nil
	}
	matches := m.linkMatches()
	lines := []overlayLine{{text: truncateLine(sheet.input.View(), inner), row: -1}}
	if len(matches) == 0 {
		return append(lines, overlayLine{text: m.overlaySheetEmpty("No links"), row: -1})
	}
	start, end := listWindow(len(matches), sheet.scroll, m.overlayRowBudget(1))
	for index := start; index < end; index++ {
		lines = append(lines, overlayLine{
			text: m.overlayToggleRow(inner, index == sheet.selected, matches[index].label),
			row:  index,
		})
	}
	return lines
}
