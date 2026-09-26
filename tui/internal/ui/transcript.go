package ui

import (
	"fmt"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// transcriptView renders the selected conversation: its topic, then either the
// Status console lines when the console is open or the live transcript. The
// viewport honors the Phase 6 scroll offset and the find highlight; it pins to
// the tail while follow-the-end is on. The console is read through the
// controller's text accessor so this package never imports internal/irc.
func (m *Model) transcriptView(height int) string {
	lines, _ := m.transcriptLines()
	start, end := m.transcriptWindow(len(lines), height)
	if start > len(lines) {
		start = len(lines)
	}
	if end > len(lines) {
		end = len(lines)
	}
	return renderColumn(m.styles.Conversation, m.transcriptWidth(), fitLines(lines[start:end], height, false))
}

// transcriptLines builds the transcript's rendered lines and the count of
// header lines (the topic block) that precede the message/console rows. Find
// and copy index into the rows, not the header.
func (m *Model) transcriptLines() ([]string, int) {
	lines := make([]string, 0, 32)
	if topic := m.ctrl.Topic(); topic != "" {
		header := topic
		if m.ctrl.IsChannel() {
			header = fmt.Sprintf("%s  (%d)", topic, m.ctrl.PeopleCount())
		}
		lines = append(lines, m.styles.Topic.Render(header))
		lines = append(lines, "")
	}
	headerCount := len(lines)

	if m.ctrl.ConsoleOpen() {
		for index, text := range m.ctrl.ConsoleText(m.ctrl.FocusedNetworkID()) {
			style := m.styles.ConsoleLine
			if m.findMatchAt(index) {
				style = m.styles.FindMatch
			}
			lines = append(lines, style.Render(text))
		}
	} else {
		for index, message := range m.ctrl.Messages() {
			lines = append(lines, m.messageRow(index, message))
		}
	}
	return lines, headerCount
}

// transcriptWindow returns the [start, end) slice of lines the viewport shows,
// counting transcriptScroll as the number of lines hidden below the window.
func (m *Model) transcriptWindow(n, height int) (int, int) {
	maxOffset := n - height
	if maxOffset < 0 {
		maxOffset = 0
	}
	offset := m.transcriptScroll
	if m.transcriptFollowEnd {
		offset = 0
	}
	if offset > maxOffset {
		offset = maxOffset
	}
	if offset < 0 {
		offset = 0
	}
	start := n - height - offset
	if start < 0 {
		start = 0
	}
	end := start + height
	if end > n {
		end = n
	}
	return start, end
}

// transcriptRowTexts is the plain text find and copy match against: author and
// body for a message row, the line itself for Status. It mirrors
// OmaircWindow.qml's transcriptRowText.
func (m *Model) transcriptRowTexts() []string {
	if m.ctrl == nil {
		return nil
	}
	if m.ctrl.ConsoleOpen() {
		return m.ctrl.ConsoleText(m.ctrl.FocusedNetworkID())
	}
	messages := m.ctrl.Messages()
	rows := make([]string, 0, len(messages))
	for _, message := range messages {
		rows = append(rows, message.Author+" "+message.Body)
	}
	return rows
}

// findMatchAt reports whether the row is the current find match.
func (m *Model) findMatchAt(row int) bool {
	return m.find.active && row >= 0 && row == m.find.index
}

// pageTranscript scrolls the transcript by a page (or a fraction of one).
// Scrolling up leaves follow-the-end; scrolling back to the bottom restores
// it.
func (m *Model) pageTranscript(direction int, fraction float64) {
	if m.ctrl == nil {
		return
	}
	lines, headerCount := m.transcriptLines()
	n := len(lines)
	height := m.bodyHeight()
	maxOffset := n - height
	if maxOffset <= 0 {
		m.transcriptScroll = 0
		m.transcriptFollowEnd = true
		return
	}
	offset := m.transcriptScroll
	if m.transcriptFollowEnd {
		offset = 0
	}
	page := int(float64(height)*fraction + 0.5)
	if page < 1 {
		page = 1
	}
	if direction < 0 {
		offset += page
	} else {
		offset -= page
	}
	if offset < 0 {
		offset = 0
	}
	if offset > maxOffset {
		offset = maxOffset
	}
	m.transcriptScroll = offset
	m.transcriptFollowEnd = offset == 0
	m.transcriptCursor = visibleRowCursor(headerCount, n, height, offset)
}

// jumpTranscript jumps to the top or the bottom of the transcript.
func (m *Model) jumpTranscript(toEnd bool) {
	if m.ctrl == nil {
		return
	}
	lines, headerCount := m.transcriptLines()
	n := len(lines)
	height := m.bodyHeight()
	if toEnd {
		m.transcriptFollowEnd = true
		m.transcriptScroll = 0
		m.transcriptCursor = n - headerCount - 1
		return
	}
	maxOffset := n - height
	if maxOffset < 0 {
		maxOffset = 0
	}
	m.transcriptFollowEnd = false
	m.transcriptScroll = maxOffset
	m.transcriptCursor = 0
}

// revealTranscriptRow scrolls the viewport so one message/console row is
// visible and leaves follow-the-end so the match stays put.
func (m *Model) revealTranscriptRow(row int) {
	if m.ctrl == nil {
		return
	}
	lines, headerCount := m.transcriptLines()
	n := len(lines)
	height := m.bodyHeight()
	maxOffset := n - height
	if maxOffset < 0 {
		maxOffset = 0
	}
	index := headerCount + row
	if index < 0 {
		return
	}
	start := index - height/2
	if start+height <= index {
		start = index - height + 1
	}
	if start < 0 {
		start = 0
	}
	if start > maxOffset {
		start = maxOffset
	}
	m.transcriptFollowEnd = false
	m.transcriptScroll = n - height - start
	if m.transcriptScroll < 0 {
		m.transcriptScroll = 0
	}
}

// visibleRowCursor is the message/console row nearest the bottom of the visible
// window, used as the copy cursor after a page.
func visibleRowCursor(headerCount, n, height, offset int) int {
	start := n - height - offset
	if start < 0 {
		start = 0
	}
	lastRow := start + height - 1 - headerCount
	rows := n - headerCount
	if lastRow >= rows {
		lastRow = rows - 1
	}
	if lastRow < 0 {
		return -1
	}
	return lastRow
}

// messageRow renders one transcript line. Actions, events, and notices get
// their own shape; a mention is emphasized. The current find match wins over
// every other style.
func (m *Model) messageRow(index int, message controller.MessageSnapshot) string {
	if m.findMatchAt(index) {
		return m.styles.FindMatch.Render(m.transcriptLine(message))
	}
	switch message.Kind {
	case "action":
		return m.styles.Action.Render("* " + message.Author + " " + message.Body)
	case "event":
		return m.styles.Event.Render(message.Body)
	case "notice":
		return m.styles.Notice.Render("-" + message.Author + "- " + message.Body)
	}
	line := message.Time.Format("15:04") + " " + message.Author + " " + message.Body
	if message.Mentioned {
		return m.styles.MentionBody.Render(line)
	}
	return line
}

// transcriptLine is the unstyled text of a message row, matching the find
// haystack.
func (m *Model) transcriptLine(message controller.MessageSnapshot) string {
	switch message.Kind {
	case "action":
		return "* " + message.Author + " " + message.Body
	case "event":
		return message.Body
	case "notice":
		return "-" + message.Author + "- " + message.Body
	}
	return message.Time.Format("15:04") + " " + message.Author + " " + message.Body
}
