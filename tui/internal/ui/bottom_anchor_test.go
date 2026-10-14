package ui

import (
	"strings"
	"testing"
)

// transcriptViewLines is the transcript column as painted, one entry per
// terminal row. transcriptView pads the row block to the viewport, so this is
// the place bottom gravity shows up. transcriptLines omits that pad.
func transcriptViewLines(m *Model) []string {
	rendered := strings.TrimSuffix(m.transcriptView(m.transcriptHeight()), "\n")
	if rendered == "" {
		return nil
	}
	return strings.Split(rendered, "\n")
}

// assertBottomAnchored requires a short transcript: blank rows between the
// pinned header and the first message, and a real row on the last viewport
// line so the block sits on the composer.
func assertBottomAnchored(t *testing.T, m *Model) {
	t.Helper()
	header := len(m.transcriptHeader())
	lines := transcriptViewLines(m)
	if len(lines) != m.transcriptHeight() {
		t.Fatalf("transcript view lines = %d, want height %d", len(lines), m.transcriptHeight())
	}
	if header <= 0 || header >= len(lines) {
		t.Fatalf("header lines = %d in a %d-line view", header, len(lines))
	}
	if got := plainLine(lines[header]); got != "" {
		t.Fatalf("first line under the header = %q, want blank space above a short transcript", got)
	}
	if got := plainLine(lines[len(lines)-1]); got == "" {
		t.Fatal("last viewport line is blank; the newest row should sit on the composer")
	}
}

func TestShortTranscriptSitsOnTheComposer(t *testing.T) {
	m := resizeModel(t, seededModel(t), 118, 60)
	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#help")
	})
	if got := m.ctrl.SelectedTarget(); got != "#help" {
		t.Fatalf("selected = %q, want #help", got)
	}
	area := m.transcriptArea()
	viewport := m.transcriptRowsHeight()
	if area.count() == 0 || len(area.lines) == 0 {
		t.Fatal("#help should have rows")
	}
	if len(area.lines) >= viewport {
		t.Fatalf("#help has %d lines in a %d-line viewport; the proof needs a short transcript",
			len(area.lines), viewport)
	}
	assertBottomAnchored(t, m)
}

func TestShortStatusSitsOnTheComposer(t *testing.T) {
	m := resizeModel(t, seededModel(t), 118, 80)
	m.toggleStatus()
	if !m.ctrl.ConsoleOpen() {
		t.Fatal("Status should be the open pane")
	}
	area := m.transcriptArea()
	viewport := m.transcriptRowsHeight()
	if len(area.lines) == 0 {
		t.Fatal("Status should have lines")
	}
	if len(area.lines) >= viewport {
		t.Fatalf("Status has %d lines in a %d-line viewport; the proof needs a short console",
			len(area.lines), viewport)
	}
	assertBottomAnchored(t, m)
}

// TestLongTranscriptStaysFilled locks the overflow path: a transcript taller
// than the viewport still starts at the header with a real row, and the tail
// stays pinned. padAbove must not insert slack into a full window.
func TestLongTranscriptStaysFilled(t *testing.T) {
	m := resizeModel(t, seededModel(t), 118, 14)
	header := len(m.transcriptHeader())
	area := m.transcriptArea()
	viewport := m.transcriptRowsHeight()
	if len(area.lines) <= viewport {
		t.Fatalf("row area of %d lines fits a %d-line viewport; the proof needs an overflow",
			len(area.lines), viewport)
	}
	if !m.transcriptFollowEnd {
		t.Fatal("the seeded view starts pinned to the tail")
	}
	lines := transcriptViewLines(m)
	if header >= len(lines) {
		t.Fatalf("header lines = %d in a %d-line view", header, len(lines))
	}
	if got := plainLine(lines[header]); got == "" {
		t.Fatal("an overflowing transcript must not pad above the first visible row")
	}
	if got := plainLine(lines[len(lines)-1]); got == "" {
		t.Fatal("an overflowing transcript must not end on a blank line")
	}
}
