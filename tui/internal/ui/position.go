package ui

import "github.com/fredimachado/omairc/tui/internal/controller"

// readerPosition is where the reader is in the open transcript: one value, so
// readings that cannot hold together are not storable together. A reader who
// follows the end has no scroll offset and no saved line to wait on. Only
// following the end reports follow-end to the read marker, so a restore that
// has not landed never lets it advance past unread backlog. The sealed
// interface follows `irc.Event`, and the four positions carry the names of
// `ReaderPosition` in `client/src/model/reader_position.rs`: followingEnd,
// anchored, restoring, and catchingUp.
type readerPosition interface {
	readerPosition()
	// reportsFollowEnd is true for followingEnd and no other position.
	reportsFollowEnd() bool
}

// followingEnd shows the newest lines and follows them as they arrive.
type followingEnd struct{}

// anchored holds the view on a line.
type anchored struct {
	Anchor readerAnchor
}

// restoring waits for a saved line that is not in the loaded transcript yet.
// The view shows the last page, and that interim page is never written over
// the saved place. OpenAtUnread is the preference the wait started with, as
// Rust's `Restoring`; the TUI never gives a wait up, so nothing reads it.
type restoring struct {
	Place        controller.ScrollView
	OpenAtUnread bool
}

// catchingUp holds the view on a line while lines that arrived since sit below
// it. FirstUnseen is the first of them, where ↓ new and Alt+U land. ResumeRow
// is where an Alt+U catch-up continues, or -1 until Alt+U has landed. Either
// one makes the reader catch up. A position with neither is anchored. The
// sequences name the same rows across a prepend that shifts or caps them, and
// are -1 when none is recorded.
type catchingUp struct {
	Anchor         readerAnchor
	FirstUnseen    int
	FirstUnseenSeq int64
	ResumeRow      int
	ResumeSeq      int64
}

// readerAnchor is where a held reader holds the view. Scroll is the number of
// lines hidden below the window. PinSeq names the top row to pin again after a
// CHATHISTORY prepend, or -1 when none is armed; TextLead is how many rendered
// lines that row's text sits below the viewport start (a date rule or unread
// mark above the text makes it positive, and restore subtracts it so chrome
// added or removed above the view does not shift the text); SpliceEpoch is the
// splice generation when the pin was armed, and restore runs only after the
// epoch advances. -1 when no pin is armed.
type readerAnchor struct {
	Scroll      int
	PinSeq      int64
	TextLead    int
	SpliceEpoch int
}

// plainAnchor is an anchor with no pin armed.
func plainAnchor(scroll int) readerAnchor {
	return readerAnchor{Scroll: scroll, PinSeq: -1, SpliceEpoch: -1}
}

func (followingEnd) readerPosition() {}
func (anchored) readerPosition()     {}
func (restoring) readerPosition()    {}
func (catchingUp) readerPosition()   {}

func (followingEnd) reportsFollowEnd() bool { return true }
func (anchored) reportsFollowEnd() bool     { return false }
func (restoring) reportsFollowEnd() bool    { return false }
func (catchingUp) reportsFollowEnd() bool   { return false }

// position is the reader's position. A Model that never set one follows the
// end.
func (m *Model) position() readerPosition {
	if m == nil || m.reader == nil {
		return followingEnd{}
	}
	return m.reader
}

// setReader is the only writer of the reader's position.
func (m *Model) setReader(p readerPosition) {
	m.reader = p
}

// followingEnd reports whether the view shows the end and follows it.
func (m *Model) followingEnd() bool {
	return m.position().reportsFollowEnd()
}

// restoringNow reports whether a saved line is still being waited for.
func (m *Model) restoringNow() bool {
	_, ok := m.position().(restoring)
	return ok
}

// held is the reader's anchor and catch-up while the view is held on a line,
// with -1 for the rows of a reader who is not catching up. It reports false for
// a reader at the end or waiting on a saved line.
func (m *Model) held() (catchingUp, bool) {
	switch p := m.position().(type) {
	case anchored:
		return catchingUp{Anchor: p.Anchor, FirstUnseen: -1, FirstUnseenSeq: -1, ResumeRow: -1, ResumeSeq: -1}, true
	case catchingUp:
		return p, true
	}
	return catchingUp{}, false
}

// setHeld stores a held reader. With no row left to catch up on it is anchored.
func (m *Model) setHeld(c catchingUp) {
	if c.FirstUnseen < 0 {
		c.FirstUnseen, c.FirstUnseenSeq = -1, -1
	}
	if c.ResumeRow < 0 {
		c.ResumeRow, c.ResumeSeq = -1, -1
	}
	if c.FirstUnseen < 0 && c.ResumeRow < 0 {
		m.setReader(anchored{Anchor: c.Anchor})
		return
	}
	m.setReader(c)
}

// scrollLines is how many lines are hidden below the window: zero for a
// reader at the end or waiting on a saved line.
func (m *Model) scrollLines() int {
	if c, ok := m.held(); ok {
		return c.Anchor.Scroll
	}
	return 0
}

