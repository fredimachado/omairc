package ui

// This file covers the inter-line spacing: transcript rows are separated by a
// blank row, a highlighted row carries wash padding above and below its text,
// the topic header stays pinned at the top of the column while those rows
// scroll, and the sidebar's groups carry a blank row between them. A terminal
// cell has no line height, so a blank row is the only spacing lever.

import (
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// plainLine is one rendered line with its ANSI stripped and its trailing
// padding removed, so a test can compare structure instead of palette.
func plainLine(line string) string {
	return strings.TrimRight(ansiPattern.ReplaceAllString(line, ""), " ")
}

// TestTranscriptRowsAreSpaced pins the row gap and the index that goes with it:
// every row starts one blank line after the previous row ends, and the row
// structure points at the lines that were actually emitted.
func TestTranscriptRowsAreSpaced(t *testing.T) {
	m := seededModel(t)
	if transcriptRowGap < 1 {
		t.Fatal("the transcript must render a gap between rows")
	}

	area := m.transcriptArea()
	if got, want := area.count(), m.transcriptRowTotal(); got != want {
		t.Fatalf("row count = %d, want transcriptRowTotal() %d", got, want)
	}
	if area.count() < 3 {
		t.Fatalf("the seeded transcript has %d rows, too few to prove spacing", area.count())
	}
	allLines, headerCount := m.transcriptLines()
	if len(area.lines) != len(allLines)-headerCount {
		t.Fatal("the row area and the rendered rows disagree on length")
	}

	for row := 0; row < area.count(); row++ {
		start := area.line(row)
		height := area.heights[row]
		if height < 1 {
			t.Fatalf("row %d has height %d", row, height)
		}
		if start+height > len(area.lines) {
			t.Fatalf("row %d spans past the %d rendered lines", row, len(area.lines))
		}
		if row == 0 {
			if start != 0 {
				t.Fatalf("row 0 starts at line %d, want 0 (the header is not part of the area)", start)
			}
			continue
		}
		// The gap is the single line between the previous row's end and this
		// row's start.
		if start-height != area.endOf(row-1) {
			t.Fatalf("row %d starts at %d, want one gap line after row %d", row, start, row-1)
		}
	}
}

// TestMentionRowsCarryWashPadding pins that a highlighted message is a taller
// block: its text plus mentionWashPad washed rows above and below, every one of
// them painted with the wash fill so the band has breathing room instead of
// striping the column with the window color.
func TestMentionRowsCarryWashPadding(t *testing.T) {
	m := seededModel(t)
	if mentionWashPad < 1 {
		t.Fatal("the mention wash must have vertical padding")
	}
	at := time.Date(2026, 9, 27, 12, 0, 0, 0, time.UTC)
	msg := controller.MessageSnapshot{
		Author:    "mira",
		Kind:      "message",
		Body:      "hello there",
		Time:      at,
		Mentioned: true,
	}
	if !isMentionRow(msg) {
		t.Fatal("a plain Mentioned message must be a mention row")
	}

	row := m.messageRow(0, msg)
	lines := strings.Split(row, "\n")
	if got, want := len(lines), 1+2*mentionWashPad; got != want {
		t.Fatalf("mention block = %d lines, want %d", got, want)
	}
	wash := m.mentionWash()
	// Every line of the block spans the column and carries the wash: the padding
	// rows differ from the text row only in their content, not their background.
	for index, line := range lines {
		if width := lipgloss.Width(line); width != m.transcriptWidth() {
			t.Fatalf("mention line %d width = %d, want the column width %d",
				index, width, m.transcriptWidth())
		}
		if index == mentionWashPad {
			if plainLine(line) == "" {
				t.Fatalf("mention text line %d is blank", index)
			}
			continue
		}
		if plainLine(line) != "" {
			t.Fatalf("mention padding line %d = %q, want blank", index, plainLine(line))
		}
	}
	if !strings.Contains(lines[mentionWashPad], "hello there") {
		t.Fatalf("mention text is not on the middle line: %q", lines[mentionWashPad])
	}
	// The padding rows are a solid fill: the wash style rendered over spaces.
	if want := wash.Render(strings.Repeat(" ", m.transcriptWidth())); lines[0] != want {
		t.Fatalf("mention top padding = %q, want the wash fill %q", lines[0], want)
	}
}

// TestNonMentionKindsStaySingleLine pins that the padding is the mention shape
// only: actions, notices, events, and plain chat stay one line, so the taller
// block is what a highlight costs.
func TestNonMentionKindsStaySingleLine(t *testing.T) {
	m := seededModel(t)
	at := time.Date(2026, 9, 27, 12, 0, 0, 0, time.UTC)
	for _, msg := range []controller.MessageSnapshot{
		{Author: "mira", Kind: "message", Body: "hi", Time: at},
		{Author: "mira", Kind: "action", Body: "waves", Time: at, Mentioned: true},
		{Author: "mira", Kind: "notice", Body: "hi", Time: at, Mentioned: true},
		{Kind: "event", Body: "mira joined", Time: at, Mentioned: true},
	} {
		if isMentionRow(msg) {
			t.Fatalf("kind %q must not be a mention row", msg.Kind)
		}
		if got := strings.Count(m.messageRow(0, msg), "\n"); got != 0 {
			t.Fatalf("kind %q rendered %d extra lines, want a single line", msg.Kind, got)
		}
	}
}

// TestFindRevealsAcrossWashPadding pins that find still lands on the right row
// once rows have different heights: the cursor is set to the matching row, and
// the reveal scrolls that row's block into view.
func TestFindRevealsAcrossWashPadding(t *testing.T) {
	m := seededModel(t)
	// Make a mid-transcript row a mention so a taller block sits above the
	// target and a stride-based lookup would drift.
	messages := m.ctrl.Messages()
	if len(messages) < 4 {
		t.Fatal("the seeded transcript is too short for this proof")
	}
	m.composer.SetValue("")

	m = press(t, m, ctrlKey('f'))
	if !m.find.active {
		t.Fatal("Ctrl+F must open find")
	}
	m.composer.SetValue("minimal")
	m.advanceFind(true)
	if m.find.index < 0 {
		t.Fatal("find must locate the seeded 'minimal' row")
	}

	area := m.transcriptArea()
	row := m.find.index
	if row >= area.count() {
		t.Fatalf("find index %d outside the %d rows", row, area.count())
	}
	// The revealed row must be inside the visible window.
	if m.transcriptFollowEnd {
		t.Fatal("revealing a match must leave follow-the-end")
	}
	height := m.bodyHeight() - len(m.transcriptHeader())
	start := len(area.lines) - height - m.transcriptScroll
	if rowStart := area.line(row); rowStart < start || rowStart >= start+height {
		t.Fatalf("matched row %d at line %d is outside the window [%d,%d)",
			row, rowStart, start, start+height)
	}
}

// TestTranscriptHeaderStaysPinned pins that the topic block is not part of the
// scrolled rows: at a size where the transcript overflows, the column still
// shows the topic and its people count while pinned to the tail.
func TestTranscriptHeaderStaysPinned(t *testing.T) {
	m := seededModel(t)
	m = resizeModel(t, m, 118, 14)

	header := m.transcriptHeader()
	headerCount := len(header)
	if headerCount == 0 {
		t.Fatal("a channel transcript must have a pinned header")
	}
	if m.bodyHeight() <= headerCount {
		t.Fatalf("body height %d leaves no room for rows past the %d-line header",
			m.bodyHeight(), headerCount)
	}
	// The seeded channel has far more rows than the short viewport shows.
	if area := m.transcriptArea(); len(area.lines) <= m.bodyHeight()-headerCount {
		t.Fatalf("row area of %d lines fits the body; the proof needs an overflow", len(area.lines))
	}

	content := m.View().Content
	if !m.transcriptFollowEnd {
		t.Fatal("the seeded view starts pinned to the tail")
	}
	for _, want := range []string{"A cozy corner for Omarchy users and builders.", "12 PEOPLE"} {
		if !strings.Contains(content, want) {
			t.Fatalf("pinned header missing %q:\n%s", want, content)
		}
	}
	// The header is the column's first rendered row, above every message.
	firstRow := strings.SplitN(content, "\n", 2)[0]
	if !strings.Contains(firstRow, "A cozy corner for Omarchy users and builders.") {
		t.Fatalf("first rendered row is not the topic header: %q", firstRow)
	}
}

// TestGroupedTypingFooterFallsBackOnWashedRow pins the edge the wash padding
// creates: the typing hint normally extends the peer's last chat line, but a
// highlighted last row owns washed padding below its text, so the hint would be
// written onto a fill row. It must fall back to the ungrouped shape instead.
func TestGroupedTypingFooterFallsBackOnWashedRow(t *testing.T) {
	// A clock in the seeded anna DM row's own minute makes the typing indicator
	// group under it, and that row is a mention.
	m := groupedTypingMentionModel(t)
	nick, grouped, show := m.ctrl.TranscriptTypingIndicator()
	if nick != "anna" || !grouped || !show {
		t.Fatalf("indicator = (%q,%v,%v), want the grouped anna footer", nick, grouped, show)
	}

	area := m.transcriptArea()
	if got := area.count(); got != 1 {
		t.Fatalf("the anna DM has %d rows, want the single seeded mention", got)
	}
	if area.heights[0] <= 1 {
		t.Fatal("the seeded last row must be a mention to reach this fallback")
	}
	// The hint lines sit past the row's wash padding, not inside it.
	if got, want := len(area.lines), area.end()+2; got != want {
		t.Fatalf("row area = %d lines, want %d past the wash padding", got, want)
	}
	for index := area.end(); index < len(area.lines); index++ {
		line := area.lines[index]
		if !strings.Contains(line, "anna") && !strings.Contains(line, "...") {
			t.Fatalf("footer line %d = %q, want the ungrouped nick or dots", index, line)
		}
	}
	// No wash fill may be printed onto a footer line.
	wash := backgroundParams(m.mentionWash().GetBackground())
	for index := area.end(); index < len(area.lines); index++ {
		if strings.Contains(area.lines[index], wash) {
			t.Fatalf("footer line %d carries the wash fill: %q", index, area.lines[index])
		}
	}
}

// groupedTypingMentionModel is the seeded demo with the clock pinned inside the
// anna DM's only row's minute, so the typing hint groups under that row, and
// that row selected. The row is a mention, so it is taller than one line.
func groupedTypingMentionModel(t *testing.T) *Model {
	t.Helper()
	ctrl := controller.New()
	ctrl.SetClock(session.NewFakeClock(time.Date(2026, 9, 12, 10, 12, 0, 0, time.UTC)))
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := New(ctrl, nil)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)
	for _, row := range ctrl.Conversations() {
		if row.Conversation == "anna" {
			ctrl.SelectConversationByID(row.ConversationID)
		}
	}
	m.afterSelectionChange("")
	return m
}

