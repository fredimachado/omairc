package ui

import (
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// ergoColourChart is one row of Ergo's default MOTD colour chart: a long
// Status line made mostly of mIRC colour codes.
func ergoColourChart(first int) string {
	var builder strings.Builder
	builder.WriteString("- ")
	for code := first; code < first+12; code++ {
		fmt.Fprintf(&builder, "\x030,%d %d\x03 ", code, code)
	}
	return builder.String()
}

// TestStatusWithColourCodesKeepsTheNewestLineOnScreen locks the live Ergo
// report: a MOTD colour chart in Status must not grow the console past its
// viewport, so a line that arrives after the MOTD is the last row on screen and
// the frame still ends on the footer.
func TestStatusWithColourCodesKeepsTheNewestLineOnScreen(t *testing.T) {
	m := seededModel(t)
	m.toggleStatus()
	if !m.ctrl.ConsoleOpen() {
		t.Fatal("Status should be the open pane")
	}
	network := m.ctrl.FocusedNetworkID()
	for first := 16; first < 100; first += 12 {
		m.ctrl.StatusEntry(irc.Outcome(network, ergoColourChart(first), time.Now()))
	}
	m.ctrl.StatusEntry(irc.Outcome(network, "bob invited you to #inv3", time.Now()))

	lines := transcriptViewLines(m)
	if len(lines) != m.transcriptHeight() {
		t.Fatalf("transcript view lines = %d, want height %d", len(lines), m.transcriptHeight())
	}
	if got := plainLine(lines[len(lines)-1]); !strings.Contains(got, "bob invited you to #inv3") {
		t.Fatalf("last Status row = %q, want the invite that arrived after the MOTD", got)
	}
	view := strings.Split(strings.TrimSuffix(m.View().Content, "\n"), "\n")
	if len(view) != 30 {
		t.Fatalf("frame is %d lines, want the 30-line terminal", len(view))
	}
	for index, line := range transcriptViewLines(m) {
		if strings.ContainsAny(line, "\x02\x03\x0f\x16\x1d\x1f") {
			t.Fatalf("transcript line %d carries a raw IRC control code: %q", index, line)
		}
	}
	for _, text := range m.transcriptRowTexts() {
		if strings.ContainsAny(text, "\x03") {
			t.Fatalf("find/copy row %q carries a raw colour code", text)
		}
	}
}