// holdAt holds the view with scroll lines hidden below the window. A catch-up
// and an armed pin go on; a reader at the end, or waiting on a saved line, is
// now held with neither.
func (m *Model) holdAt(scroll int) {
	c, ok := m.held()
	if !ok {
		m.setReader(anchored{Anchor: plainAnchor(scroll)})
		return
	}
	c.Anchor.Scroll = scroll
	m.setHeld(c)
}

// leaveEnd holds a reader who is at the end, or waiting on a saved line, where
// the view is. A reader already held stays.
func (m *Model) leaveEnd() {
	if _, ok := m.held(); !ok {
		m.holdAt(0)
	}
}

// startRestore waits for the saved line. The view shows the last page.
func (m *Model) startRestore(place controller.ScrollView) {
	m.setReader(restoring{Place: place, OpenAtUnread: m.ctrl != nil && m.ctrl.OpenAtUnread()})
}

// retireRestore stops waiting for a saved line. The reader holds the last
// page it was shown.
func (m *Model) retireRestore() {
	if m.restoringNow() {
		m.holdAt(0)
	}
}

// firstUnseen is the first row that arrived while the reader was scrolled up,
// or -1.
func (m *Model) firstUnseen() int {
	if c, ok := m.held(); ok {
		return c.FirstUnseen
	}
	return -1
}

// firstUnseenSeq is the sequence of that row recorded for an older-page
// restore, or -1.
func (m *Model) firstUnseenSeq() int64 {
	if c, ok := m.held(); ok {
		return c.FirstUnseenSeq
	}
	return -1
}

// resumeRow is the message row where an Alt+U catch-up continues, or -1 when
// no catch-up is active.
func (m *Model) resumeRow() int {
	if c, ok := m.held(); ok {
		return c.ResumeRow
	}
	return -1
}

// resumeSeq is the sequence of that row recorded for an older-page restore, or
// -1.
func (m *Model) resumeSeq() int64 {
	if c, ok := m.held(); ok {
		return c.ResumeSeq
	}
	return -1
}

// pin is the armed older-page pin, with PinSeq -1 when none is armed.
func (m *Model) pin() readerAnchor {
	if c, ok := m.held(); ok {
		return c.Anchor
	}
	return plainAnchor(0)
}

// setFirstUnseen arms the first-new row, or drops it with a negative row. A
// reader at the end, or waiting on a saved line, has nothing to catch up on.
func (m *Model) setFirstUnseen(row int, seq int64) {
	if c, ok := m.held(); ok {
		c.FirstUnseen, c.FirstUnseenSeq = row, seq
		m.setHeld(c)
	}
}

// setResume moves the Alt+U catch-up row, or ends the catch-up with a negative
// row.
func (m *Model) setResume(row int, seq int64) {
	if c, ok := m.held(); ok {
		c.ResumeRow, c.ResumeSeq = row, seq
		m.setHeld(c)
	}
}

// clearFirstUnseen drops the jump-to-first-new row and any snapshotted sequence
// for an inflight older-page restore. Clearing only the row left a stale
// sequence that could re-arm ↓ new after a CHATHISTORY BEFORE splice.
func (m *Model) clearFirstUnseen() {
	if m == nil {
		return
	}
	m.setFirstUnseen(-1, -1)
}

// endCatchUp drops the catch-up. The reader stays on the same line.
func (m *Model) endCatchUp() {
	if c, ok := m.held(); ok {
		c.FirstUnseen, c.ResumeRow = -1, -1
		m.setHeld(c)
	}
}

// clearTranscriptAnchor disarms the older-page pin and the sequences recorded
// beside it.
func (m *Model) clearTranscriptAnchor() {
	if c, ok := m.held(); ok {
		c.Anchor.PinSeq, c.Anchor.TextLead, c.Anchor.SpliceEpoch = -1, 0, -1
		c.FirstUnseenSeq, c.ResumeSeq = -1, -1
		m.setHeld(c)
	}
}

// armTranscriptAnchor arms `pin` for the older page just requested. Its
// first-new and catch-up rows are recorded by sequence, so the restore
// re-resolves them after the prepend. A pin with no sequence disarms. Only a
// reader held on a line has a pin: one at the end stays there across the
// splice.
func (m *Model) armTranscriptAnchor(pin readerAnchor) {
	c, ok := m.held()
	if !ok {
		return
	}
	if pin.PinSeq < 0 {
		m.clearTranscriptAnchor()
		return
	}
	c.Anchor.PinSeq, c.Anchor.TextLead, c.Anchor.SpliceEpoch = pin.PinSeq, pin.TextLead, pin.SpliceEpoch
	c.FirstUnseenSeq = m.sequenceOf(c.FirstUnseen)
	c.ResumeSeq = m.sequenceOf(c.ResumeRow)
	m.setHeld(c)
}

// sequenceOf is the sequence of message row, or -1 when there is none.
func (m *Model) sequenceOf(row int) int64 {
	if m.ctrl == nil || row < 0 {
		return -1
	}
	messages := m.ctrl.Messages()
	if row >= len(messages) {
		return -1
	}
	return messages[row].Sequence
}
