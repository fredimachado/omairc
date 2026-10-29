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

// TestOlderPageAtTopDoesNotArmJumpWhenCaughtUp is a caught-up guard: a
// CHATHISTORY BEFORE page at the top must not arm ↓ new when there is no
// marker. It is not the invent-without-sequence repro (see
// TestNoteTranscriptGrowthRestoresInventedFirstUnseenAfterOlderPage).
func TestOlderPageAtTopDoesNotArmJumpWhenCaughtUp(t *testing.T) {
	m, key, day := scrollableYesterday(t)
	if m.firstUnseenRow != -1 || m.jumpArmed() {
		t.Fatalf("precondition: caught up at top, unseen=%d armed=%v",
			m.firstUnseenRow, m.jumpArmed())
	}
	prependOlder(m, key, "ancient", day.Add(-48*time.Hour), "ancient-id")
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if m.firstUnseenRow != -1 || m.jumpArmed() || strings.Contains(plain, "↓ new") {
		t.Fatalf("after older page: unseen=%d armed=%v\n%s",
			m.firstUnseenRow, m.jumpArmed(), plain)
	}
}

// TestOlderPageWithoutAnchorDoesNotArmJumpWhenCaughtUp is the splice-skipped
// path: head growth with no transcript anchor must not set firstUnseenRow to
// the old count (already-read history) and paint ↓ new.
func TestOlderPageWithoutAnchorDoesNotArmJumpWhenCaughtUp(t *testing.T) {
	m, key, day := scrollableYesterday(t)
	if m.firstUnseenRow != -1 || m.jumpArmed() {
		t.Fatalf("precondition: caught up at top, unseen=%d armed=%v",
			m.firstUnseenRow, m.jumpArmed())
	}
	m.clearTranscriptAnchor()
	m.transcriptCount = m.transcriptRowTotal()
	m.transcriptSpliceEpoch = m.ctrl.TranscriptSpliceEpoch()
	m.ctrl.Apply(irc.HistoryEvent{
		Conversation: key,
		Target:       "#room",
		Kind:         irc.HistoryChat,
		OlderPage:    true,
		Lines: []irc.ReplayLine{{
			Author:    "bob",
			Body:      "ancient unanchored",
			Timestamp: day.Add(-48 * time.Hour),
			MsgID:     irc.MsgID{Value: "ancient-unanchored"},
		}},
	})
	m.ctrl.Publish(irc.ViewNotify{Messages: true})
	m.noteTranscriptGrowth()
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if m.firstUnseenRow != -1 || m.jumpArmed() || strings.Contains(plain, "↓ new") {
		t.Fatalf("after unanchored older page: unseen=%d armed=%v\n%s",
			m.firstUnseenRow, m.jumpArmed(), plain)
	}
}

