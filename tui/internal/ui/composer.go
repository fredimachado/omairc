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

// composerFrameRows is the border row the framed composer wears above and below
// its input line. The frame replaced the blank fill rows the field used to
// carry, so the block still spans composerFieldHeight rows and the input still
// sits on composerRow().
const composerFrameRows = 1

// composerFieldHeight is the composer block's total row span.
const composerFieldHeight = 1 + 2*composerFrameRows

// newComposerInput builds the composer's text input from the shell styles.
// model.go's New must build the composer with this instead of textinput.New. It
// uses the shared input set (Input in style.go), so the composer's prompt,
// placeholder, and caret follow the theme like every other field and the
// interior stays unfilled. It also switches the input to a real terminal cursor;
// composerCursor returns nil while the virtual cursor is on, and View should
// place the real one. The styles are a snapshot: model.go's SetTheme must
// re-apply m.composer.SetStyles(m.styles.Input) after it rebuilds the
// shell styles.
func newComposerInput(styles Styles) textinput.Model {
	input := textinput.New()
	input.SetStyles(styles.Input)
	input.Placeholder = composerPlaceholder
	input.Prompt = composerPrompt
	input.SetVirtualCursor(false)
	return input
}

// composerView renders the composer as a framed box inside the transcript
// column, sized and indented to that column rather than the window.
// newComposerInput puts the prompt and placeholder inside the input, so the
// interior is just the input clipped to the box's inner width (composerWidth in
// model.go minus the border). The frame wears the same rounded border as the
// side rails — lit while the composer owns the keyboard, plain while a modal or
// overlay does — and the interior carries no fill of its own, so the field reads
// as framed text over the window background and its bottom border lands on the
// line the rails bottom out on. The block is padded to the transcript width so
// it joins the transcript grid exactly and never bleeds into the member column.
// A terminal too small for the columns keeps the old bare, single-row field.
// While find is active the composer is the find query box.
func (m *Model) composerView() string {
	if m == nil {
		return ""
	}
	if m.terminalTooSmall() {
		return m.styles.Composer.Inline(true).MaxWidth(m.width).Render(m.composer.View())
	}
	inner := m.composerInteriorWidth()
	field := truncateLine(m.composer.View(), inner)
	if pad := inner - lipgloss.Width(field); pad > 0 {
		field += strings.Repeat(" ", pad)
	}
	indent := strings.Repeat(" ", composerInset)
	box := m.composerFrameStyle().Width(m.composerWidth()).Render(field)
	lines := strings.Split(box, "\n")
	for index, line := range lines {
		lines[index] = indent + line
	}
	return lipgloss.NewStyle().Width(m.transcriptWidth()).Render(strings.Join(lines, "\n"))
}

// composerFrameStyle is the composer box's border style: the side rails' frame
// with the focus border while the composer owns the keyboard, dropped to the
// plain border while a modal or an overlay does. It mirrors the QML composer's
// accent-while-focused border.
func (m *Model) composerFrameStyle() lipgloss.Style {
	if m.composer.Focused() {
		return m.styles.PanelFocused
	}
	return m.styles.Panel
}

// composerBorderWidth is the frame thickness on one side of the box.
func (m *Model) composerBorderWidth() int {
	frameX, _ := m.composerFrameStyle().GetFrameSize()
	return frameX / 2
}

// composerInteriorWidth is the box's inner width: composerWidth minus both
// border cells. It is what the input and its fill are clipped to.
func (m *Model) composerInteriorWidth() int {
	width := m.composerWidth() - 2*m.composerBorderWidth()
	if width < 1 {
		width = 1
	}
	return width
}

// composerTextLeft is the left cell of the box's interior: past the border, where
// the input's prompt starts. composerCursor offsets the real cursor by it.
func (m *Model) composerTextLeft() int {
	return m.composerLeft() + m.composerBorderWidth()
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
	// The box is inset inside the transcript column and bordered, so the real
	// cursor moves with the column's left edge, its inset, and the border. A
	// terminal too small for the columns renders the bare full-width field at
	// the left edge and needs no offset.
	if !m.terminalTooSmall() {
		cursor.Position.X += m.composerTextLeft()
	}
	return cursor
}

