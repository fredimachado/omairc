package ui

import (
	"sort"
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
)

// The composer's prompt and placeholder are rendered by the text input itself,
// so the shared styles.Input drives the accent prompt (dim when blurred), the
// dim placeholder, and the accent caret in one place. composerPrompt is two
// display cells wide, matching composerPrefixWidth in model.go, which is the
// room resize() leaves for it.
const (
	composerPrompt      = "› "
	composerPlaceholder = "Message"
	findPlaceholder     = "Find"
)

// composerFieldPad is the blank fill rows the composer block carries above and
// below its input line, so the field reads as a panel instead of a thin line.
// It goes on ComposerField as lipgloss PaddingTop/PaddingBottom: vertical
// padding renders inside the block, so the surface fill covers the padding rows,
// which a plain blank row cannot do. It costs two grid rows, so every row-budget
// calculation goes through composerFieldHeight instead of assuming one row.
const composerFieldPad = 1

// composerFieldHeight is the composer block's total row span.
const composerFieldHeight = 1 + 2*composerFieldPad

// newComposerInput builds the composer's text input from the shell styles.
// model.go's New must build the composer with this instead of textinput.New. It
// uses the composer's own input set (ComposerInput in style.go), which is the
// shared theme-driven look plus the surface fill that makes the field read as
// its own block under the transcript. It also switches the input to a real
// terminal cursor; composerCursor returns nil while the virtual cursor is on,
// and View should place the real one. The styles are a snapshot: model.go's
// SetTheme must re-apply m.composer.SetStyles(m.styles.ComposerInput) after it
// rebuilds the shell styles.
func newComposerInput(styles Styles) textinput.Model {
	input := textinput.New()
	input.SetStyles(styles.ComposerInput)
	input.Placeholder = composerPlaceholder
	input.Prompt = composerPrompt
	input.SetVirtualCursor(false)
	return input
}

// composerView renders the composer as a filled block centered under the
// transcript column. newComposerInput puts the prompt and placeholder inside the
// input, so the field is just the input clipped to the composer width
// (composerWidth in model.go). The block is inset within the transcript column
// and joined with blank cells outside it, so it groups with the transcript it
// belongs to instead of running the full window width. PaddingTop/PaddingBottom
// give it its own vertical breathing room, with the surface fill carrying
// through the padding rows. A terminal too small for the columns keeps the old
// bare, single-row full-width field. While find is active the composer is the
// find query box.
func (m *Model) composerView() string {
	if m == nil {
		return ""
	}
	if m.width < minWidth || m.height < minHeight {
		return m.styles.Composer.Inline(true).MaxWidth(m.width).Render(m.composer.View())
	}
	width := m.composerWidth()
	field := truncateLine(m.composer.View(), width)
	if pad := width - lipgloss.Width(field); pad > 0 {
		field += m.styles.ComposerField.Render(strings.Repeat(" ", pad))
	}
	indent := strings.Repeat(" ", m.composerLeft())
	block := m.styles.ComposerField.Padding(composerFieldPad, 0).Render(field)
	lines := strings.Split(block, "\n")
	for index, line := range lines {
		lines[index] = indent + line
	}
	return strings.Join(lines, "\n")
}

// composerCursor is the composer's real terminal cursor at row, the composer
// line's zero-based index in the rendered frame, or nil when the composer is
// not focused or still uses its virtual cursor. model.go's View sets
// v.Cursor = m.composerCursor(row); the row is model.go's, because the footer
// sits below the composer. The prompt is part of the input, so its own Cursor
// already carries the prompt offset.
func (m *Model) composerCursor(row int) *tea.Cursor {
	if m == nil || m.ctrl == nil {
		return nil
	}
	cursor := m.composer.Cursor()
	if cursor == nil {
		return nil
	}
	cursor.Position.Y = row
	// The composer is inset under the transcript column, so the real cursor
	// moves with the field. A terminal too small for the columns renders the
	// bare full-width field at the left edge and needs no offset.
	if m.width >= minWidth && m.height >= minHeight {
		cursor.Position.X += m.composerLeft()
	}
	return cursor
}

