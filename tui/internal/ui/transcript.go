package ui

import (
	"sort"
	"strings"
	"time"

	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// headerTailGutter is the blank band left between the transcript header's
// right-aligned tail and the member column joined immediately after it. The band
// spans transcriptWidth(), and the member column's "ONLINE - N" heading sits on
// the same row on the other side of its border, so the tail keeps this much band
// before the edge rather than running up against it.
const headerTailGutter = 2

// isMentionRow reports whether a message renders as a full-width wash band. Only
// a plain chat message can be a mention: the action, event, and notice shapes
// return from messageRow before it checks Mentioned.
func isMentionRow(message controller.MessageSnapshot) bool {
	switch message.Kind {
	case "action", "event", "notice":
		return false
	}
	return message.Mentioned
}

// transcriptRows is the rendered scrolling row area: the lines themselves, plus
// the line index and the height of every row. Rows are packed edge to edge, but
// a row is not always one line: the "New messages" boundary rides above its
// row. Find, copy, and the scroll offsets all resolve a row through this index
// rather than a stride.
type transcriptRows struct {
	lines   []string
	starts  []int
	heights []int
}

// count is the number of message/console rows.
func (r transcriptRows) count() int { return len(r.starts) }

// line is the first rendered line of a row.
func (r transcriptRows) line(row int) int { return r.starts[row] }

// rowAt is the row containing a line index, or -1 when the line precedes every
// row. A line inside a taller row belongs to that row.
func (r transcriptRows) rowAt(line int) int {
	index := sort.Search(len(r.starts), func(i int) bool { return r.starts[i] > line })
	return index - 1
}

// endOf is the line index just past a row's own lines.
func (r transcriptRows) endOf(row int) int { return r.starts[row] + r.heights[row] }

// end is the line index just past the last row's own lines. Anything after it
// is the typing footer, which is not a row.
func (r transcriptRows) end() int {
	if len(r.starts) == 0 {
		return 0
	}
	return r.endOf(len(r.starts) - 1)
}

// append adds one row block. The row's height comes from the rendered block
// itself, so the index can never drift from the lines that are actually
// emitted.
func (r *transcriptRows) append(block string) {
	lines := strings.Split(block, "\n")
	r.starts = append(r.starts, len(r.lines))
	r.heights = append(r.heights, len(lines))
	r.lines = append(r.lines, lines...)
}

// transcriptRowArea builds the scrolling row area: one block per message or
// Status line. It does not include the pinned header, so its first row starts
// at line 0.
func (m *Model) transcriptRowArea() transcriptRows {
	var area transcriptRows
	if m.ctrl == nil {
		return area
	}
	if m.ctrl.ConsoleOpen() {
		for index, text := range m.ctrl.ConsoleText(m.ctrl.FocusedNetworkID()) {
			style := m.styles.ConsoleLine
			if m.findMatchAt(index) {
				style = m.styles.FindMatch
			}
			area.append(style.Render(text))
		}
		return area
	}
	markRow := m.ctrl.UnreadMarkRow()
	messages := m.ctrl.Messages()
	// The nick column is one width for the whole transcript, so it is measured
	// once per render rather than per row.
	nickWidth := nickColumnWidth(messages)
	for index, message := range messages {
		block := m.messageRowAt(messages, index, message, nickWidth)
		// The "New messages" boundary sits above the first unread row, attached
		// to that row's block so the rendered row count still matches the
		// message count find and copy index by.
		if index == markRow {
			block = m.unreadMarkLine() + "\n" + block
		}
		area.append(block)
	}
	return area
}

// unreadMarkLine renders the "New messages" boundary: a centered caption
// between two accent-mixed rules sized to the transcript column, mirroring
// UnreadMark.qml. A column too narrow for the caption plus a rule on each side
// shows the caption alone.
func (m *Model) unreadMarkLine() string {
	width := m.transcriptWidth()
	label := m.styles.UnreadMark.Render("New messages")
	remaining := width - lipgloss.Width(label) - 2
	if remaining < 2 {
		return truncateLine(label, width)
	}
	left := remaining / 2
	right := remaining - left
	return m.styles.UnreadMarkRule.Render(strings.Repeat("─", left)) + " " +
		label + " " +
		m.styles.UnreadMarkRule.Render(strings.Repeat("─", right))
}

// transcriptRowTotal is the current transcript's row count: the number of
// messages, or of Status lines while the console is open. It is the count find
// and copy index, which is not the rendered line count.
func (m *Model) transcriptRowTotal() int {
	if m.ctrl == nil {
		return 0
	}
	if m.ctrl.ConsoleOpen() {
		return len(m.ctrl.ConsoleText(m.ctrl.FocusedNetworkID()))
	}
	return len(m.ctrl.Messages())
}

// transcriptView renders the selected conversation: its pinned topic header,
// then either the Status console lines when the console is open or the live
// transcript. The header stays put while the rows scroll, mirroring
// ConversationColumn.qml's fixed conversationHeader above its scrolling
// Flickable. The viewport honors the Phase 6 scroll offset and the find
// highlight; it pins to the tail while follow-the-end is on. The console is
// read through the controller's text accessor so this package never imports
// internal/irc.
func (m *Model) transcriptView(height int) string {
	header := m.transcriptHeader()
	if len(header) >= height {
		// No room for rows: show as much of the header as fits rather than
		// letting the pinned block push the grid over its budget.
		return renderColumn(m.styles.Conversation, m.transcriptWidth(), fitLines(header, height, false))
	}
	viewport := height - len(header)
	body := m.transcriptArea().lines
	start, end := m.transcriptWindow(len(body), viewport)
	if start > len(body) {
		start = len(body)
	}
	if end > len(body) {
		end = len(body)
	}
	rows := fitLines(body[start:end], viewport, false)
	lines := make([]string, 0, height)
	lines = append(lines, header...)
	lines = append(lines, rows...)
	return renderColumn(m.styles.Conversation, m.transcriptWidth(), lines)
}

// transcriptHeader renders the pinned header block: the topic line plus its
// trailing blank, or nothing when the conversation has neither a topic nor an
// armed jump hint. It stays put while the rows scroll under it.
func (m *Model) transcriptHeader() []string {
	if m.ctrl == nil {
		return nil
	}
	topic := m.ctrl.Topic()
	if topic == "" && m.unseenMarker() == "" {
		return nil
	}
	return []string{m.topicHeaderLine(topic), ""}
}

// transcriptArea is the scrolling row area with the direct-message typing footer
// appended. It is the single place the rows are assembled, so the view, the
// scroll offsets, find, and copy all read the same structure.
func (m *Model) transcriptArea() transcriptRows {
	area := m.transcriptRowArea()
	if m.ctrl != nil && !m.ctrl.ConsoleOpen() {
		area = m.appendTranscriptTypingFooter(area)
	}
	return area
}

// transcriptLines builds the transcript's rendered lines and the count of
// header lines (the topic block) that precede the message/console rows. Find
// and copy index into the rows, not the header.
func (m *Model) transcriptLines() ([]string, int) {
	header := m.transcriptHeader()
	area := m.transcriptArea()
	lines := make([]string, 0, len(header)+len(area.lines))
	lines = append(lines, header...)
	lines = append(lines, area.lines...)
	return lines, len(header)
}

// topicBarTint is how far the transcript header's band mixes from the page
// toward the theme accent. Enough to read as a title strip, faint enough to stay
// a surface rather than a banner.
const topicBarTint = 0.20

// topicHeaderLine renders the conversation's header as a full-width band: the
// topic caption on the left and, right-aligned, the "↓ new" jump marker while a
// jump is armed. The band spans transcriptWidth(), so the tail ends
// headerTailGutter cells short of the member column that follows instead of
// touching it. A long topic is truncated so the tail stays visible.
//
// The member count is deliberately not repeated here: a channel's count belongs
// to the member column's "ONLINE - N" heading, so the header stays the topic.
func (m *Model) topicHeaderLine(topic string) string {
	width := m.transcriptWidth()
	content := m.styles.Topic.Render(topic)
	if marker := m.unseenMarker(); marker != "" {
		// The tail owns the right end, one gutter short of the member column.
		available := width - headerTailGutter
		if available < 0 {
			available = 0
		}
		gap := available - lipgloss.Width(content) - lipgloss.Width(marker)
		if gap < 1 {
			// Spend the topic until the tail and one space fit. If the tail alone
			// is wider than the column there is nothing left to give, but the tail
			// is still emitted last so it is never the thing cut.
			maxTopic := available - lipgloss.Width(marker) - 1
			if maxTopic < 0 {
				maxTopic = 0
			}
			content = truncateLine(content, maxTopic)
			gap = available - lipgloss.Width(content) - lipgloss.Width(marker)
			if gap < 0 {
				gap = 0
			}
		}
		content += strings.Repeat(" ", gap) + marker
	} else {
		content = truncateLine(content, width)
	}
	// The band is the block's own background, so it covers the caption, the gap,
	// and the padding out to the column edge rather than stopping at the text.
	return m.styles.TopicBar.Width(width).Render(content)
}

// appendTranscriptTypingFooter adds the direct-message typing hint after the
// message rows: an ungrouped hint gets the peer's byline (a blank clock, the
// nick, and the separator), and a grouped one drops it. The dots are always on
// their own line at the body column. It is display-only: transcriptRowTexts
// keeps one entry per controller message, and the footer sits past every
// message index.
//
// The dots never extend the peer's message line. A message row is one entry
// that can hold several physical lines once the body wraps, so appending to it
// left the dots glued to the last word of the body. Qt's typingRow hides the
// avatar and the header on a grouped footer but keeps TypingDots on its own
// row, which is the shape reproduced here.
func (m *Model) appendTranscriptTypingFooter(area transcriptRows) transcriptRows {
	nick, grouped, show := m.ctrl.TranscriptTypingIndicator()
	if !show {
		return area
	}
	nickWidth := nickColumnWidth(m.ctrl.Messages())
	first, continuation := m.chatByline(nick, time.Time{}, false, nickWidth,
		bylineStyle{clock: m.styles.Time, nick: m.nickStyle(nick), rule: m.styles.MutedLine})
	if !grouped {
		// The peer did not speak in the current displayed minute, so the footer
		// reintroduces them: the blank clock, the nick, and the separator.
		area.lines = append(area.lines, first)
	}
	area.lines = append(area.lines, continuation+m.typingDots())
	return area
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
		rows = append(rows, message.Author+" "+m.ctrl.PlainIrcText(message.Body))
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
	if m.ctrl == nil || m.ctrl.ConsoleOpen() {
		return
	}
	area := m.transcriptArea()
	n := len(area.lines)
	height := m.transcriptRowsHeight()
	if height < 1 {
		return
	}
	maxOffset := n - height
	if maxOffset <= 0 {
		if direction < 0 {
			if m.requestOlderTranscriptPage() {
				m.transcriptFollowEnd = false
			}
		}
		if direction > 0 {
			m.transcriptScroll = 0
			m.setTranscriptFollowEnd(true)
			m.firstUnseenRow = -1
		}
		return
	}
	offset := m.transcriptScroll
	if m.transcriptFollowEnd {
		offset = 0
	}
	if direction < 0 && offset >= maxOffset {
		m.maybeRequestOlderTranscript()
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
	m.setTranscriptFollowEnd(offset == 0)
	if m.transcriptFollowEnd {
		m.firstUnseenRow = -1
	}
	start := n - height - offset
	if start < 0 {
		start = 0
	}
	m.transcriptCursor = area.rowAt(start + height - 1)
}

func (m *Model) maybeRequestOlderTranscript() {
	if m.requestOlderTranscriptPage() {
		m.transcriptFollowEnd = false
	}
}

// requestOlderTranscriptPage snapshots the viewport and asks for one older
// page. It reports whether a request was sent or is already in flight.
func (m *Model) requestOlderTranscriptPage() bool {
	if m.ctrl == nil || m.connectVisible() || m.ctrl.ConsoleOpen() || !m.ctrl.ChatHistoryEnabled() {
		return false
	}
	m.snapshotTranscriptAnchor()
	if m.ctrl.RequestOlderTranscriptHistory() {
		return true
	}
	if m.ctrl.OlderTranscriptHistoryInflight() {
		return true
	}
	m.clearTranscriptAnchor()
	return false
}

func (m *Model) setTranscriptFollowEnd(follow bool) {
	if follow && !m.transcriptFollowEnd && m.ctrl != nil {
		m.ctrl.ClearHistoryPageCapTail()
	}
	m.transcriptFollowEnd = follow
}

func (m *Model) clearTranscriptAnchor() {
	m.transcriptAnchorSequence = -1
	m.transcriptAnchorSpliceEpoch = -1
	m.firstUnseenSequenceAtAnchor = -1
}

func (m *Model) snapshotTranscriptAnchor() {
	if m.ctrl == nil {
		m.clearTranscriptAnchor()
		return
	}
	messages := m.ctrl.Messages()
	if len(messages) == 0 {
		m.clearTranscriptAnchor()
		return
	}
	row := 0
	if !m.transcriptFollowEnd {
		area := m.transcriptArea()
		n := len(area.lines)
		height := m.transcriptRowsHeight()
		if height >= 1 && n > height {
			offset := m.transcriptScroll
			start := n - height - offset
			if start < 0 {
				start = 0
			}
			row = area.rowAt(start)
			if row < 0 {
				row = 0
			}
		}
	}
	if row >= len(messages) {
		m.clearTranscriptAnchor()
		return
	}
	m.transcriptAnchorSequence = messages[row].Sequence
	m.transcriptAnchorSpliceEpoch = m.ctrl.TranscriptSpliceEpoch()
	if m.firstUnseenRow >= 0 && m.firstUnseenRow < len(messages) {
		m.firstUnseenSequenceAtAnchor = messages[m.firstUnseenRow].Sequence
	} else {
		m.firstUnseenSequenceAtAnchor = -1
	}
}

func (m *Model) restoreTranscriptAnchor() {
	if m.transcriptAnchorSequence < 0 || m.ctrl == nil {
		return
	}
	if m.transcriptFollowEnd {
		m.clearTranscriptAnchor()
		return
	}
	sequence := m.transcriptAnchorSequence
	m.clearTranscriptAnchor()
	messages := m.ctrl.Messages()
	for index, message := range messages {
		if message.Sequence == sequence {
			m.pinTranscriptToRow(index)
			return
		}
	}
}

// jumpTranscript jumps to the top or the bottom of the transcript.
func (m *Model) jumpTranscript(toEnd bool) {
	if m.ctrl == nil {
		return
	}
	area := m.transcriptArea()
	n := len(area.lines)
	height := m.transcriptRowsHeight()
	if height < 1 {
		height = 1
	}
	if toEnd {
		m.setTranscriptFollowEnd(true)
		m.transcriptScroll = 0
		m.transcriptCursor = m.transcriptRowTotal() - 1
		m.firstUnseenRow = -1
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
	area := m.transcriptArea()
	n := len(area.lines)
	height := m.transcriptRowsHeight()
	if height < 1 {
		height = 1
	}
	maxOffset := n - height
	if maxOffset < 0 {
		maxOffset = 0
	}
	if row < 0 || row >= area.count() {
		return
	}
	// The row area starts at row 0, past the pinned header, so the row's line
	// index needs no header offset. A row can be more than one line, so the
	// index comes from the row structure, not a stride.
	index := area.line(row)
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

// pinTranscriptToRow scrolls the viewport so row sits at the top and leaves
// follow-the-end, so the reader keeps their place. It mirrors
// TranscriptList.pinToUnread: a row that cannot fill a viewport, or an
// out-of-range row, falls back to following the end.
func (m *Model) pinTranscriptToRow(row int) {
	if m.ctrl == nil {
		return
	}
	area := m.transcriptArea()
	n := len(area.lines)
	height := m.transcriptRowsHeight()
	if height < 1 {
		height = 1
	}
	maxOffset := n - height
	if maxOffset <= 0 || row < 0 || row >= area.count() {
		m.setTranscriptFollowEnd(true)
		m.transcriptScroll = 0
		m.firstUnseenRow = -1
		return
	}
	start := area.line(row)
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

// noteTranscriptGrowth arms the first-new-row marker when rows arrive while the
// reader is scrolled up, and clears it while they follow the end or the
// transcript shrank. It mirrors TranscriptList.noteGrowth, which is driven by
// count changes rather than by the model's own notification.
func (m *Model) noteTranscriptGrowth() {
	count := m.transcriptRowTotal()
	if m.ctrl != nil && m.transcriptAnchorSequence >= 0 {
		spliceEpoch := m.ctrl.TranscriptSpliceEpoch()
		if spliceEpoch > m.transcriptAnchorSpliceEpoch {
			unseenSequence := m.firstUnseenSequenceAtAnchor
			m.restoreTranscriptAnchor()
			if unseenSequence >= 0 && !m.transcriptFollowEnd {
				resolved := false
				for index, message := range m.ctrl.Messages() {
					if message.Sequence == unseenSequence {
						m.firstUnseenRow = index
						resolved = true
						break
					}
				}
				if !resolved {
					m.firstUnseenRow = -1
				}
			}
			m.transcriptCount = count
			return
		}
	}
	switch {
	case count < m.transcriptCount:
		if m.firstUnseenRow >= count {
			m.firstUnseenRow = -1
		}
	case count > m.transcriptCount:
		if m.transcriptFollowEnd {
			m.firstUnseenRow = -1
		} else if m.firstUnseenRow < 0 {
			m.firstUnseenRow = m.transcriptCount
		}
	default:
		m.transcriptCount = count
		return
	}
	m.transcriptCount = count
}

// jumpArmed reports whether a jump-to-newest affordance applies: the reader is
// scrolled up and either rows arrived below them or the transcript carries a
// "New messages" mark. It mirrors TranscriptList.jumpArmed with canScrollDown
// standing in for !transcriptFollowEnd.
func (m *Model) jumpArmed() bool {
	if m.ctrl == nil || m.transcriptFollowEnd {
		return false
	}
	if m.firstUnseenRow >= 0 && m.firstUnseenRow < m.transcriptRowTotal() {
		return true
	}
	if m.ctrl.ConsoleOpen() {
		return false
	}
	return m.ctrl.UnreadMarkRow() >= 0
}

// jumpToUnseen lands on the first row that arrived while the reader was
// scrolled up, else on the newest row. It mirrors TranscriptList.jumpToUnseen.
func (m *Model) jumpToUnseen() {
	if !m.jumpArmed() {
		return
	}
	if m.firstUnseenRow >= 0 {
		row := m.firstUnseenRow
		m.firstUnseenRow = -1
		m.pinTranscriptToRow(row)
		return
	}
	m.jumpTranscript(true)
}

// unseenMarker is the transcript header's "↓ new" hint, shown while a jump is
// armed. It is the keyboard-first stand-in for the Qt list's floating jump
// button, which a terminal cannot place over the rows.
func (m *Model) unseenMarker() string {
	if !m.jumpArmed() {
		return ""
	}
	return m.styles.UnseenJump.Render("↓ new")
}

// renderMessageBody renders a message body with its IRC emphasis (bold,
// italic, underline) applied to the body portion only. The author/timestamp
// prefix is rendered by the caller, so a bold run can never leak into it.
// Control codes never reach the output: a body with no emphasis just renders
// its plain text, and an emphasized body is split into styled runs.
func (m *Model) renderMessageBody(body string, base lipgloss.Style) string {
	if m.ctrl == nil {
		return base.Render(body)
	}
	plain := m.ctrl.PlainIrcText(body)
	if !m.ctrl.HasIrcEmphasis(body) {
		return base.Render(plain)
	}
	var builder strings.Builder
	for _, run := range m.ctrl.EmphasizedRuns(body) {
		style := base
		if run.Bold {
			style = style.Bold(true)
		}
		if run.Italic {
			style = style.Italic(true)
		}
		if run.Underline {
			style = style.Underline(true)
		}
		builder.WriteString(style.Render(run.Text))
	}
	return builder.String()
}

// transcriptClockFormat is the byline timestamp's fixed HH:mm shape, and
// transcriptClockWidth its cell width, so the nick column never moves. It is
// read from MessageSnapshot.Time, which the controller already localizes to the
// clock's zone (the Qt displayTime / currentTranscriptMinute rule), so the
// render and continuesChatGroup compare display cells and must not re-localize.
const (
	transcriptClockFormat = "15:04"
	transcriptClockWidth  = len(transcriptClockFormat)
)

// transcriptSeparator is the vertical rule between the nick column and the
// message body: a space, the rule, and a space.
const transcriptSeparator = " │ "

// isChatRow reports whether a message renders with the time/nick column and a
// wrapped body. The action, notice, and event shapes keep their own single-line
// chrome.
func isChatRow(message controller.MessageSnapshot) bool {
	switch message.Kind {
	case "action", "event", "notice":
		return false
	}
	return true
}

// groupableChatRow reports whether a row may continue the run above it. A whois
// row carries the column but is a distinct shape in Qt, so it never groups.
func groupableChatRow(message controller.MessageSnapshot) bool {
	return isChatRow(message) && message.Kind != "whois"
}

// continuesChatGroup reports whether message continues previous as one group:
// the same nick in the same minute from the same origin. It mirrors
// continuesMessageGroup in OmaircWindow.qml. The later rows of a group blank
// the timestamp and nick so the run reads as one block.
func continuesChatGroup(previous *controller.MessageSnapshot, message controller.MessageSnapshot) bool {
	if !groupableChatRow(message) || previous == nil || !groupableChatRow(*previous) {
		return false
	}
	if message.Author == "" || previous.Author == "" || message.Origin != previous.Origin {
		return false
	}
	return message.Author == previous.Author &&
		message.Time.Format(transcriptClockFormat) == previous.Time.Format(transcriptClockFormat)
}

// nickColumnWidth is the width of the right-aligned nick column: the widest chat
// row's nick in one transcript, so every separator lands in the same column.
func nickColumnWidth(messages []controller.MessageSnapshot) int {
	width := 0
	for _, message := range messages {
		if !isChatRow(message) {
			continue
		}
		if cell := lipgloss.Width(message.Author); cell > width {
			width = cell
		}
	}
	return width
}

// bylineStyle is one byline's styling: the clock, the nick, and the separator
// runs. The plain chat row uses the shell palette; the mention row swaps in its
// wash so no cell of the band is left unwashed.
type bylineStyle struct {
	clock lipgloss.Style
	nick  lipgloss.Style
	rule  lipgloss.Style
}

// chatByline renders a chat row's byline: the dimmed timestamp, the
// right-aligned nick in its palette color, and the separator. It returns the
// opening run and the matching continuation prefix, both the same width, so a
// wrapped body and a grouped row line up under the first line. A grouped row and
// every wrapped continuation line carry the separator on its own, which keeps
// the rule an unbroken column down the transcript. A grouped or zero-time row
// blanks the clock; a grouped row blanks the nick too.
func (m *Model) chatByline(nick string, at time.Time, grouped bool, nickWidth int, style bylineStyle) (first, continuation string) {
	// The continuation prefix is the blank time and nick columns, then the
	// separator: the same rule the first line carries, so the column never
	// breaks across a wrap or a grouped run.
	continuation = strings.Repeat(" ", transcriptClockWidth+1+nickWidth) +
		style.rule.Render(transcriptSeparator)
	if grouped {
		return continuation, continuation
	}
	clock := strings.Repeat(" ", transcriptClockWidth)
	if !at.IsZero() {
		clock = style.clock.Render(at.Format(transcriptClockFormat))
	}
	nickColumn := strings.Repeat(" ", max(0, nickWidth-lipgloss.Width(nick))) +
		style.nick.Render(nick)
	first = clock + " " + nickColumn + style.rule.Render(transcriptSeparator)
	return first, continuation
}

// chatRow renders a plain chat row: the byline column, then the body wrapped
// under the body column.
func (m *Model) chatRow(message controller.MessageSnapshot, grouped bool, nickWidth int) string {
	first, continuation := m.chatByline(message.Author, message.Time, grouped, nickWidth,
		bylineStyle{clock: m.styles.Time, nick: m.nickStyle(message.Author), rule: m.styles.MutedLine})
	body := m.renderMessageBody(message.Body, lipgloss.NewStyle())
	return joinWrappedBody(first, continuation, body, m.transcriptWidth())
}

// joinWrappedBody hangs a wrapped body off a byline: the first wrapped line
// follows the byline, every later line is prefixed with the continuation (the
// blanked columns plus the separator), and a body wider than the column wraps
// instead of being truncated by renderColumn.
func joinWrappedBody(first, continuation, body string, width int) string {
	available := width - lipgloss.Width(continuation)
	if available < 1 {
		available = 1
	}
	lines := strings.Split(lipgloss.Wrap(body, available, ""), "\n")
	joined := first + lines[0]
	for _, line := range lines[1:] {
		joined += "\n" + continuation + line
	}
	return joined
}

// messageRow renders one transcript row. It measures the transcript's column
// geometry for the caller, so a direct call reads the same layout the view
// does. The current find match wins over every other style.
func (m *Model) messageRow(index int, message controller.MessageSnapshot) string {
	var messages []controller.MessageSnapshot
	if m.ctrl != nil && !m.ctrl.ConsoleOpen() {
		messages = m.ctrl.Messages()
	}
	return m.messageRowAt(messages, index, message, nickColumnWidth(messages))
}

// messageRowAt renders one row against a measured transcript. messages is the
// slice index points into, so the row reads the one above it for grouping, and
// nickWidth is the shared nick column, measured once for the whole render.
// Actions, events, and notices keep their own shape; chat rows carry the
// time/nick column and a wrapped body.
func (m *Model) messageRowAt(messages []controller.MessageSnapshot, index int, message controller.MessageSnapshot, nickWidth int) string {
	if m.findMatchAt(index) {
		return m.styles.FindMatch.Render(m.transcriptLine(message))
	}
	switch message.Kind {
	case "action":
		// "* nick body": an italic, muted action shape with the nick still
		// carrying its palette color.
		return m.styles.Action.Render("* ") +
			m.nickStyle(message.Author).Render(message.Author) +
			m.styles.Action.Render(" ") +
			m.renderMessageBody(message.Body, m.styles.Action)
	case "event":
		// A dim, centered server event, mirroring MessageRow.qml's centered
		// messageEvent.
		return m.eventRow(message.Body)
	case "notice":
		// "-nick- body": the notice marker stays muted while the nick keeps its
		// palette color.
		return m.styles.Notice.Render("-") +
			m.nickStyle(message.Author).Render(message.Author) +
			m.styles.Notice.Render("- ") +
			m.renderMessageBody(message.Body, m.styles.Notice)
	}
	var previous *controller.MessageSnapshot
	if index > 0 && index-1 < len(messages) {
		previous = &messages[index-1]
	}
	grouped := continuesChatGroup(previous, message)
	if message.Mentioned {
		return m.mentionRow(message, grouped, nickWidth)
	}
	return m.chatRow(message, grouped, nickWidth)
}

// nickStyle is the transcript byline style: the nick's fixed NickPalette color
// in bold, mirroring OmaircStyle.qml's nickColor and MessageHeader.qml's bold
// author label. Nick colors never move with the theme.
func (m *Model) nickStyle(nick string) lipgloss.Style {
	return lipgloss.NewStyle().Foreground(nickColor(nick)).Bold(true)
}

// mentionWash is the background of a highlighted row: the page color mixed
// toward the mention tint, mirroring MentionWash.qml's washColor. It is a
// one-off derivation from Styles.Colors, not a new style field.
func (m *Model) mentionWash() lipgloss.Style {
	return lipgloss.NewStyle().Background(
		mixColors(m.styles.Colors.Background, m.styles.Colors.Mention, 0.09))
}

// eventRow centers a dim server event within the transcript column. A body
// wider than the column is left to renderColumn's truncation.
func (m *Model) eventRow(body string) string {
	text := m.renderMessageBody(body, m.styles.Event)
	pad := (m.transcriptWidth() - lipgloss.Width(text)) / 2
	if pad < 1 {
		return text
	}
	return strings.Repeat(" ", pad) + text
}

// mentionRow renders a highlighted message as a full-width wash band: the same
// time/nick column as a plain chat row, the body in the mention tint, and every
// cell carrying the wash background so the tint spans the column like
// MentionWash.qml rather than stopping at the end of the text. Nick keeps its
// palette color and the body keeps its IRC emphasis on top of the wash.
func (m *Model) mentionRow(message controller.MessageSnapshot, grouped bool, nickWidth int) string {
	wash := m.mentionWash()
	colors := m.styles.Colors
	first, continuation := m.chatByline(message.Author, message.Time, grouped, nickWidth, bylineStyle{
		clock: wash.Foreground(colors.TextDim),
		nick:  wash.Foreground(nickColor(message.Author)).Bold(true),
		rule:  wash.Foreground(colors.TextDim),
	})
	body := m.renderMessageBody(message.Body, wash.Foreground(colors.Mention))
	width := m.transcriptWidth()
	// Every wrapped line is padded to the column so the band never stops short.
	lines := strings.Split(joinWrappedBody(first, continuation, body, width), "\n")
	for index, line := range lines {
		if pad := width - lipgloss.Width(line); pad > 0 {
			lines[index] = line + wash.Render(strings.Repeat(" ", pad))
		}
	}
	return strings.Join(lines, "\n")
}

// transcriptLine is the unstyled text of a message row, matching the find
// haystack. The body is read through PlainIrcText so control codes never enter
// the haystack.
func (m *Model) transcriptLine(message controller.MessageSnapshot) string {
	body := message.Body
	if m.ctrl != nil {
		body = m.ctrl.PlainIrcText(message.Body)
	}
	switch message.Kind {
	case "action":
		return "* " + message.Author + " " + body
	case "event":
		return body
	case "notice":
		return "-" + message.Author + "- " + body
	}
	return message.Time.Format("15:04") + " " + message.Author + " " + body
}
