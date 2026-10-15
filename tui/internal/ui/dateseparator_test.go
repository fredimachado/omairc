package ui

import (
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

func yesterdayTranscript(t *testing.T, selectFirst bool) *Model {
	t.Helper()
	ctrl := controller.New()
	ctrl.SetClock(session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)))
	ctrl.Apply(irc.WelcomeEvent{NetworkID: "net", CurrentNick: "omairc"})
	if selectFirst {
		ctrl.SelectConversation("net", "#room")
	}
	ctrl.Apply(irc.MessageEvent{
		Conversation: irc.ConversationKey{NetworkID: "net", NormalizedTarget: "#room"},
		Author:       "alice",
		Body:         "only yesterday",
		Timestamp:    time.Date(2026, 9, 25, 15, 4, 0, 0, time.UTC),
		Target:       "#room",
	})
	m := New(ctrl, nil)
	return resizeModel(t, m, 80, 24)
}

func TestYesterdayBacklogRendersDateAboveTheFirstRow(t *testing.T) {
	m := yesterdayTranscript(t, true)
	if got := m.ctrl.UnreadMarkRow(); got != -1 {
		t.Fatalf("UnreadMarkRow = %d, want -1 when the backlog was already selected", got)
	}
	area := m.transcriptRowArea()
	if area.count() != 1 {
		t.Fatalf("rendered rows = %d, want 1 message row", area.count())
	}
	if got, want := plainText(area.lines[0]), plainText(m.eventRow("Yesterday")); got != want {
		t.Fatalf("first line = %q, want %q", got, want)
	}
	joined := plainText(strings.Join(area.lines, "\n"))
	if strings.Count(joined, "Yesterday") != 1 {
		t.Fatalf("Yesterday count in %q, want 1", joined)
	}
	if strings.Contains(joined, "New messages") {
		t.Fatalf("selected backlog must not render a boundary:\n%s", joined)
	}
}

func TestLeadingDateLetsTheUnreadBoundaryRender(t *testing.T) {
	m := yesterdayTranscript(t, false)
	if got := m.ctrl.UnreadMarkRow(); got != 0 {
		t.Fatalf("UnreadMarkRow = %d, want 0 under a leading date", got)
	}
	area := m.transcriptRowArea()
	if area.count() != m.transcriptRowTotal() {
		t.Fatalf("rendered rows = %d, want %d message rows", area.count(), m.transcriptRowTotal())
	}
	if got, want := plainText(area.lines[0]), plainText(m.eventRow("Yesterday")); got != want {
		t.Fatalf("first line = %q, want the date above New messages", got)
	}
	if !strings.Contains(plainText(area.lines[1]), "New messages") {
		t.Fatalf("second line = %q, want the New messages boundary", plainText(area.lines[1]))
	}
}
