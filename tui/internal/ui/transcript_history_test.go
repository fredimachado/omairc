package ui

import (
	"fmt"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
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

func scrollableYesterday(t *testing.T) (*Model, irc.ConversationKey, time.Time) {
	t.Helper()
	ctrl := controller.New()
	ctrl.SetClock(session.NewFakeClock(time.Date(2026, 9, 26, 12, 0, 0, 0, time.UTC)))
	ctrl.Apply(irc.WelcomeEvent{NetworkID: "net", CurrentNick: "omairc"})
	ctrl.SelectConversation("net", "#room")
	key := ctrl.Reducer().ConversationKey("net", "#room")
	day := time.Date(2026, 9, 25, 15, 0, 0, 0, time.UTC)
	ctrl.Apply(irc.MessageEvent{
		Conversation: key,
		Author:       "alice",
		Body:         "kept",
		Timestamp:    day,
		Target:       "#room",
	})
	for index := 0; index < 40; index++ {
		ctrl.Apply(irc.MessageEvent{
			Conversation: key,
			Author:       "alice",
			Body:         fmt.Sprintf("pad %d", index),
			Timestamp:    day.Add(time.Duration(index+1) * time.Minute),
			Target:       "#room",
		})
	}
	m := resizeModel(t, New(ctrl, nil), 80, 30)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyHome, Mod: tea.ModCtrl})
	if m.transcriptFollowEnd {
		t.Fatal("Ctrl+Home must leave follow-the-end")
	}
	return m, key, day
}

func screenLineContaining(m *Model, body string) int {
	area := m.transcriptArea()
	height := m.transcriptRowsHeight()
	start, end := m.transcriptWindow(len(area.lines), height)
	for index := start; index < end; index++ {
		if strings.Contains(plainText(area.lines[index]), body) {
			return index - start
		}
	}
	return -1
}

func prependOlder(m *Model, key irc.ConversationKey, body string, at time.Time, id string) {
	m.snapshotTranscriptAnchor()
	m.ctrl.Apply(irc.HistoryEvent{
		Conversation: key,
		Target:       "#room",
		Kind:         irc.HistoryChat,
		OlderPage:    true,
		Lines: []irc.ReplayLine{{
			Author:    "bob",
			Body:      body,
			Timestamp: at,
			MsgID:     irc.MsgID{Value: id},
		}},
	})
	m.ctrl.Publish(irc.ViewNotify{Messages: true})
	m.noteTranscriptGrowth()
}

func TestSameDayOlderPageKeepsAnchorTextLine(t *testing.T) {
	m, key, day := scrollableYesterday(t)
	if labels := m.ctrl.DateSeparators(); len(labels) == 0 || labels[0] != "Yesterday" {
		t.Fatalf("leading label = %q, want Yesterday", labels)
	}
	before := screenLineContaining(m, "kept")
	if before < 1 {
		t.Fatalf("kept screen line = %d, want it below the leading rule", before)
	}
	prependOlder(m, key, "older same day", day.Add(-time.Hour), "same-day")
	if got := screenLineContaining(m, "kept"); got != before {
		t.Fatalf("kept screen line = %d, want %d after the top rule moved", got, before)
	}
	labels := m.ctrl.DateSeparators()
	kept := -1
	for index, message := range m.ctrl.Messages() {
		if message.Body == "kept" {
			kept = index
			break
		}
	}
	if kept <= 0 || kept >= len(labels) || labels[kept] != "" {
		t.Fatalf("kept row %d labels %q, want the rule on the older line", kept, labels)
	}
	if labels[0] != "Yesterday" {
		t.Fatalf("leading label = %q, want one Yesterday", labels[0])
	}
}

func TestOlderDayPrependKeepsAnchorTextLine(t *testing.T) {
	m, key, day := scrollableYesterday(t)
	before := screenLineContaining(m, "kept")
	if before < 1 {
		t.Fatalf("kept screen line = %d, want it below the leading rule", before)
	}
	prependOlder(m, key, "from the day before", day.Add(-24*time.Hour), "older-day")
	if got := screenLineContaining(m, "kept"); got != before {
		t.Fatalf("kept screen line = %d, want %d after the head day changed", got, before)
	}
	labels := m.ctrl.DateSeparators()
	kept := -1
	for index, message := range m.ctrl.Messages() {
		if message.Body == "kept" {
			kept = index
			break
		}
	}
	if kept <= 0 || kept >= len(labels) || labels[kept] != "Yesterday" {
		t.Fatalf("kept row %d labels %q, want Yesterday still on that message", kept, labels)
	}
	if labels[0] == "Yesterday" || labels[0] == "" {
		t.Fatalf("leading label = %q, want the older day", labels[0])
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

func TestDetachedGrowthKeepsTheTopLine(t *testing.T) {
	m, d := unreadDemoModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	top := m.firstVisibleMessageRow()
	body := m.ctrl.Messages()[top].Body
	scrollBefore := m.transcriptScroll
	linesBefore := m.transcriptLineCount
	d.InjectOmarchy([]byte(":anna!u@h PRIVMSG #omarchy :a line below the reader\r\n"))
	m.noteTranscriptGrowth()
	if m.transcriptFollowEnd {
		t.Fatal("a detached reader must stay detached")
	}
	if got := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body; got != body {
		t.Fatalf("top body = %q, want %q", got, body)
	}
	delta := m.transcriptLineCount - linesBefore
	if delta < 1 {
		t.Fatalf("rendered lines grew by %d, want at least 1", delta)
	}
	if m.transcriptScroll != scrollBefore+delta {
		t.Fatalf("scroll = %d, want %d", m.transcriptScroll, scrollBefore+delta)
	}
}

func TestFollowEndGrowthLeavesScrollAtZero(t *testing.T) {
	m, d := unreadDemoModel(t)
	if !m.transcriptFollowEnd {
		t.Fatal("startup must follow the end")
	}
	d.InjectOmarchy([]byte(":anna!u@h PRIVMSG #omarchy :a line at the end\r\n"))
	m.noteTranscriptGrowth()
	if !m.transcriptFollowEnd || m.transcriptScroll != 0 {
		t.Fatalf("follow=%v scroll=%d, want pinned with scroll 0", m.transcriptFollowEnd, m.transcriptScroll)
	}
	if m.firstUnseenRow != -1 {
		t.Fatalf("firstUnseenRow = %d, want -1", m.firstUnseenRow)
	}
}

func TestLeavingConversationClearsHistoryPageCapTail(t *testing.T) {
	m := seededModel(t)
	key := m.ctrl.Reducer().ConversationKey("omarchy", "#omarchy")
	m.ctrl.Reducer().MarkHistoryPageCapTail(key)
	previousID := m.selectedConversationID()
	m.ctrl.SelectConversation("omarchy", "#desktop")
	m.afterSelectionChange(previousID, false)
	conversation := m.ctrl.Reducer().Find(key)
	if conversation == nil {
		t.Fatal("omarchy channel must still exist")
	}
	if conversation.HistoryPageCapTail {
		t.Fatal("leaving a conversation must clear its HistoryPageCapTail")
	}
}
