package ui

import (
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
)

// This file is the Ctrl+Shift+O link sheet. The URL extraction and the
// allowlisted open path are a Phase 10 seam: the overlay opens and closes and
// the chord is real, but the source list is empty today. It is gated while
// Connect is visible and stays enabled while it is already open so the chord
// can toggle it closed.

// linkState is the link sheet. The filter input is wired now; the row source
// lands in Phase 10.
type linkState struct {
	open  bool
	input textinput.Model
}

func newLinkState() linkState {
	input := textinput.New()
	input.Placeholder = "Filter links"
	input.Prompt = "› "
	return linkState{input: input}
}

// linkVisible reports whether the link sheet is open.
func (m *Model) linkVisible() bool { return m != nil && m.link.open }

// toggleLink opens or closes the link sheet.
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

// linkEntries is the Phase 10 URL-extraction seam. It is empty today.
func (m *Model) linkEntries() []string {
	return nil
}

// handleLinkKey folds one key while the sheet is open. Escape and the toggle
// chord close it; everything else filters the (empty) list.
func (m *Model) handleLinkKey(key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	switch key {
	case "esc", "escape", "ctrl+shift+o":
		m.closeLink()
		return m, nil
	}
	var cmd tea.Cmd
	m.link.input, cmd = m.link.input.Update(msg)
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
	entries := m.linkEntries()
	if len(entries) == 0 {
		lines = append(lines, m.styles.JumpEmpty.Render("No links"))
	}
	for _, entry := range entries {
		lines = append(lines, m.styles.JumpRow.Render(truncateLine(entry, inner)))
	}
	rendered := m.styles.JumpCard.Width(inner).Render(strings.Join(lines, "\n"))
	return strings.Split(rendered, "\n")
}