// notifyComposerTyping reports the composer's current text to the controller so
// it can publish an outbound typing hint for the selected conversation. It
// mirrors OmaircWindow.qml's handleComposerText -> irc.notifyComposerText, but
// compares against the last value so it fires only on a real change, the way
// QML's textChanged does. Update defers it, so no composer mutation needs its
// own call site: a keystroke, history recall, nick completion, a restored
// draft, a slash insertion, and the post-send clear are all covered by the one
// place the runtime delivers every message.
func (m *Model) notifyComposerTyping() {
	if m == nil || m.ctrl == nil {
		return
	}
	value := m.composer.Value()
	if value == m.lastComposerText {
		return
	}
	m.lastComposerText = value
	m.ctrl.NotifyComposerText(value)
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

// nickCompleteSession is the composer's Tab-completion cursor. It mirrors
// OmaircWindow.qml's nickCompleteMatches / nickCompleteIndex /
// nickCompleteOrigin: the first Tab picks the alphabetically first matching
// nick or channel, Shift+Tab starts at the last, and further presses cycle,
// wrapping. Any other key, a conversation switch, or a send resets it (see
// resetNickComplete).
type nickCompleteSession struct {
	matches []string
	index   int
	origin  int
	active  bool
}

// resetNickComplete ends a nick-completion session. It mirrors
// OmaircWindow.qml's resetNickComplete.
func (m *Model) resetNickComplete() {
	m.nickComplete = nickCompleteSession{}
}

// completeNick completes the composer's current word with Tab. It mirrors
// OmaircWindow.qml's completeNick: a first Tab starts a session from the
// matched nicks and channel names (even when the prefix is ambiguous) and
// repeated Tabs cycle through them. backward starts at the last match and
// walks toward the first. Shift+Tab is that direction.
func (m *Model) completeNick(backward bool) {
	if m.ctrl == nil || m.ctrl.ConsoleOpen() {
		return
	}
	if m.nickComplete.active && len(m.nickComplete.matches) > 0 {
		count := len(m.nickComplete.matches)
		if backward {
			m.nickComplete.index = (m.nickComplete.index + count - 1) % count
		} else {
			m.nickComplete.index = (m.nickComplete.index + 1) % count
		}
		m.applyNickComplete()
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
	if len(matches) == 0 {
		return
	}
	index := 0
	if backward {
		index = len(matches) - 1
	}
	m.nickComplete = nickCompleteSession{matches: matches, index: index, origin: origin, active: true}
	m.applyNickComplete()
}

// applyNickComplete replaces the in-progress word (from the session's origin
// through the cursor) with the selected nick and a separator: a leading colon
// and space when the completion opens the line, otherwise a trailing space. It
// mirrors OmaircWindow.qml's applyNickComplete.
func (m *Model) applyNickComplete() {
	if !m.nickComplete.active || len(m.nickComplete.matches) == 0 {
		return
	}
	origin := m.nickComplete.origin
	if origin < 0 {
		return
	}
	runes := []rune(m.composer.Value())
	if origin > len(runes) {
		return
	}
	cursor := m.composer.Position()
	if cursor < origin {
		cursor = origin
	}
	if cursor > len(runes) {
		cursor = len(runes)
	}
	nick := m.nickComplete.matches[m.nickComplete.index]
	insertion := []rune(nick + " ")
	if origin == 0 {
		insertion = []rune(nick + ": ")
	}
	result := make([]rune, 0, origin+len(insertion)+len(runes)-cursor)
	result = append(result, runes[:origin]...)
	result = append(result, insertion...)
	result = append(result, runes[cursor:]...)
	m.composer.SetValue(string(result))
	m.composer.SetCursor(origin + len(insertion))
}

// nickMatchesForPrefix returns channel members and the current network's
// channel names whose text starts with prefix, sorted by that text. On a
// direct message the target nick is a candidate too. Status has none.
func (m *Model) nickMatchesForPrefix(prefix string) []string {
	if m.ctrl == nil || m.ctrl.ConsoleOpen() {
		return nil
	}
	lower := strings.ToLower(prefix)
	seen := make(map[string]struct{})
	candidates := make([]string, 0, len(m.ctrl.Members()))
	add := func(name string) {
		if name == "" {
			return
		}
		if _, ok := seen[name]; ok {
			return
		}
		seen[name] = struct{}{}
		candidates = append(candidates, name)
	}
	if m.ctrl.IsChannel() {
		for _, member := range m.ctrl.Members() {
			add(member.Nick)
		}
	} else {
		add(m.ctrl.SelectedTarget())
	}
	networkID := m.ctrl.SelectedNetworkID()
	for _, row := range m.ctrl.Conversations() {
		if row.Direct || row.NetworkID != networkID {
			continue
		}
		add(row.Conversation)
	}
	matches := make([]string, 0, len(candidates))
	for _, name := range candidates {
		if strings.HasPrefix(strings.ToLower(name), lower) {
			matches = append(matches, name)
		}
	}
	sort.Strings(matches)
	return matches
}

// clampComposer stops the composer at the current surface's send budget,
// including a paste. A `/me` draft uses the expanded ACTION cap. Find reuses
// the field and is not a send, so a query is left alone. It mirrors
// OmaircWindow.qml's clampComposerToSendLimit.
func (m *Model) clampComposer() {
	if m == nil || m.ctrl == nil || m.find.active {
		return
	}
	value := m.composer.Value()
	budget := m.ctrl.ComposerByteBudgetFor(value)
	if m.composer.CharLimit != budget {
		m.composer.CharLimit = budget
	}
	clamped := m.ctrl.ClampUtf8Prefix(value, budget)
	if clamped == value {
		return
	}
	pos := m.composer.Position()
	m.composer.SetValue(clamped)
	m.composer.SetCursor(pos)
}