// composerHistoryLimit is how many sent lines the Up/Down recall keeps.
const composerHistoryLimit = 50

// rememberSentLine records one sent composer line for Up/Down recall. It
// mirrors OmaircWindow.qml's rememberSentComposerLine.
func (m *Model) rememberSentLine(text string) {
	m.composerHistory = append(m.composerHistory, text)
	if len(m.composerHistory) > composerHistoryLimit {
		m.composerHistory = m.composerHistory[len(m.composerHistory)-composerHistoryLimit:]
	}
	m.resetHistoryBrowse()
}

// resetHistoryBrowse leaves the history browsing session, so the next Up
// starts from the newest line and Down restores the current draft.
func (m *Model) resetHistoryBrowse() {
	m.composerHistoryIndex = -1
	m.composerHistoryDraft = ""
}

// recallHistory recalls sent lines with Up (-1) and Down (+1), restoring the
// in-progress draft when Down passes the newest line. It mirrors
// OmaircWindow.qml's recallComposerHistory.
func (m *Model) recallHistory(delta int) {
	lines := m.composerHistory
	if m.composer.Value() == "" && len(lines) == 0 {
		return
	}
	if m.composerHistoryIndex < 0 {
		if delta > 0 || len(lines) == 0 {
			return
		}
		m.composerHistoryDraft = m.composer.Value()
		m.composerHistoryIndex = len(lines)
	}
	next := m.composerHistoryIndex + delta
	if next < 0 {
		next = 0
	}
	if next >= len(lines) {
		draft := m.composerHistoryDraft
		m.resetHistoryBrowse()
		m.composer.SetValue(draft)
		m.composer.CursorEnd()
		return
	}
	m.composerHistoryIndex = next
	m.composer.SetValue(lines[next])
	m.composer.CursorEnd()
}

// completeNick completes the composer's current word with Tab. It mirrors
// OmaircWindow.qml's completeNick with the plan's simpler rule: exactly one
// candidate completes; no candidate or an ambiguous prefix leaves the draft.
func (m *Model) completeNick() {
	if m.ctrl == nil || m.ctrl.ConsoleOpen() {
		return
	}
	runes := []rune(m.composer.Value())
	cursor := m.composer.Position()
	if cursor > len(runes) {
		cursor = len(runes)
	}
	if cursor < 0 {
		cursor = 0
	}
	origin := 0
	for index := cursor - 1; index >= 0; index-- {
		if runes[index] == ' ' {
			origin = index + 1
			break
		}
	}
	token := string(runes[origin:cursor])
	if token == "" {
		return
	}
	matches := m.nickMatchesForPrefix(token)
	if len(matches) != 1 {
		return
	}
	insertion := []rune(matches[0] + " ")
	if origin == 0 {
		insertion = []rune(matches[0] + ": ")
	}
	result := make([]rune, 0, len(runes)+len(insertion))
	result = append(result, runes[:origin]...)
	result = append(result, insertion...)
	result = append(result, runes[cursor:]...)
	m.composer.SetValue(string(result))
	m.composer.SetCursor(origin + len(insertion))
}

// nickMatchesForPrefix returns the channel members whose nick starts with
// prefix, sorted by nick. On a direct message the target nick is the only
// candidate; Status has none.
func (m *Model) nickMatchesForPrefix(prefix string) []string {
	if m.ctrl == nil || m.ctrl.ConsoleOpen() {
		return nil
	}
	lower := strings.ToLower(prefix)
	candidates := make([]string, 0, len(m.ctrl.Members()))
	if m.ctrl.IsChannel() {
		for _, member := range m.ctrl.Members() {
			if member.Nick != "" {
				candidates = append(candidates, member.Nick)
			}
		}
	} else if target := m.ctrl.SelectedTarget(); target != "" {
		candidates = append(candidates, target)
	}
	matches := make([]string, 0, len(candidates))
	for _, nick := range candidates {
		if strings.HasPrefix(strings.ToLower(nick), lower) {
			matches = append(matches, nick)
		}
	}
	sort.Strings(matches)
	return matches
}
