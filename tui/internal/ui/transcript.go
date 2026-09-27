package ui

import (
	"fmt"
	"sort"
	"strings"

	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// peopleCountGutter is the blank separation between the transcript's
// right-aligned "N PEOPLE" count and the member column joined immediately
// after it. Without it the two columns' text runs together in the grid
// ("12 PEOPLEONLINE - 12"), so the header is built one gutter short of
// transcriptWidth() and renderColumn pads the rest.
const peopleCountGutter = 2

// transcriptRowGap is the number of blank lines rendered between two adjacent
// transcript rows, so messages read as separate blocks instead of a dense wall
// of text. A terminal cell has no line height to tune, so the gap is blank
// rows; it is the one lever for inter-line breathing room.
const transcriptRowGap = 1

// mentionWashPad is the number of washed rows a highlighted message carries
// above and below its text. It goes on the wash style as lipgloss
// PaddingTop/PaddingBottom rather than as separate blank rows: vertical padding
// renders inside the block, so the tint covers the padding rows, which a plain
// blank row cannot do. It also means a highlighted row is taller than a plain
// one, so rows do not share a stride and every row-to-line lookup is an index.
const mentionWashPad = 1

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
// the line index and the height of every row. A highlighted row carries
// mentionWashPad washed rows above and below its text, so rows are not a uniform
// height; find, copy, and the scroll offsets all resolve a row through this
// structure instead of a stride.
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
// row. A line inside a row's wash padding or in a gap between rows belongs to
// the row it follows.
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

// append adds one row block, separated from the previous row by the row gap.
// The row's height comes from the rendered block itself, so the index can never
// drift from the lines that are actually emitted.
func (r *transcriptRows) append(block string) {
	if len(r.starts) > 0 {
		r.lines = append(r.lines, "")
	}
	lines := strings.Split(block, "\n")
	r.starts = append(r.starts, len(r.lines))
	r.heights = append(r.heights, len(lines))
	r.lines = append(r.lines, lines...)
}

// transcriptRowArea builds the scrolling row area: one block per message or
// Status line, each preceded by the row gap. It does not include the pinned
// header, so its first row starts at line 0.
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
	for index, message := range m.ctrl.Messages() {
		block := m.messageRow(index, message)
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
// trailing blank, or nothing when the conversation has no topic. It stays put
// while the rows scroll under it.
func (m *Model) transcriptHeader() []string {
	if m.ctrl == nil {
		return nil
	}
	topic := m.ctrl.Topic()
	if topic == "" {
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

// topicHeaderLine renders the conversation's header: the muted topic caption
// on the left and, for a channel, a right-aligned, filled "N PEOPLE" chip
// mirroring ConversationColumn.qml's people control. The line is sized to
// transcriptWidth() minus peopleCountGutter so the chip sits at the right edge
// but never touches the member column that follows. A long topic is truncated
// so the chip stays visible; a direct message is just the caption, with no
// count.
func (m *Model) topicHeaderLine(topic string) string {
	topicRendered := m.styles.Topic.Render(topic)
	if !m.ctrl.IsChannel() {
		return topicRendered
	}
	countRendered := m.peopleChip(m.ctrl.PeopleCount())
	width := m.transcriptWidth() - peopleCountGutter
	if width < 0 {
		width = 0
	}
	gap := width - lipgloss.Width(topicRendered) - lipgloss.Width(countRendered)
	if gap < 1 {
		// Spend the topic until the count and one space fit. If the count
		// alone is wider than the column there is nothing left to give, but
		// the count is still emitted last so it is never the thing cut.
		maxTopic := width - lipgloss.Width(countRendered) - 1
		if maxTopic < 0 {
			maxTopic = 0
		}
		topicRendered = truncateLine(topicRendered, maxTopic)
		gap = width - lipgloss.Width(topicRendered) - lipgloss.Width(countRendered)
		if gap < 0 {
			gap = 0
		}
	}
	return topicRendered + strings.Repeat(" ", gap) + countRendered
}

// appendTranscriptTypingFooter adds the direct-message typing hint after the
// message rows. A grouped hint belongs to the peer's last live chat row, so it
// extends that rendered line; an ungrouped hint gets the peer's header line and
// an indented dots line. It is display-only: transcriptRowTexts keeps one entry
// per controller message, and the footer sits past every message index. A
// highlighted last row owns wash padding below its text, so the grouped hint
// cannot extend it cleanly and falls back to the ungrouped shape.
func (m *Model) appendTranscriptTypingFooter(area transcriptRows) transcriptRows {
	nick, grouped, show := m.ctrl.TranscriptTypingIndicator()
	if !show {
		return area
	}
	dots := m.styles.MutedLine.Render(" ...")
	last := len(area.lines) - 1
	if grouped && last >= 0 && !m.lastRowCarriesWashPad(area) {
		// Keep the dots on screen even when the peer's line is at the column
		// edge: renderColumn would otherwise truncate them away.
		available := m.transcriptWidth() - lipgloss.Width(dots)
		if available < 0 {
			available = 0
		}
		area.lines[last] = truncateLine(area.lines[last], available) + dots
		return area
	}
	area.lines = append(area.lines, m.styles.MutedLine.Render(nick))
	area.lines = append(area.lines, m.styles.MutedLine.Render("   ..."))
	return area
}

// lastRowCarriesWashPad reports whether the last row ends in the wash columns a
// highlighted message adds below its text. The typing hint is built to extend a
// plain text row, so it must not land on a wash padding line. A washed row is
// mentionWashPad rows above and below its single text line; a "New messages"
// boundary also makes a row taller than one line, so the height test is the
// wash height rather than "more than one line".
func (m *Model) lastRowCarriesWashPad(area transcriptRows) bool {
	if area.count() == 0 {
		return false
	}
	return area.heights[area.count()-1] >= 1+2*mentionWashPad
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
	if m.ctrl == nil {
		return
	}
	area := m.transcriptArea()
	n := len(area.lines)
	height := m.bodyHeight() - len(m.transcriptHeader())
	if height < 1 {
		return
	}
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
	start := n - height - offset
	if start < 0 {
		start = 0
	}
	m.transcriptCursor = area.rowAt(start + height - 1)
}

// jumpTranscript jumps to the top or the bottom of the transcript.
func (m *Model) jumpTranscript(toEnd bool) {
	if m.ctrl == nil {
		return
	}
	area := m.transcriptArea()
	n := len(area.lines)
	height := m.bodyHeight() - len(m.transcriptHeader())
	if height < 1 {
		height = 1
	}
	if toEnd {
		m.transcriptFollowEnd = true
		m.transcriptScroll = 0
		m.transcriptCursor = m.transcriptRowTotal() - 1
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
	height := m.bodyHeight() - len(m.transcriptHeader())
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
	// index needs no header offset. A highlighted row is taller than a plain
	// one, so the index comes from the row structure, not a stride.
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
	height := m.bodyHeight() - len(m.transcriptHeader())
	if height < 1 {
		height = 1
	}
	maxOffset := n - height
	if maxOffset <= 0 || row < 0 || row >= area.count() {
		m.transcriptFollowEnd = true
		m.transcriptScroll = 0
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

// messageRow renders one transcript line. Actions, events, and notices get
// their own shape; a mention is washed as a full-width row. Every chat byline
// leads with a dimmed timestamp and the author in its fixed palette color,
// mirroring MessageHeader.qml. The body carries its own IRC emphasis. The
// current find match wins over every other style. It stays one line per
// message: find/copy index the rows and the typing footer extends the last
// row.
func (m *Model) messageRow(index int, message controller.MessageSnapshot) string {
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
	if message.Mentioned {
		return m.mentionRow(message)
	}
	return m.styles.Time.Render(message.Time.Format("15:04")) + " " +
		m.nickStyle(message.Author).Render(message.Author) + " " +
		m.renderMessageBody(message.Body, lipgloss.NewStyle())
}

// nickStyle is the transcript byline style: the nick's fixed NickPalette color
// in bold, mirroring OmaircStyle.qml's nickColor and MessageHeader.qml's bold
// author label. Nick colors never move with the theme.
func (m *Model) nickStyle(nick string) lipgloss.Style {
	return lipgloss.NewStyle().Foreground(nickColor(nick)).Bold(true)
}

// peopleChip renders the "N PEOPLE" label as a filled chip: the F1 badge chrome
// (bold ink on the raised surface) plus a one-cell pad on each side, mirroring
// ConversationColumn.qml's peopleButton. The pad is part of the rendered width,
// so topicHeaderLine's gutter math stays exact.
func (m *Model) peopleChip(count int) string {
	return m.styles.Badge.Padding(0, 1).Render(fmt.Sprintf("%d PEOPLE", count))
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

// mentionRow renders a highlighted message as a full-width band. Every cell
// carries the wash background so the tint spans the column like MentionWash.qml
// rather than stopping at the end of the text; the horizontal padding is part
// of the line so it survives renderColumn. Nick keeps its palette color and the
// body keeps its IRC emphasis on top of the wash.
//
// PaddingTop/PaddingBottom give the band its breathing room, with the wash
// covering the padding rows. That makes a highlighted row taller than a plain
// one, which transcriptRows tracks; the row is no longer one line, so find and
// copy resolve it through that index rather than assuming a stride.
func (m *Model) mentionRow(message controller.MessageSnapshot) string {
	wash := m.mentionWash()
	colors := m.styles.Colors
	line := wash.Foreground(colors.TextDim).Render(message.Time.Format("15:04")+" ") +
		wash.Foreground(nickColor(message.Author)).Bold(true).Render(message.Author+" ") +
		m.renderMessageBody(message.Body, wash.Foreground(colors.Mention))
	if width := m.transcriptWidth(); lipgloss.Width(line) < width {
		line += wash.Render(strings.Repeat(" ", width-lipgloss.Width(line)))
	}
	return wash.Padding(mentionWashPad, 0).Render(line)
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
