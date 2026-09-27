package ui

import (
	"fmt"
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
		lines = append(lines, m.topicHeaderLine(topic))
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
		lines = m.appendTranscriptTypingFooter(lines, headerCount)
	}
	return lines, headerCount
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
// per controller message, and the footer sits past every message index.
func (m *Model) appendTranscriptTypingFooter(lines []string, headerCount int) []string {
	nick, grouped, show := m.ctrl.TranscriptTypingIndicator()
	if !show {
		return lines
	}
	dots := m.styles.MutedLine.Render(" ...")
	if grouped {
		if len(lines) <= headerCount {
			return lines
		}
		// Keep the dots on screen even when the peer's line is at the column
		// edge: renderColumn would otherwise truncate them away.
		available := m.transcriptWidth() - lipgloss.Width(dots)
		if available < 0 {
			available = 0
		}
		lines[len(lines)-1] = truncateLine(lines[len(lines)-1], available) + dots
		return lines
	}
	lines = append(lines, m.styles.MutedLine.Render(nick))
	lines = append(lines, m.styles.MutedLine.Render("   ..."))
	return lines
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
// rather than stopping at the end of the text; the padding is part of the line
// so it survives renderColumn. The nick keeps its palette color and the body
// keeps its IRC emphasis on top of the wash.
func (m *Model) mentionRow(message controller.MessageSnapshot) string {
	wash := m.mentionWash()
	colors := m.styles.Colors
	line := wash.Foreground(colors.TextDim).Render(message.Time.Format("15:04")+" ") +
		wash.Foreground(nickColor(message.Author)).Bold(true).Render(message.Author+" ") +
		m.renderMessageBody(message.Body, wash.Foreground(colors.Mention))
	if width := m.transcriptWidth(); lipgloss.Width(line) < width {
		line += wash.Render(strings.Repeat(" ", width-lipgloss.Width(line)))
	}
	return line
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
