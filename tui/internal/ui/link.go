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
	open     bool
	input    textinput.Model
	selected int
}

const linkPlaceholder = "Filter links"

func newLinkState() linkState {
	return linkState{input: newTextInput(defaultStyles(), linkPlaceholder, overlayFilterPrompt)}
}

// linkVisible reports whether the link sheet is open.
func (m *Model) linkVisible() bool { return m != nil && m.link.open }

// toggleLink opens or closes the link sheet. It is only reached while Connect
// is closed; while the sheet is open the overlay owns the keys, so Alt+Down
// and Ctrl+/ never run.
func (m *Model) toggleLink() {
	if m.link.open {
		m.closeLink()
		return
	}
	if m.ctrl == nil {
		return
	}
	m.closeAllOverlays()
	m.link.open = true
	m.link.input = newTextInput(m.styles, linkPlaceholder, overlayFilterPrompt)
	m.link.selected = 0
	m.link.input.SetWidth(m.overlayInputWidth())
	m.composer.Blur()
	_ = m.link.input.Focus()
}

// closeLink dismisses the sheet and returns focus to the composer without
// changing the draft.
func (m *Model) closeLink() {
	if !m.link.open {
		return
	}
	m.link.open = false
	m.link.input.Blur()
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
	matches := m.linkMatches()
	if len(matches) == 0 {
		m.link.selected = 0
		return
	}
	m.link.selected = ((m.link.selected+delta)%len(matches) + len(matches)) % len(matches)
	m.revealTranscriptRow(matches[m.link.selected].row)
}

// activateLink opens the selected URL or joins the selected invite channel,
// then dismisses the sheet. A URL row that is not allowed opens nothing.
func (m *Model) activateLink() {
	matches := m.linkMatches()
	if len(matches) == 0 {
		m.closeLink()
		return
	}
	index := m.link.selected
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
	var cmd tea.Cmd
	m.link.input, cmd = m.link.input.Update(msg)
	m.link.selected = 0
	return m, cmd
}

// linkCard renders the link sheet as one card block through the shared overlay
// frame in model.go.
func (m *Model) linkCard(width int) string {
	return m.overlayCardBlock(width, m.linkCardBody)
}

// linkCardBody is the link sheet's content, before the shared frame. inner is
// the content width inside the border.
func (m *Model) linkCardBody(inner int) []string {
	matches := m.linkMatches()
	lines := m.overlaySheetHeader(inner, "Links", len(matches))
	lines = append(lines, truncateLine(m.link.input.View(), inner))
	if len(matches) == 0 {
		return append(lines, m.overlaySheetEmpty("No links"))
	}
	for index, entry := range matches {
		lines = append(lines, m.overlayToggleRow(inner, index == m.link.selected, entry.label))
	}
	return lines
}
