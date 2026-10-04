package ui

import (
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/irc"
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

func TestNoteTranscriptGrowthRestoresFirstUnseenAfterOlderPage(t *testing.T) {
	m := seededModel(t)
	m.transcriptFollowEnd = false
	messages := m.ctrl.Messages()
	if len(messages) < 2 {
		t.Fatal("seeded transcript must have messages")
	}
	markerIndex := len(messages) - 1
	m.firstUnseenRow = markerIndex
	m.transcriptCount = m.transcriptRowTotal()
	m.transcriptAnchorSequence = messages[0].Sequence
	m.transcriptAnchorSpliceEpoch = m.ctrl.TranscriptSpliceEpoch()
	m.firstUnseenSequenceAtAnchor = messages[markerIndex].Sequence

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

	wantRow := markerIndex + 1
	if m.firstUnseenRow != wantRow {
		t.Fatalf("firstUnseenRow = %d, want %d after one prepended row", m.firstUnseenRow, wantRow)
	}
}

func TestLeavingConversationClearsHistoryPageCapTail(t *testing.T) {
	m := seededModel(t)
	key := m.ctrl.Reducer().ConversationKey("omarchy", "#omarchy")
	m.ctrl.Reducer().MarkHistoryPageCapTail(key)
	previousID := m.selectedConversationID()
	m.ctrl.SelectConversation("omarchy", "#desktop")
	m.afterSelectionChange(previousID)
	conversation := m.ctrl.Reducer().Find(key)
	if conversation == nil {
		t.Fatal("omarchy channel must still exist")
	}
	if conversation.HistoryPageCapTail {
		t.Fatal("leaving a conversation must clear its HistoryPageCapTail")
	}
}
