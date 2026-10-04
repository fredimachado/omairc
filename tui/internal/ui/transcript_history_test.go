package ui

import (
	"testing"

	tea "charm.land/bubbletea/v2"
)

func TestSnapshotTranscriptAnchorRecordsTopRowSequence(t *testing.T) {
	m := seededModel(t)
	m = resizeModel(t, m, 118, 8)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyHome, Mod: tea.ModCtrl})
	if m.transcriptFollowEnd {
		t.Fatal("Ctrl+Home must leave follow-the-end")
	}
	messages := m.ctrl.Messages()
	if len(messages) == 0 {
		t.Fatal("seeded transcript must have messages")
	}
	m.snapshotTranscriptAnchor()
	if m.transcriptAnchorSequence != messages[0].Sequence {
		t.Fatalf("anchor sequence = %d, want top row %d", m.transcriptAnchorSequence, messages[0].Sequence)
	}
	scrollBefore := m.transcriptScroll
	m.restoreTranscriptAnchor()
	if m.transcriptAnchorSequence != -1 {
		t.Fatal("restore must clear the anchor")
	}
	if m.transcriptScroll != scrollBefore {
		t.Fatalf("scroll = %d, want unchanged %d", m.transcriptScroll, scrollBefore)
	}
}

func TestNewModelAnchorInactive(t *testing.T) {
	m := seededModel(t)
	if m.transcriptAnchorSequence != -1 {
		t.Fatalf("transcriptAnchorSequence = %d, want -1", m.transcriptAnchorSequence)
	}
}
