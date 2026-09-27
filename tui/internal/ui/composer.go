package ui

import (
	"sort"
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
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

// newComposerInput builds the composer's text input from the shell styles.
// model.go's New must build the composer with this instead of textinput.New, so
// the field matches the overlay filters' theme-driven look (see newTextInput in
// style.go). It also switches the input to a real terminal cursor; composerCursor
// returns nil while the virtual cursor is on, and View should place the real one.
// The styles are a snapshot: model.go's SetTheme must re-apply
// m.composer.SetStyles(m.styles.Input) after it rebuilds the shell styles.
func newComposerInput(styles Styles) textinput.Model {
	input := newTextInput(styles, composerPlaceholder, composerPrompt)
	input.SetVirtualCursor(false)
	return input
}

// composerView renders the single-line composer. newComposerInput puts the
// prompt and placeholder inside the input, so the line is just the styled field
// clipped to the window: the input's width already excludes composerPrefixWidth
// and MaxWidth drops a stray wide glyph instead of wrapping to a second line. A
// leading "/" is still plain text until the slash catalog lands. While find is
// active the composer is the find query box.
func (m *Model) composerView() string {
	line := m.composer.View()
	// model.go still builds the composer with textinput.New and no prompt in
	// this worktree. newComposerInput moves the prompt into the input; until the
	// integrator wires it, render the chip here so the line never loses it.
	if m.composer.Prompt == "" {
		line = m.styles.Prompt.Render(composerPrompt) + line
	}
	return m.styles.Composer.MaxWidth(m.width).Inline(true).Render(line)
}

// composerCursor is the composer's real terminal cursor at row, the composer
// line's zero-based index in the rendered frame, or nil when the composer is
// not focused or still uses its virtual cursor. model.go's View sets
// v.Cursor = m.composerCursor(row); the row is model.go's, because the footer
// sits below the composer. The input's own Cursor already accounts for a prompt
// it owns, so only the composerView chip fallback needs an extra offset.
func (m *Model) composerCursor(row int) *tea.Cursor {
	if m == nil || m.ctrl == nil {
		return nil
	}
	cursor := m.composer.Cursor()
	if cursor == nil {
		return nil
	}
	if m.composer.Prompt == "" {
		cursor.Position.X += composerPrefixWidth
	}
	cursor.Position.Y = row
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
