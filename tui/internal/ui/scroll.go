package ui

// Per-conversation scroll memory. The controller stores the place; this file
// decides when to read and write it. It mirrors OmaircWindow.qml's
// rememberOpenTranscript / restoreRememberedTranscript / finishTranscriptSwitch.
// Status shares the viewport, so a Status scroll must not clobber the
// conversation, and leaving Status must restore the saved line.

func (m *Model) rememberOpenTranscript() {
	if m == nil || m.suspendScrollMemory || m.scrollRestorePending || m.ctrl == nil || m.ctrl.ConsoleOpen() {
		return
	}
	if m.transcriptFollowEnd {
		m.ctrl.RememberScrollPlace(true, -1)
		return
	}
	row := m.firstVisibleMessageRow()
	if row < 0 {
		row = 0
	}
	m.ctrl.RememberScrollPlace(false, row)
}

// firstVisibleMessageRow is the Messages() index at the top of the viewport.
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
	offset := m.transcriptScroll
	if m.transcriptFollowEnd {
		offset = 0
	}
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
		m.scrollRestorePending = false
		return "absent"
	}
	place := m.ctrl.CurrentScrollPlace()
	if !place.Known {
		m.scrollRestorePending = false
		return "absent"
	}
	if place.Follow {
		m.scrollRestorePending = false
		m.setTranscriptFollowEnd(true)
		m.transcriptScroll = 0
		m.firstUnseenRow = -1
		return "applied"
	}
	if place.Row >= 0 {
		m.scrollRestorePending = false
		m.pinTranscriptToRow(place.Row)
		return "applied"
	}
	// The line is not loaded. Show the last page without entering follow,
	// which would drop the history page cap, and without writing this
	// interim page over the anchor.
	m.scrollRestorePending = true
	m.transcriptFollowEnd = false
	m.transcriptScroll = 0
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
	if m == nil || !m.scrollRestorePending || m.suspendScrollMemory {
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
	m.scrollRestorePending = false
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