// TestOlderPageDoesNotRestoreConsumedFirstUnseen proves an inflight older-page
// snapshot must not revive a firstUnseenRow the reader already cleared. A
// leftover sequence alone is not enough to re-arm after the row was consumed.
func TestOlderPageDoesNotRestoreConsumedFirstUnseen(t *testing.T) {
	m, key, day := scrollableYesterday(t)
	messages := m.ctrl.Messages()
	marker := len(messages) - 1
	m.firstUnseenRow = marker
	m.transcriptCount = m.transcriptRowTotal()
	m.transcriptSpliceEpoch = m.ctrl.TranscriptSpliceEpoch()
	m.snapshotTranscriptAnchor()
	if m.firstUnseenSequenceAtAnchor != messages[marker].Sequence {
		t.Fatal("snapshot must record the unseen sequence")
	}
	stale := m.firstUnseenSequenceAtAnchor
	m.clearFirstUnseen()
	if m.firstUnseenSequenceAtAnchor >= 0 {
		t.Fatalf("clearFirstUnseen must drop the snapshotted sequence, got %d",
			m.firstUnseenSequenceAtAnchor)
	}
	// Leave a stale sequence as a pre-fix clear of only firstUnseenRow did.
	m.firstUnseenSequenceAtAnchor = stale
	m.ctrl.Apply(irc.HistoryEvent{
		Conversation: key,
		Target:       "#room",
		Kind:         irc.HistoryChat,
		OlderPage:    true,
		Lines: []irc.ReplayLine{{
			Author:    "bob",
			Body:      "after clear",
			Timestamp: day.Add(-48 * time.Hour),
			MsgID:     irc.MsgID{Value: "after-clear"},
		}},
	})
	m.ctrl.Publish(irc.ViewNotify{Messages: true})
	m.noteTranscriptGrowth()
	if m.firstUnseenRow != -1 || m.jumpArmed() {
		t.Fatalf("consumed unseen must stay clear after older page, unseen=%d armed=%v",
			m.firstUnseenRow, m.jumpArmed())
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

// TestNoteTranscriptGrowthRestoresInventedFirstUnseenAfterOlderPage proves the
// inflight invent-without-sequence path: snapshot older-page with no marker,
// a live append invents firstUnseenRow while the anchor is live, then the
// older page restores. The marker must stay on the invented live row, not the
// pre-prepend index.
func TestNoteTranscriptGrowthRestoresInventedFirstUnseenAfterOlderPage(t *testing.T) {
	m := seededModel(t)
	m.transcriptFollowEnd = false
	messages := m.ctrl.Messages()
	if len(messages) < 2 {
		t.Fatal("seeded transcript must have messages")
	}
	m.transcriptCount = m.transcriptRowTotal()
	m.transcriptSpliceEpoch = m.ctrl.TranscriptSpliceEpoch()
	m.snapshotTranscriptAnchor()
	if m.transcriptAnchorSequence < 0 {
		t.Fatal("precondition: older-page anchor must be live")
	}
	if m.firstUnseenRow >= 0 || m.firstUnseenSequenceAtAnchor >= 0 {
		t.Fatalf("precondition: no marker yet, unseen=%d seq=%d",
			m.firstUnseenRow, m.firstUnseenSequenceAtAnchor)
	}

	oldCount := m.transcriptCount
	key := m.ctrl.Reducer().ConversationKey("omarchy", "#omarchy")
	m.ctrl.Apply(irc.MessageEvent{
		Conversation: key,
		Author:       "alice",
		Body:         "live while older page inflight",
		Timestamp:    time.Date(2026, 9, 27, 12, 0, 0, 0, time.UTC),
		Target:       "#omarchy",
	})
	m.ctrl.Publish(irc.ViewNotify{Messages: true})
	m.noteTranscriptGrowth()
	if m.firstUnseenRow != oldCount {
		t.Fatalf("invented firstUnseenRow = %d, want %d", m.firstUnseenRow, oldCount)
	}
	inventedSequence := m.ctrl.Messages()[m.firstUnseenRow].Sequence
	if m.firstUnseenSequenceAtAnchor != inventedSequence {
		t.Fatalf("invent under a live anchor must record sequence, got %d want %d",
			m.firstUnseenSequenceAtAnchor, inventedSequence)
	}
	if m.transcriptAnchorSequence < 0 {
		t.Fatal("live invent must leave the older-page anchor armed")
	}

	when := time.Date(2026, 9, 26, 8, 0, 0, 0, time.UTC)
	m.ctrl.Apply(irc.HistoryEvent{
		Conversation: key,
		Target:       "#omarchy",
		Kind:         irc.HistoryChat,
		OlderPage:    true,
		Lines: []irc.ReplayLine{{
			Author:    "bob",
			Body:      "older during invent",
			Timestamp: when,
			MsgID:     irc.MsgID{Value: "older-during-invent"},
		}},
	})
	m.ctrl.Publish(irc.ViewNotify{Messages: true})
	m.noteTranscriptGrowth()

	wantRow := oldCount + 1
	if m.firstUnseenRow != wantRow {
		t.Fatalf("firstUnseenRow = %d, want %d after prepend over invented marker",
			m.firstUnseenRow, wantRow)
	}
	if got := m.ctrl.Messages()[m.firstUnseenRow].Sequence; got != inventedSequence {
		t.Fatalf("marker sequence = %d, want invented live %d", got, inventedSequence)
	}
}

func TestNoteTranscriptGrowthRestoresUnseenResumeAfterOlderPage(t *testing.T) {
	m := seededModel(t)
	m.transcriptFollowEnd = false
	messages := m.ctrl.Messages()
	if len(messages) < 2 {
		t.Fatal("seeded transcript must have messages")
	}
	resumeIndex := len(messages) - 1
	m.unseenResumeRow = resumeIndex
	m.transcriptCount = m.transcriptRowTotal()
	m.transcriptAnchorSequence = messages[0].Sequence
	m.transcriptAnchorSpliceEpoch = m.ctrl.TranscriptSpliceEpoch()
	m.unseenResumeSequenceAtAnchor = messages[resumeIndex].Sequence

	key := m.ctrl.Reducer().ConversationKey("omarchy", "#omarchy")
	when := time.Date(2026, 9, 26, 8, 0, 0, 0, time.UTC)
	m.ctrl.Apply(irc.HistoryEvent{
		Conversation: key,
		Target:       "#omarchy",
		Kind:         irc.HistoryChat,
		OlderPage:    true,
		Lines: []irc.ReplayLine{{
			Author:    "bob",
			Body:      "older resume page",
			Timestamp: when,
			MsgID:     irc.MsgID{Value: "older-resume-page"},
		}},
	})
	m.ctrl.Publish(irc.ViewNotify{Messages: true})
	m.noteTranscriptGrowth()

	wantRow := resumeIndex + 1
	if m.unseenResumeRow != wantRow {
		t.Fatalf("unseenResumeRow = %d, want %d after one prepended row", m.unseenResumeRow, wantRow)
	}
}

func TestNoteTranscriptGrowthShiftsFirstUnseenWithoutASequence(t *testing.T) {
	m := seededModel(t)
	m.transcriptFollowEnd = false
	messages := m.ctrl.Messages()
	if len(messages) < 2 {
		t.Fatal("seeded transcript must have messages")
	}
	markerIndex := len(messages) - 1
	m.firstUnseenRow = markerIndex
	m.firstUnseenSequenceAtAnchor = -1
	m.transcriptCount = m.transcriptRowTotal()
	m.transcriptAnchorSequence = messages[0].Sequence
	m.transcriptAnchorSpliceEpoch = m.ctrl.TranscriptSpliceEpoch()

	key := m.ctrl.Reducer().ConversationKey("omarchy", "#omarchy")
	when := time.Date(2026, 9, 26, 8, 2, 0, 0, time.UTC)
	m.ctrl.Apply(irc.HistoryEvent{
		Conversation: key,
		Target:       "#omarchy",
		Kind:         irc.HistoryChat,
		OlderPage:    true,
		Lines: []irc.ReplayLine{{
			Author:    "bob",
			Body:      "older unseen delta",
			Timestamp: when,
			MsgID:     irc.MsgID{Value: "older-unseen-delta"},
		}},
	})
	m.ctrl.Publish(irc.ViewNotify{Messages: true})
	m.noteTranscriptGrowth()

	wantRow := markerIndex + 1
	if m.firstUnseenRow != wantRow {
		t.Fatalf("firstUnseenRow = %d, want %d after a delta shift", m.firstUnseenRow, wantRow)
	}
}

func TestNoteTranscriptGrowthShiftsUnseenResumeWithoutASequence(t *testing.T) {
	m := seededModel(t)
	m.transcriptFollowEnd = false
	messages := m.ctrl.Messages()
	if len(messages) < 2 {
		t.Fatal("seeded transcript must have messages")
	}
	resumeIndex := len(messages) - 1
	m.unseenResumeRow = resumeIndex
	m.unseenResumeSequenceAtAnchor = -1
	m.transcriptCount = m.transcriptRowTotal()
	m.transcriptAnchorSequence = messages[0].Sequence
	m.transcriptAnchorSpliceEpoch = m.ctrl.TranscriptSpliceEpoch()

	key := m.ctrl.Reducer().ConversationKey("omarchy", "#omarchy")
	when := time.Date(2026, 9, 26, 8, 1, 0, 0, time.UTC)
	m.ctrl.Apply(irc.HistoryEvent{
		Conversation: key,
		Target:       "#omarchy",
		Kind:         irc.HistoryChat,
		OlderPage:    true,
		Lines: []irc.ReplayLine{{
			Author:    "bob",
			Body:      "older resume delta",
			Timestamp: when,
			MsgID:     irc.MsgID{Value: "older-resume-delta"},
		}},
	})
	m.ctrl.Publish(irc.ViewNotify{Messages: true})
	m.noteTranscriptGrowth()

	wantRow := resumeIndex + 1
	if m.unseenResumeRow != wantRow {
		t.Fatalf("unseenResumeRow = %d, want %d after a delta shift", m.unseenResumeRow, wantRow)
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
