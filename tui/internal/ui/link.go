package ui

import (
	"strings"

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

func newLinkState() linkState {
	input := textinput.New()
	input.Placeholder = "Filter links"
	input.Prompt = "› "
	return linkState{input: input}
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
	m.link.selected = 0
	m.link.input.SetValue("")
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
	if entry.kind == linkKindInvite {
		if m.ctrl != nil {
			m.ctrl.ConsoleSubmit("/join " + entry.value)
		}
	} else {
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

// linkCardLines renders the sheet into a bordered block exactly width cells
// wide.
func (m *Model) linkCardLines(width int) []string {
	inner := width - 4
	if inner < 8 {
		inner = 8
	}
	lines := []string{m.styles.JumpQuery.Render("Links") + "  " + m.link.input.View()}
	matches := m.linkMatches()
	if len(matches) == 0 {
		lines = append(lines, m.styles.JumpEmpty.Render("No links"))
	}
	for index, entry := range matches {
		style := m.styles.JumpRow
		if index == m.link.selected {
			style = m.styles.JumpSelected
		}
		lines = append(lines, style.Render(truncateLine(entry.label, inner)))
	}
	rendered := m.styles.JumpCard.Width(inner).Render(strings.Join(lines, "\n"))
	return strings.Split(rendered, "\n")
}
