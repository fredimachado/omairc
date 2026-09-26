package ui

import (
	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// slashSession is the shell's slash-completion state. It mirrors
// IrcSlashSession: the projection comes from the controller (the shell cannot
// import internal/irc), while the open/selected/needle/dismissed state and the
// key semantics live here.
type slashSession struct {
	probe     controller.SlashProbe
	selected  int
	dismissed string
	lastText  string
}

// slashKeyResult is the outcome of routing one key through the session. It
// mirrors IrcSlashKeyResult: Accepted means the key was consumed, and
// Insertion (when non-empty) replaces the composer's slash query.
type slashKeyResult struct {
	accepted  bool
	insertion string
}

// open reports whether the list is showing.
func (s *slashSession) open() bool { return s.probe.Open }

// reset forgets all session state (after a send).
func (s *slashSession) reset() {
	s.probe = controller.SlashProbe{}
	s.selected = -1
}

// sync folds a fresh projection and the composer text into the session. It
// mirrors IrcSlashSession::sync + applyProbe, including the dismissedNeede
// rule: after Escape the list stays closed until the needle changes.
func (s *slashSession) sync(probe controller.SlashProbe, composerText string) {
	s.lastText = composerText
	if !probe.Open {
		if s.dismissed != "" && s.dismissed != probe.Needle {
			s.dismissed = ""
		}
		s.probe = controller.SlashProbe{}
		s.selected = -1
		return
	}
	if probe.Needle == s.dismissed {
		s.probe = controller.SlashProbe{}
		s.selected = -1
		return
	}
	s.dismissed = ""
	needleChanged := !s.probe.Open || s.probe.Needle != probe.Needle
	next := 0
	if !needleChanged {
		next = s.selected
		if next < 0 {
			next = 0
		}
		if next >= len(probe.Hits) {
			next = len(probe.Hits) - 1
		}
	}
	s.probe = probe
	s.selected = next
}

// selectedIndex returns the highlighted row, or -1.
func (s *slashSession) selectedIndex() int {
	if !s.open() {
		return -1
	}
	return s.selected
}

// insertionAt returns the text Tab would insert for one row, or "".
func (s *slashSession) insertionAt(index int) string {
	if !s.open() || index < 0 || index >= len(s.probe.Hits) {
		return ""
	}
	return s.probe.Hits[index].Label + " "
}

// replacedComposer replaces the whole slash query with one row's insertion,
// keeping any leading whitespace the composer had. It mirrors
// IrcSlashSession::replacedComposer.
func (s *slashSession) replacedComposer(index int) string {
	insertion := s.insertionAt(index)
	if insertion == "" {
		return ""
	}
	start := 0
	for start < len(s.lastText) && isSpaceByte(s.lastText[start]) {
		start++
	}
	return s.lastText[:start] + insertion
}

// tokenPassesSelected reports whether the selected row names the typed verb or
// one of its aliases, so Enter should send rather than insert. It mirrors
// IrcSlashSession::tokenPassesSelected.
func (s *slashSession) tokenPassesSelected() bool {
	if !s.open() || s.selected < 0 || s.selected >= len(s.probe.Hits) {
		return false
	}
	return s.probe.Hits[s.selected].Exact
}

// routeKey routes one composer key through the session. It returns Accepted
// when the session consumed the key; Insertion carries the replacement text.
// It mirrors IrcSlashSession::routeKey.
func (s *slashSession) routeKey(key string) slashKeyResult {
	if !s.open() {
		return slashKeyResult{}
	}
	switch key {
	case "esc", "escape":
		s.dismiss()
		return slashKeyResult{accepted: true}
	case "up":
		s.move(-1)
		return slashKeyResult{accepted: true}
	case "down":
		s.move(1)
		return slashKeyResult{accepted: true}
	case "tab":
		return slashKeyResult{accepted: true, insertion: s.replacedComposer(s.selected)}
	case "enter", "return":
		if s.tokenPassesSelected() {
			return slashKeyResult{}
		}
		return slashKeyResult{accepted: true, insertion: s.replacedComposer(s.selected)}
	}
	return slashKeyResult{}
}

// activate selects one row and returns the replacement text. It mirrors
// IrcSlashSession::activate.
func (s *slashSession) activate(index int) string {
	if !s.open() || len(s.probe.Hits) == 0 {
		return ""
	}
	if index < 0 {
		index = 0
	}
	if index >= len(s.probe.Hits) {
		index = len(s.probe.Hits) - 1
	}
	s.selected = index
	return s.replacedComposer(s.selected)
}

// dismiss closes the list and remembers the needle so sync keeps it closed.
func (s *slashSession) dismiss() {
	if !s.open() {
		return
	}
	s.dismissed = s.probe.Needle
	s.probe = controller.SlashProbe{}
	s.selected = -1
}

// move wraps the selection by delta. It mirrors the Up/Down arithmetic in
// IrcSlashSession::routeKey.
func (s *slashSession) move(delta int) {
	count := len(s.probe.Hits)
	if count == 0 {
		return
	}
	next := s.selected + delta
	next = ((next % count) + count) % count
	s.selected = next
}

// slashLines renders the completion list above the composer: a compact list of
// the highlighted row plus its usage. It returns nil when the list is closed.
func (m *Model) slashLines() []string {
	if !m.slash.open() {
		return nil
	}
	hits := m.slash.probe.Hits
	lines := make([]string, 0, len(hits))
	for index, hit := range hits {
		label := hit.Label
		if index == m.slash.selected {
			lines = append(lines, m.styles.SlashSelected.Render(label+"  "+hit.Usage))
			continue
		}
		lines = append(lines, m.styles.SlashRow.Render(label+"  "+hit.Usage))
	}
	return lines
}

// slashInsert applies a routed insertion to the composer and syncs the
// session. It is used for Tab/Enter/Up/Down/Escape and for text mutations that
// re-project.
func (m *Model) slashInsert(text string) {
	if text == "" {
		return
	}
	m.composer.SetValue(text)
	m.composer.CursorEnd()
	m.syncSlash()
}

// syncSlash re-projects the composer text into the slash session, unless the
// user is browsing history (which must keep the list closed so Up/Down keep
// walking the history). It mirrors the window's `slashCommands.sync` calls.
func (m *Model) syncSlash() {
	if m.ctrl == nil {
		m.slash.reset()
		return
	}
	if m.composerHistoryIndex >= 0 {
		m.slash.dismiss()
		return
	}
	m.slash.sync(m.ctrl.SlashProject(m.composer.Value(), m.ctrl.ConsoleOpen()), m.composer.Value())
}

// routeSlashKey routes one composer key through the slash session and applies
// any insertion. It reports whether the key was consumed, so the chord table
// (Escape, Tab, Up/Down, Enter) does not also run.
func (m *Model) routeSlashKey(key string) (bool, tea.Cmd) {
	result := m.slash.routeKey(key)
	if !result.accepted {
		return false, nil
	}
	if result.insertion != "" {
		m.slashInsert(result.insertion)
	}
	m.refocusComposer()
	return true, nil
}

// isSpaceByte reports ASCII whitespace, matching the slash needle rule.
func isSpaceByte(value byte) bool {
	switch value {
	case ' ', '\t', '\n', '\v', '\f', '\r':
		return true
	}
	return false
}
