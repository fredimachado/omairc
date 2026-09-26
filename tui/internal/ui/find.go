package ui

import (
	"strings"

	tea "charm.land/bubbletea/v2"
)

// This file is the Ctrl+F find session. Find reuses the composer as its query
// box: the unsent draft is stashed on entry and restored on Escape. The current
// match is highlighted in the transcript and the viewport is left un-pinned so
// the match stays put, mirroring OmaircWindow.qml's beginOrAdvanceFind.

// findState is the Ctrl+F query, its current match row, and the composer draft
// stashed when find opened.
type findState struct {
	active bool
	index  int
	draft  string
}

// beginOrAdvanceFind enters find on the first Ctrl+F (stashing the draft and
// running the current composer text as the query), then advances on each later
// press.
func (m *Model) beginOrAdvanceFind() {
	if m.ctrl == nil {
		return
	}
	if !m.find.active {
		m.find.draft = m.composer.Value()
		m.find.active = true
		m.find.index = -1
		m.transcriptCursor = -1
		m.composer.Placeholder = "Find"
		m.advanceFind(true)
		return
	}
	m.advanceFind(false)
}

// advanceFind moves to the next match, wrapping. fromStart begins the search
// at the first row (used while the query is being typed).
func (m *Model) advanceFind(fromStart bool) {
	query := strings.ToLower(strings.TrimSpace(m.composer.Value()))
	if query == "" {
		m.find.index = -1
		return
	}
	rows := m.transcriptRowTexts()
	if len(rows) == 0 {
		m.find.index = -1
		return
	}
	start := m.find.index
	if fromStart {
		start = -1
	}
	for step := 1; step <= len(rows); step++ {
		index := ((start+step)%len(rows) + len(rows)) % len(rows)
		if strings.Contains(strings.ToLower(rows[index]), query) {
			m.find.index = index
			m.transcriptCursor = index
			m.revealTranscriptRow(index)
			return
		}
	}
	m.find.index = -1
}

// leaveFind leaves find and restores the stashed draft, the way Escape does.
func (m *Model) leaveFind() {
	if !m.find.active {
		return
	}
	m.find.active = false
	m.find.index = -1
	m.composer.Placeholder = "Message"
	m.composer.SetValue(m.find.draft)
	m.composer.CursorEnd()
}

// handleFindKey folds one key while find is active. Escape leaves find, Enter
// and Ctrl+F advance, plain Up/Down/Tab are swallowed, other navigation chords
// still work, and everything else edits the query and re-runs find from the
// start.
func (m *Model) handleFindKey(key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	switch key {
	case "esc", "escape":
		m.leaveFind()
		return m, nil
	case "enter", "return":
		m.advanceFind(false)
		return m, nil
	case "ctrl+f":
		m.advanceFind(false)
		return m, nil
	case "up", "down", "tab":
		return m, nil
	}
	if handled, cmd := m.dispatchChord(key, msg); handled {
		return m, cmd
	}
	var cmd tea.Cmd
	m.composer, cmd = m.composer.Update(msg)
	m.advanceFind(true)
	return m, cmd
}