// TestSidebarGroupsHaveABlankRow pins the sidebar rhythm: a blank row separates
// the CHANNELS and DIRECT MESSAGES groups, and a blank row precedes the rule
// that separates two networks.
func TestSidebarGroupsHaveABlankRow(t *testing.T) {
	m := seededModel(t)
	const width = 30
	rows := strings.Split(m.sidebarView(width, m.bodyHeight()), "\n")
	plain := make([]string, len(rows))
	for index, row := range rows {
		plain[index] = plainLine(row)
	}

	heading := -1
	for index, row := range plain {
		if strings.Contains(row, "DIRECT MESSAGES") {
			heading = index
			break
		}
	}
	if heading < 0 {
		t.Fatalf("no DIRECT MESSAGES heading in the sidebar:\n%s", strings.Join(plain, "\n"))
	}
	if heading == 0 || plain[heading-1] != "" {
		t.Fatalf("no blank row above the DIRECT MESSAGES heading at line %d:\n%s",
			heading, strings.Join(plain, "\n"))
	}

	rule := -1
	for index, row := range plain {
		if row == strings.Repeat("─", width) {
			rule = index
			break
		}
	}
	if rule < 0 {
		t.Fatalf("no network separator in the sidebar:\n%s", strings.Join(plain, "\n"))
	}
	if rule == 0 || plain[rule-1] != "" {
		t.Fatalf("no blank row above the network rule at line %d:\n%s",
			rule, strings.Join(plain, "\n"))
	}
}
