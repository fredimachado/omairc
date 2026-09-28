package ui

// This file covers the transcript's byline column: the timestamp at the left,
// the right-aligned nick, the separator rule, wrapping under the body column,
// and the grouping that blanks a continuation row's time and nick.

import (
	"strings"
	"testing"
	"time"

	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// plainText strips a rendered line's ANSI so a test can compare columns.
func plainText(line string) string {
	return ansiPattern.ReplaceAllString(line, "")
}

// TestTranscriptNickColumnFitsChatRowsOnly pins that the right-aligned column is
// measured from the widest plain chat nick, ignoring the shaped action, notice,
// and event rows that do not carry it.
func TestTranscriptNickColumnFitsChatRowsOnly(t *testing.T) {
	messages := []controller.MessageSnapshot{
		{Author: "bob", Kind: "message"},
		{Author: "averyveryverylongnick", Kind: "action"},
		{Author: "sue", Kind: "notice"},
		{Author: "longnickname", Kind: "message"},
	}
	if got, want := nickColumnWidth(messages), len("longnickname"); got != want {
		t.Fatalf("nickColumnWidth = %d, want %d (widest chat nick only)", got, want)
	}
}

// TestTranscriptChatRowColumns pins the byline: the HH:mm stamp at the left, the
// nick right-aligned in its column, and the separator at the shared offset, so
// every body starts at the same cell.
func TestTranscriptChatRowColumns(t *testing.T) {
	m := seededModel(t)
	at := time.Date(2026, 9, 27, 12, 5, 0, 0, time.UTC)
	messages := []controller.MessageSnapshot{
		{Author: "anna", Kind: "message", Body: "hi", Time: at},
		{Author: "longnickname", Kind: "message", Body: "hello", Time: at},
	}
	nickWidth := nickColumnWidth(messages)
	if want := len("longnickname"); nickWidth != want {
		t.Fatalf("nickWidth = %d, want %d", nickWidth, want)
	}
	separatorOffset := transcriptClockWidth + 1 + nickWidth + 1

	short := plainText(m.messageRowAt(messages, 0, messages[0], nickWidth))
	if got := strings.Index(short, "│"); got != separatorOffset {
		t.Fatalf("short row separator at %d, want %d: %q", got, separatorOffset, short)
	}
	if !strings.HasPrefix(short, "12:05 ") {
		t.Fatalf("short row = %q, want it to start with the timestamp", short)
	}
	if column := short[transcriptClockWidth+1 : separatorOffset-1]; column != "        anna" {
		t.Fatalf("short row nick column = %q, want %q right-aligned", column, "anna")
	}

	long := plainText(m.messageRowAt(messages, 1, messages[1], nickWidth))
	if column := long[transcriptClockWidth+1 : separatorOffset-1]; column != "longnickname" {
		t.Fatalf("long row nick column = %q, want it to fill the column", column)
	}
}

// TestTranscriptChatRowWrapsUnderBody pins that a body wider than the column
// wraps and every continuation line keeps the separator at the shared column,
// so the rule stays an unbroken line down the transcript while the body wraps
// under the first line.
func TestTranscriptChatRowWrapsUnderBody(t *testing.T) {
	m := seededModel(t)
	at := time.Date(2026, 9, 27, 12, 5, 0, 0, time.UTC)
	messages := []controller.MessageSnapshot{
		{Author: "anna", Kind: "message",
			Body: strings.TrimSpace(strings.Repeat("word ", 60)), Time: at},
	}
	nickWidth := nickColumnWidth(messages)
	separatorOffset := transcriptClockWidth + 1 + nickWidth + 1

	block := plainText(m.messageRowAt(messages, 0, messages[0], nickWidth))
	lines := strings.Split(block, "\n")
	if len(lines) < 2 {
		t.Fatalf("wrapped row = %q, want more than one line", block)
	}
	for index, line := range lines {
		if width := lipgloss.Width(line); width > m.transcriptWidth() {
			t.Fatalf("wrapped line %d width = %d, want <= %d: %q",
				index, width, m.transcriptWidth(), line)
		}
		if got := strings.Index(line, "│"); got != separatorOffset {
			t.Fatalf("line %d separator at %d, want %d (unbroken column): %q",
				index, got, separatorOffset, line)
		}
	}
	// The body of every continuation line follows the blank columns plus the
	// separator, so it aligns with the first line's body.
	wantPrefix := strings.Repeat(" ", separatorOffset) + "│ "
	if !strings.HasPrefix(lines[1], wantPrefix) {
		t.Fatalf("continuation line = %q, want the prefix %q", lines[1], wantPrefix)
	}
}

// TestTranscriptGroupBlanksTimeAndNick pins that a chat row continuing the row
// above it blanks the timestamp and nick while keeping the separator, so a run
// from one nick lines up under its first row.
func TestTranscriptGroupBlanksTimeAndNick(t *testing.T) {
	m := seededModel(t)
	at := time.Date(2026, 9, 27, 12, 5, 0, 0, time.UTC)
	messages := []controller.MessageSnapshot{
		{Author: "anna", Kind: "message", Body: "one", Time: at},
		{Author: "anna", Kind: "message", Body: "two", Time: at},
	}
	nickWidth := nickColumnWidth(messages)
	separatorOffset := transcriptClockWidth + 1 + nickWidth + 1

	first := plainText(m.messageRowAt(messages, 0, messages[0], nickWidth))
	if !strings.HasPrefix(first, "12:05 ") {
		t.Fatalf("first row = %q, want the timestamp", first)
	}

	grouped := plainText(m.messageRowAt(messages, 1, messages[1], nickWidth))
	if prefix := grouped[:separatorOffset]; strings.TrimSpace(prefix) != "" {
		t.Fatalf("grouped row prefix = %q, want blank time and nick", prefix)
	}
	if got := strings.Index(grouped, "│"); got != separatorOffset {
		t.Fatalf("grouped separator at %d, want %d: %q", got, separatorOffset, grouped)
	}
	if !strings.HasSuffix(grouped, "│ two") {
		t.Fatalf("grouped row = %q, want the separator then the body", grouped)
	}
}

// TestContinuesChatGroup mirrors continuesMessageGroup in OmaircWindow.qml: a
// row groups only with a same-nick, same-minute, same-origin chat row above it.
func TestContinuesChatGroup(t *testing.T) {
	at := time.Date(2026, 9, 27, 12, 5, 0, 0, time.UTC)
	live := controller.MessageSnapshot{Author: "anna", Kind: "message", Time: at, Origin: "live"}
	cases := []struct {
		name  string
		prior controller.MessageSnapshot
		row   controller.MessageSnapshot
		want  bool
	}{
		{"same nick and minute", live,
			controller.MessageSnapshot{Author: "anna", Kind: "message", Time: at, Origin: "live"}, true},
		{"different nick", live,
			controller.MessageSnapshot{Author: "bob", Kind: "message", Time: at, Origin: "live"}, false},
		{"different minute", live,
			controller.MessageSnapshot{Author: "anna", Kind: "message", Time: at.Add(time.Minute), Origin: "live"}, false},
		{"different origin", live,
			controller.MessageSnapshot{Author: "anna", Kind: "message", Time: at, Origin: "replay"}, false},
		{"action above", controller.MessageSnapshot{Author: "anna", Kind: "action", Time: at, Origin: "live"},
			controller.MessageSnapshot{Author: "anna", Kind: "message", Time: at, Origin: "live"}, false},
		{"whois above", controller.MessageSnapshot{Author: "anna", Kind: "whois", Time: at, Origin: "live"},
			controller.MessageSnapshot{Author: "anna", Kind: "message", Time: at, Origin: "live"}, false},
	}
	for _, testCase := range cases {
		prior := testCase.prior
		if got := continuesChatGroup(&prior, testCase.row); got != testCase.want {
			t.Fatalf("%s: continuesChatGroup = %v, want %v", testCase.name, got, testCase.want)
		}
	}
	if continuesChatGroup(nil, live) {
		t.Fatal("continuesChatGroup(nil, row) = true, want false")
	}
}
