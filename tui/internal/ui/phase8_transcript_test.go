package ui

import (
	"strings"
	"testing"
	"time"

	"charm.land/lipgloss/v2"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// transcriptPhase8Model seeds the demo on a frozen clock so the anna DM's
// seeded 10:12 row can never share the current HH:mm. A shared minute would
// group the typing footer, and the group-vs-ungroup choice would change with
// the wall clock. seededModel in phase5_test.go leaves the real clock in place.
func transcriptPhase8Model(t *testing.T) *Model {
	t.Helper()
	ctrl := controller.New()
	ctrl.SetClock(session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)))
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := New(ctrl, nil)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	return updated.(*Model)
}

// openPhase8AnnaDirect selects the seeded anna direct message through the same
// nick jump the typing fence drives.
func openPhase8AnnaDirect(t *testing.T, m *Model) *Model {
	t.Helper()
	m = press(t, m, ctrlShiftKey('k'))
	if !m.nickVisible() {
		t.Fatal("Ctrl+Shift+K must open the nick jump on a channel")
	}
	m.nick.input.SetValue("anna")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if got := m.View().WindowTitle; got != "anna - Omairc" {
		t.Fatalf("after the nick jump title = %q, want %q", got, "anna - Omairc")
	}
	return m
}

// TestPhase8TranscriptChannelHeaderPeopleCount proves the channel header
// carries the right-aligned "N PEOPLE" people control, not the old "(N)" form,
// and that it fills the transcript column, one gutter short, so the label sits
// near the right edge without touching the member column.
func TestPhase8TranscriptChannelHeaderPeopleCount(t *testing.T) {
	m := seededModel(t)
	if !m.ctrl.IsChannel() {
		t.Fatalf("seeded selection target = %q, want a channel", m.ctrl.SelectedTarget())
	}
	if got := m.ctrl.PeopleCount(); got != 12 {
		t.Fatalf("PeopleCount = %d, want 12", got)
	}

	lines, headerCount := m.transcriptLines()
	if headerCount < 1 {
		t.Fatal("a channel transcript must have a topic header")
	}
	header := lines[0]
	if !strings.Contains(header, "12 PEOPLE") {
		t.Fatalf("channel header = %q, want it to contain %q", header, "12 PEOPLE")
	}
	if strings.Contains(header, "(12)") {
		t.Fatalf("channel header = %q, the old (12) form must be gone", header)
	}
	if got, want := lipgloss.Width(header), m.transcriptWidth()-peopleCountGutter; got != want {
		t.Fatalf("channel header width = %d, want %d (transcript width less the gutter)", got, want)
	}
}

// TestPhase8TranscriptDirectHeaderHasNoPeopleCount proves a direct message
// header is only the topic caption and never carries a people count.
func TestPhase8TranscriptDirectHeaderHasNoPeopleCount(t *testing.T) {
	m := openPhase8AnnaDirect(t, seededModel(t))

	lines, _ := m.transcriptLines()
	if len(lines) == 0 {
		t.Fatal("the anna DM transcript must have a header")
	}
	header := lines[0]
	if !strings.Contains(header, "Direct message with anna") {
		t.Fatalf("DM header = %q, want the direct-message caption", header)
	}
	if strings.Contains(header, "PEOPLE") {
		t.Fatalf("DM header = %q, must not carry a people count", header)
	}
}

// TestPhase8TranscriptDirectTypingFooterUngrouped proves the seeded anna DM
// ends with the ungrouped footer: the peer's header line and an indented dots
// line after the last message row.
func TestPhase8TranscriptDirectTypingFooterUngrouped(t *testing.T) {
	m := openPhase8AnnaDirect(t, transcriptPhase8Model(t))

	nick, grouped, show := m.ctrl.TranscriptTypingIndicator()
	if nick != "anna" || grouped || !show {
		t.Fatalf("indicator = (%q, %v, %v), want (anna, false, true)", nick, grouped, show)
	}

	rows := len(m.ctrl.Messages())
	if rows == 0 {
		t.Fatal("the anna DM must have at least one seeded message row")
	}
	lines, headerCount := m.transcriptLines()
	if got, want := len(lines), headerCount+rows+2; got != want {
		t.Fatalf("line count = %d, want %d (rows plus the two footer lines)", got, want)
	}
	if got, want := lines[headerCount+rows], m.styles.MutedLine.Render("anna"); got != want {
		t.Fatalf("footer header = %q, want muted %q", got, "anna")
	}
	if dots := lines[headerCount+rows+1]; !strings.Contains(dots, "...") {
		t.Fatalf("footer dots = %q, want three periods", dots)
	}
}

// TestPhase8TranscriptChannelHasNoTypingFooter proves the typing footer stays
// DM-only. anna is typing in #omarchy, but the channel keeps the member-panel
// glyph and appends nothing to the transcript.
func TestPhase8TranscriptChannelHasNoTypingFooter(t *testing.T) {
	m := transcriptPhase8Model(t)
	if _, _, show := m.ctrl.TranscriptTypingIndicator(); show {
		t.Fatal("the transcript typing footer must be DM-only")
	}

	rows := len(m.ctrl.Messages())
	lines, headerCount := m.transcriptLines()
	if got, want := len(lines), headerCount+rows; got != want {
		t.Fatalf("line count = %d, want %d; a channel must append no footer", got, want)
	}
	dots := m.styles.MutedLine.Render("   ...")
	for _, line := range lines {
		if line == dots {
			t.Fatalf("channel transcript contains a typing dots row: %q", line)
		}
	}
}

// TestPhase8TranscriptPeopleCountHasColumnGutter is the regression test for the
// merged-column bug: the right-aligned count must stop peopleCountGutter cells
// short of the transcript edge so the member column's "ONLINE - N" heading does
// not run into it. The old width == transcriptWidth() math produced
// "12 PEOPLEONLINE - 12" in the real grid.
func TestPhase8TranscriptPeopleCountHasColumnGutter(t *testing.T) {
	m := seededModel(t)
	if !m.ctrl.IsChannel() {
		t.Fatalf("seeded selection target = %q, want a channel", m.ctrl.SelectedTarget())
	}
	if !m.membersVisible() {
		t.Fatal("the member column must be visible at 118 wide for this regression")
	}

	lines, headerCount := m.transcriptLines()
	if headerCount < 1 {
		t.Fatal("a channel transcript must have a topic header")
	}
	header := lines[0]
	// State the invariant directly: the count ends before the gutter.
	if got, want := lipgloss.Width(header), m.transcriptWidth()-peopleCountGutter; got != want {
		t.Fatalf("channel header width = %d, want %d (headroom for the gutter)", got, want)
	}
	if !strings.HasSuffix(header, m.styles.PeopleCount.Render("12 PEOPLE")) {
		t.Fatalf("channel header = %q, want it to end with the rendered count %q", header, "12 PEOPLE")
	}

	content := m.View().Content
	firstRow := strings.SplitN(content, "\n", 2)[0]
	t.Logf("first rendered row: %q", firstRow)
	if strings.Contains(content, "PEOPLEONLINE") {
		t.Fatalf("the people count merged with the member column heading:\n%s", content)
	}
	if !strings.Contains(content, "12 PEOPLE") {
		t.Fatalf("rendered content missing %q:\n%s", "12 PEOPLE", content)
	}
	if !strings.Contains(content, "ONLINE - 12") {
		t.Fatalf("rendered content missing %q:\n%s", "ONLINE - 12", content)
	}
}
