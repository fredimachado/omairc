package ui

import (
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// The read marker may advance only for a reader who follows the end. What the
// shell reports is the reader's position and nothing else.

func TestPendingRestoreDoesNotReportFollowEnd(t *testing.T) {
	m, _ := plantUnloadedScrollPlace(t)
	if !m.restoringNow() {
		t.Fatalf("reader = %T, want restoring while the saved line is not loaded", m.reader)
	}
	if m.reader.reportsFollowEnd() {
		t.Fatal("a pending restore must not report follow-end: the read marker would jump past unread backlog")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnd, Mod: tea.ModCtrl})
	if m.restoringNow() {
		t.Fatal("Ctrl+End must retire the pending restore")
	}
	if !m.reader.reportsFollowEnd() {
		t.Fatal("Ctrl+End follows the end and reports it")
	}
}

func TestLandingShortOfEndStaysAnchored(t *testing.T) {
	m := seededModel(t)
	m = resizeModel(t, m, 118, 6)
	m.pinTranscriptToRow(0)
	if _, ok := m.reader.(anchored); !ok {
		t.Fatalf("reader = %T, want anchored on a line far from the end", m.reader)
	}
	if m.reader.reportsFollowEnd() {
		t.Fatal("a landing short of the end must not report follow-end")
	}
	if m.scrollLines() <= 0 {
		t.Fatalf("scroll = %d, want lines hidden below the window", m.scrollLines())
	}
}

// An older page above the reader moves both rows of a catch-up, and the reader
// stays catching up: the position changes shape for neither.
func TestPrependShiftsCatchUpRows(t *testing.T) {
	m := seededModel(t)
	m.holdAt(0)
	messages := m.ctrl.Messages()
	if len(messages) < 3 {
		t.Fatal("seeded transcript must have messages")
	}
	first, resume := len(messages)-1, len(messages)-2
	m.transcriptCount = m.transcriptRowTotal()
	m.setHeld(catchingUp{
		Anchor:         readerAnchor{PinSeq: messages[0].Sequence, SpliceEpoch: m.ctrl.TranscriptSpliceEpoch()},
		FirstUnseen:    first,
		FirstUnseenSeq: messages[first].Sequence,
		ResumeRow:      resume,
		ResumeSeq:      messages[resume].Sequence,
	})

	key := m.ctrl.Reducer().ConversationKey("omarchy", "#omarchy")
	when := time.Date(2026, 9, 26, 8, 0, 0, 0, time.UTC)
	m.ctrl.Apply(irc.HistoryEvent{
		Conversation: key,
		Target:       "#omarchy",
		Kind:         irc.HistoryChat,
		OlderPage:    true,
		Lines: []irc.ReplayLine{{
			Author:    "bob",
			Body:      "older page",
			Timestamp: when,
			MsgID:     irc.MsgID{Value: "older-page"},
		}},
	})
	m.ctrl.Publish(irc.ViewNotify{Messages: true})
	m.noteTranscriptGrowth()

	if _, ok := m.reader.(catchingUp); !ok {
		t.Fatalf("reader = %T, want the catch-up kept across the prepend", m.reader)
	}
	if m.firstUnseen() != first+1 || m.resumeRow() != resume+1 {
		t.Fatalf("rows = %d and %d, want %d and %d after one prepended row",
			m.firstUnseen(), m.resumeRow(), first+1, resume+1)
	}
}
