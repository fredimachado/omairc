package ui

import tea "charm.land/bubbletea/v2"

// copySelection emits the OSC 52 clipboard sequence for the row under the
// transcript cursor, or the composer draft when there is no row. The TUI has no
// pointer path, so the app tracks a row cursor (set by find and by paging)
// instead of a mouse selection; the terminal's own mouse selection stays the
// user's.
func (m *Model) copySelection() tea.Cmd {
	text := m.copyTargetText()
	if text == "" {
		return nil
	}
	return tea.SetClipboard(text)
}

// copyTargetText is the text Ctrl+C copies: the find match, else the focused
// transcript row, else the composer draft, else the newest row.
func (m *Model) copyTargetText() string {
	if m.ctrl == nil {
		return ""
	}
	rows := m.transcriptRowTexts()
	if m.find.active && m.find.index >= 0 && m.find.index < len(rows) {
		return rows[m.find.index]
	}
	if m.transcriptCursor >= 0 && m.transcriptCursor < len(rows) {
		return rows[m.transcriptCursor]
	}
	if value := m.composer.Value(); value != "" {
		return value
	}
	if len(rows) > 0 {
		return rows[len(rows)-1]
	}
	return ""
}
