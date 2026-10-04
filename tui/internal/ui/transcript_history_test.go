package ui

import (
	"testing"

	tea "charm.land/bubbletea/v2"
)

func TestSnapshotTranscriptAnchorPreservesDistanceFromEnd(t *testing.T) {
	m := seededModel(t)
	m = resizeModel(t, m, 118, 8)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyHome, Mod: tea.ModCtrl})
	if m.transcriptFollowEnd {
		t.Fatal("Ctrl+Home must leave follow-the-end")
	}
	m.snapshotTranscriptAnchor()
	want := m.transcriptRowTotal() - m.transcriptCursor
	if m.transcriptAnchorFromEnd != want {
		t.Fatalf("anchor from end = %d, want %d", m.transcriptAnchorFromEnd, want)
	}
	scrollBefore := m.transcriptScroll
	m.restoreTranscriptAnchor()
	if m.transcriptAnchorFromEnd != -1 {
		t.Fatal("restore must clear the anchor")
	}
	if m.transcriptScroll != scrollBefore {
		t.Fatalf("scroll = %d, want unchanged %d", m.transcriptScroll, scrollBefore)
	}
}
