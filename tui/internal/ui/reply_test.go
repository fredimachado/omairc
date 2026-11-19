package ui

import (
	"strings"
	"testing"
	"time"

	"charm.land/lipgloss/v2"
	"github.com/fredimachado/omairc/tui/internal/controller"
)

func plainRow(m *Model, message controller.MessageSnapshot) []string {
	return strings.Split(ansiPattern.ReplaceAllString(m.messageRow(0, message), ""), "\n")
}

func TestReplyLineSitsAboveTheBodyAndChipsBelow(t *testing.T) {
	m := seededModel(t)
	at := time.Date(2026, 9, 27, 12, 0, 0, 0, time.UTC)
	reply := controller.MessageSnapshot{
		Author: "bob", Kind: "message", Body: "agreed", Time: at,
		ReplyTo: "m1", ReplyPreview: "↪ alice: ship it",
		Reactions: []controller.ReactionSnapshot{
			{Emoji: "👍", Count: 2, Nicks: []string{"anna", "carl"}},
			{Emoji: "🎉", Count: 1, Nicks: []string{"carl"}},
		},
	}
	lines := plainRow(m, reply)
	if len(lines) != 3 {
		t.Fatalf("reply row = %q, want reply line, body, chips", lines)
	}
	if !strings.Contains(lines[0], "↪ alice: ship it") || strings.Contains(lines[0], "agreed") {
		t.Fatalf("first line = %q, want the reply line alone", lines[0])
	}
	if !strings.Contains(lines[1], "agreed") {
		t.Fatalf("second line = %q, want the body", lines[1])
	}
	if !strings.Contains(lines[2], "👍 2") || !strings.Contains(lines[2], "🎉 1") {
		t.Fatalf("third line = %q, want the chips", lines[2])
	}
	// The reply line and chips hang under the body column, not under the clock.
	bodyAt := strings.Index(lines[1], "agreed")
	if at := strings.Index(lines[0], "↪"); at != bodyAt {
		t.Fatalf("reply line at column %d, body at %d", at, bodyAt)
	}
	if at := strings.Index(lines[2], "👍"); at != bodyAt {
		t.Fatalf("chips at column %d, body at %d", at, bodyAt)
	}
}

func TestUnloadedParentReadsReply(t *testing.T) {
	m := seededModel(t)
	reply := controller.MessageSnapshot{
		Author: "bob", Kind: "message", Body: "late", Time: time.Date(2026, 9, 27, 12, 0, 0, 0, time.UTC),
		ReplyTo: "gone", ReplyPreview: "↪ reply",
	}
	lines := plainRow(m, reply)
	if len(lines) != 2 || !strings.Contains(lines[0], "↪ reply") {
		t.Fatalf("row = %q, want the bare reply line then the body", lines)
	}
}

func TestReplyAndChipsStayInsideTheColumnAndKeepWideEmojiWhole(t *testing.T) {
	m := seededModel(t)
	long := strings.Repeat("wide ", 40)
	reply := controller.MessageSnapshot{
		Author: "bob", Kind: "message", Body: "x", Time: time.Date(2026, 9, 27, 12, 0, 0, 0, time.UTC),
		ReplyTo: "m1", ReplyPreview: "↪ alice: " + long,
		Reactions: []controller.ReactionSnapshot{{Emoji: "👍", Count: 3}},
	}
	lines := plainRow(m, reply)
	width := m.transcriptWidth()
	for _, line := range lines {
		if got := lipgloss.Width(line); got > width {
			t.Fatalf("line %q is %d wide, want at most %d", line, got, width)
		}
	}
	if !strings.Contains(lines[0], "…") {
		t.Fatalf("reply line = %q, want an ellipsis", lines[0])
	}
}

func TestActionAndNoticeRowsCarryReplyAndChipsToo(t *testing.T) {
	m := seededModel(t)
	at := time.Date(2026, 9, 27, 12, 0, 0, 0, time.UTC)
	for _, kind := range []string{"action", "notice"} {
		message := controller.MessageSnapshot{
			Author: "bob", Kind: kind, Body: "waves", Time: at,
			ReplyTo: "m1", ReplyPreview: "↪ alice: hi",
			Reactions: []controller.ReactionSnapshot{{Emoji: "👍", Count: 1}},
		}
		lines := plainRow(m, message)
		if len(lines) != 3 || !strings.HasPrefix(lines[0], "  ↪ alice: hi") || !strings.HasPrefix(lines[2], "  👍 1") {
			t.Fatalf("%s row = %q", kind, lines)
		}
	}
}

func TestEventRowsAreNeverDecorated(t *testing.T) {
	m := seededModel(t)
	event := controller.MessageSnapshot{
		Kind: "event", Body: "bob joined", ReplyPreview: "↪ reply",
		Reactions: []controller.ReactionSnapshot{{Emoji: "👍", Count: 1}},
	}
	if lines := plainRow(m, event); len(lines) != 1 {
		t.Fatalf("event row = %q, want one line", lines)
	}
}
