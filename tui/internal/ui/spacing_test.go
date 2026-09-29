package ui

// This file covers the transcript and sidebar rhythm: transcript rows render
// edge to edge with no blank line between them, a highlighted row stays a
// single washed line, the topic header stays pinned at the top of the column
// while the rows scroll, and the sidebar's groups carry a blank row between
// them.

import (
	"strings"
	"testing"
	"time"

	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// plainLine is one rendered line with its ANSI stripped and its trailing
// padding removed, so a test can compare structure instead of palette.
func plainLine(line string) string {
	return strings.TrimRight(ansiPattern.ReplaceAllString(line, ""), " ")
}

// TestTranscriptRowsArePacked pins that rows render edge to edge: every row
// starts where the previous row ended, and the row index points at the lines
// that were actually emitted.
func TestTranscriptRowsArePacked(t *testing.T) {
	m := seededModel(t)
	area := m.transcriptArea()
	if got, want := area.count(), m.transcriptRowTotal(); got != want {
		t.Fatalf("row count = %d, want transcriptRowTotal() %d", got, want)
	}
	if area.count() < 3 {
		t.Fatalf("the seeded transcript has %d rows, too few to prove packing", area.count())
	}
	allLines, headerCount := m.transcriptLines()
	if len(area.lines) != len(allLines)-headerCount {
		t.Fatal("the row area and the rendered rows disagree on length")
	}
	if area.line(0) != 0 {
		t.Fatalf("row 0 starts at line %d, want 0 (the header is not part of the area)", area.line(0))
	}
	for row := 1; row < area.count(); row++ {
		if start := area.line(row); start != area.endOf(row-1) {
			t.Fatalf("row %d starts at %d, want %d: rows must be packed, not spaced",
				row, start, area.endOf(row-1))
		}
	}
}

// TestMentionRowsStaySingleLine pins that a highlighted message is one washed
// line, not a padded block: the tint spans the column but adds no rows above or
// below it.
func TestMentionRowsStaySingleLine(t *testing.T) {
	m := seededModel(t)
	msg := controller.MessageSnapshot{
		Author:    "mira",
		Kind:      "message",
		Body:      "hello there",
		Time:      time.Date(2026, 9, 27, 12, 0, 0, 0, time.UTC),
		Mentioned: true,
	}
	if !isMentionRow(msg) {
		t.Fatal("a plain Mentioned message must be a mention row")
	}

	row := m.messageRow(0, msg)
	if strings.Count(row, "\n") != 0 {
		t.Fatalf("mention row = %q, want a single line", row)
	}
	if got, want := lipgloss.Width(row), m.transcriptWidth(); got != want {
		t.Fatalf("mention row width = %d, want the column width %d", got, want)
	}
	if !strings.Contains(row, "hello there") {
		t.Fatalf("mention row = %q, want the message body", row)
	}
}

// TestNonMentionKindsStaySingleLine pins that actions, notices, and events also
// stay one line, so no kind gains a blank row.
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

// TestFindRevealsMatchingRow pins that find sets the cursor to the matching row
// and scrolls that row into view.
func TestFindRevealsMatchingRow(t *testing.T) {
	m := seededModel(t)
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
	height := m.transcriptRowsHeight()
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
	if m.transcriptHeight() <= headerCount {
		t.Fatalf("transcript height %d leaves no room for rows past the %d-line header",
			m.transcriptHeight(), headerCount)
	}
	// The seeded channel has far more rows than the short viewport shows.
	if area := m.transcriptArea(); len(area.lines) <= m.transcriptHeight()-headerCount {
		t.Fatalf("row area of %d lines fits the transcript; the proof needs an overflow", len(area.lines))
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
