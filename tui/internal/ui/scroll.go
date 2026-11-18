package ui

// Per-conversation scroll memory. The controller stores the place; this file
// decides when to read and write it. It mirrors `client/src/ui/shell`'s
// rememberOpenTranscript / restoreRememberedTranscript / finishTranscriptSwitch.
// Status shares the viewport, so a Status scroll must not clobber the
// conversation, and leaving Status must restore the saved line.

func (m *Model) rememberOpenTranscript() {
	if m == nil || m.suspendScrollMemory || m.restoringNow() || m.ctrl == nil || m.ctrl.ConsoleOpen() {
		return
	}
	if m.followingEnd() {
		m.ctrl.RememberScrollPlace(true, -1)
		return
	}
	row := m.firstVisibleMessageRow()
	if row < 0 {
		row = 0
	}
	m.ctrl.RememberScrollPlace(false, row)
}

// firstVisibleMessageRow is the Messages index at the top of the viewport.
func (m *Model) firstVisibleMessageRow() int {
	if m == nil || m.ctrl == nil {
		return 0
	}
	messages := len(m.ctrl.Messages())
	if messages == 0 {
		return 0
	}
	area := m.transcriptArea()
	n := len(area.lines)
	height := m.transcriptRowsHeight()
	if height < 1 {
		height = 1
	}
	offset := m.scrollLines()
	start := n - height - offset
	if start < 0 {
		start = 0
	}
	row := area.rowAt(start)
	if row < 0 {
		return 0
	}
	if row >= messages {
		return messages - 1
	}
	return row
}

// applyRememberedTranscript lands on the saved place. "applied" moved the
// viewport, "pending" is waiting for the line to load, and "absent" means
// this conversation has no saved place.
func (m *Model) applyRememberedTranscript() string {
	if m.ctrl == nil || m.ctrl.ConsoleOpen() {
		m.retireRestore()
		return "absent"
	}
	place := m.ctrl.CurrentScrollPlace()
	if !place.Known {
		m.retireRestore()
		return "absent"
	}
	if place.Follow {
		m.followEnd()
		m.clearFirstUnseen()
		return "applied"
	}
	if place.Row >= 0 {
		m.retireRestore()
		m.pinTranscriptToRow(place.Row)
		if m.scrollLines() == 0 {
			// The line sits inside the last page, so the landing clamped to
			// the end: the reader follows it, as Rust's layout clamp rule has
			// it, and the read marker may advance.
			m.followEnd()
			m.clearFirstUnseen()
			m.syncReadMarkerViewport()
		}
		return "applied"
	}
	// The line is not loaded. Show the last page without entering follow
	// which would drop the history page cap, and without writing this
	// interim page over the anchor.
	m.startRestore(place)
	return "pending"
}

// landSavedTranscript applies a known place without writing it back. Startup
// and a resize both use it so the same line stays at the top when the width
// changes. A conversation with no saved place is left where the caller put it.
func (m *Model) landSavedTranscript() {
	defer m.syncTranscriptLineCount()
	if m == nil || m.suspendScrollMemory || m.ctrl == nil || m.ctrl.ConsoleOpen() {
		return
	}
	m.suspendScrollMemory = true
	m.applyRememberedTranscript()
	m.suspendScrollMemory = false
}

// maybeLandSavedTranscript retries a place whose line was not loaded yet.
func (m *Model) maybeLandSavedTranscript() {
	if m == nil || !m.restoringNow() || m.suspendScrollMemory {
		return
	}
	if m.ctrl == nil || m.ctrl.ConsoleOpen() {
		return
	}
	if m.applyRememberedTranscript() == "applied" {
		m.rememberOpenTranscript()
	}
}

// noteViewportSettled records the viewport after a user scroll. A selection
// change sets suspendScrollMemory so the transitional pin is not stored.
func (m *Model) noteViewportSettled() {
	// A real scroll retires a pending anchor. beginTranscriptSwitch calls
	// rememberOpenTranscript directly, so an anchor that has not loaded yet
	// stays stored.
	m.retireRestore()
	m.rememberOpenTranscript()
}

// beginTranscriptSwitch remembers the conversation being left. wasConsole is
// true when Status was the surface, so the return trip does not treat the
// shared viewport as a re-select.
func (m *Model) beginTranscriptSwitch() (previousID string, wasConsole bool) {
	previousID = m.selectedConversationID()
	wasConsole = m.ctrl != nil && m.ctrl.ConsoleOpen()
	m.rememberOpenTranscript()
	m.suspendScrollMemory = true
	return previousID, wasConsole
}
