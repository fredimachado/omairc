package ui

// Unsent composer text across a restart. m.drafts keeps a draft per target
// while the program runs, and saveDraft writes it on every keystroke. The
// controller keeps each conversation's draft in drafts.json
// (controller.RememberDraft); this file hands it over when the shell switches
// away, when the terminal loses focus, when a line is sent, and on quit, never
// per keystroke, and asks for it back when a conversation shows. What is kept
// is the stored draft or unsentText, never the field's text, which may be a
// find query or a recalled history line. Status has no persisted draft.
// Rust: client/src/ui/shell/drafts.rs.

// unsentText is the text the user typed and has not sent. While find is on the
// field holds the query, and while Up/Down walks history it holds a recalled
// line, so the saved draft stands in for the field in both cases.
func (m *Model) unsentText() string {
	if m.find.active {
		return m.find.draft
	}
	if m.composerHistoryIndex >= 0 {
		return m.composerHistoryDraft
	}
	return m.composer.Value()
}

// rememberDraftFor keeps text as the stored draft of draftKey. A Status key
// ("status\n<network>"), an empty key, and a nil controller keep nothing.
func (m *Model) rememberDraftFor(draftKey, text string) {
	if m.ctrl == nil {
		return
	}
	if isStatusDraftKey(draftKey) {
		return
	}
	m.ctrl.RememberDraftByID(draftKey, text)
}

// isStatusDraftKey reports a Status surface key ("status\n<network>").
func isStatusDraftKey(draftKey string) bool {
	return len(draftKey) > len(statusDraftPrefix) && draftKey[:len(statusDraftPrefix)] == statusDraftPrefix
}

const statusDraftPrefix = "status\n"

// persistCurrentDraft keeps what the composer holds as unsent text for the
// conversation on screen. Blur and quit call it. It does not touch m.drafts.
func (m *Model) persistCurrentDraft() {
	m.rememberDraftFor(m.draftKey, m.unsentText())
}

// FlushDraft keeps the composer's unsent text once more after the program has
// returned, which covers every exit path (SIGHUP, a returned error) that did
// not pass through Ctrl+Q. It takes the controller lock itself, since no
// Update turn holds it then.
func (m *Model) FlushDraft() {
	if m == nil || m.ctrl == nil {
		return
	}
	m.ctrl.Lock()
	defer m.ctrl.Unlock()
	m.persistCurrentDraft()
}

// storedDraft is the draft the controller kept for draftKey, if the key names
// a conversation.
func (m *Model) storedDraft(draftKey string) (string, bool) {
	if m.ctrl == nil || isStatusDraftKey(draftKey) {
		return "", false
	}
	return m.ctrl.DraftForID(draftKey)
}

// restoreLaunchDraft fills the composer from the stored draft of the view the
// program opens on. A conversation selected later goes through loadDraft.
func (m *Model) restoreLaunchDraft() {
	if stored, ok := m.storedDraft(m.draftKey); ok {
		m.drafts[m.draftKey] = stored
		m.composer.SetValue(stored)
		m.composer.CursorEnd()
	}
}
