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

// assertHeaderBandGutter checks a banded transcript header spans the column and
// stops peopleCountGutter blank cells short of the member column, so the
// "N PEOPLE" chip never runs into the member heading.
func assertHeaderBandGutter(t *testing.T, width int, header string) {
	t.Helper()
	if got := lipgloss.Width(header); got != width {
		t.Fatalf("header band width = %d, want the full %d-cell column", got, width)
	}
	plain := []rune(ansiPattern.ReplaceAllString(header, ""))
	if len(plain) < peopleCountGutter {
		t.Fatalf("header = %q, too narrow for the %d-cell gutter", string(plain), peopleCountGutter)
	}
	if trailing := string(plain[len(plain)-peopleCountGutter:]); strings.TrimSpace(trailing) != "" {
		t.Fatalf("header = %q, want the chip to end %d gutter cells short of the column edge",
			string(plain), peopleCountGutter)
	}
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
	// The band spans the column, so the count keeps one gutter of band before
	// the member column instead of touching it.
	assertHeaderBandGutter(t, m.transcriptWidth(), header)
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
	area := m.transcriptArea()
	// The two footer lines follow the last row directly.
	if got, want := len(lines), headerCount+area.end()+2; got != want {
		t.Fatalf("line count = %d, want %d (rows plus the two footer lines)", got, want)
	}
	footerByline := lines[headerCount+area.end()]
	if want := m.nickStyle("anna").Render("anna"); !strings.Contains(footerByline, want) {
		t.Fatalf("footer byline = %q, want the peer's nickname %q", footerByline, want)
	}
	if !strings.Contains(footerByline, transcriptSeparator) {
		t.Fatalf("footer byline = %q, want the column separator %q",
			footerByline, transcriptSeparator)
	}
	dots := lines[headerCount+area.end()+1]
	if !strings.Contains(dots, "...") {
		t.Fatalf("footer dots = %q, want three periods", dots)
	}
	if plain := ansiPattern.ReplaceAllString(dots, ""); !strings.HasPrefix(plain, " ") {
		t.Fatalf("footer dots = %q, want it indented to the body column", plain)
	}
}

// TestPhase8TranscriptMentionRowWash proves a highlighted row is a full-width
// wash band, one line per message, with the palette-colored bold nick and the
// dimmed timestamp inside it.
func TestPhase8TranscriptMentionRowWash(t *testing.T) {
	m := seededModel(t)
	msg := controller.MessageSnapshot{
		Author:    "mira",
		Kind:      "message",
		Body:      "hello there",
		Time:      time.Date(2026, 9, 27, 12, 0, 0, 0, time.UTC),
		Mentioned: true,
	}

	row := m.messageRow(0, msg)
	if strings.Count(row, "\n") != 0 {
		t.Fatalf("mention row = %q, want a single line", row)
	}
	if got, want := lipgloss.Width(row), m.transcriptWidth(); got != want {
		t.Fatalf("mention row width = %d, want %d (the wash spans the column)", got, want)
	}
	for _, want := range []string{"12:00", "mira", "hello there"} {
		if !strings.Contains(row, want) {
			t.Fatalf("mention row = %q, want it to contain %q", row, want)
		}
	}
	wash := m.mentionWash()
	nick := wash.Foreground(nickColor("mira")).Bold(true).Render("mira")
	if !strings.Contains(row, nick) {
		t.Fatalf("mention row = %q, want the palette-colored bold nick %q", row, nick)
	}
	if tinted := wash.Foreground(m.styles.Colors.Mention).Render("mira"); strings.Contains(row, tinted) {
		t.Fatalf("mention row = %q, want the nick in its palette color, not the mention tint", row)
	}
	if stamped := wash.Foreground(m.styles.Colors.TextDim).Render("12:00"); !strings.Contains(row, stamped) {
		t.Fatalf("mention row = %q, want the dimmed timestamp %q", row, stamped)
	}
	if !strings.Contains(row, transcriptSeparator) {
		t.Fatalf("mention row = %q, want the column separator %q", row, transcriptSeparator)
	}
}

// TestPhase8TranscriptActionNoticeEventShapes proves the three non-chat kinds
// keep their distinct shapes: the action marker, the dashed notice byline, and
// the centered event line, each one line per message.
func TestPhase8TranscriptActionNoticeEventShapes(t *testing.T) {
	m := seededModel(t)
	at := time.Date(2026, 9, 27, 12, 0, 0, 0, time.UTC)

	action := m.messageRow(0, controller.MessageSnapshot{Author: "mira", Kind: "action", Body: "waves", Time: at})
	if !strings.HasPrefix(action, m.styles.Action.Render("* ")) {
		t.Fatalf("action row = %q, want the %q marker", action, "* ")
	}
	if want := m.nickStyle("mira").Render("mira"); !strings.Contains(action, want) {
		t.Fatalf("action row = %q, want the palette-colored bold nick %q", action, want)
	}

	notice := m.messageRow(0, controller.MessageSnapshot{Author: "mira", Kind: "notice", Body: "hi", Time: at})
	if !strings.HasPrefix(notice, m.styles.Notice.Render("-")) {
		t.Fatalf("notice row = %q, want the dashed %q byline", notice, "-")
	}
	if want := m.nickStyle("mira").Render("mira"); !strings.Contains(notice, want) {
		t.Fatalf("notice row = %q, want the palette-colored bold nick %q", notice, want)
	}

	event := m.messageRow(0, controller.MessageSnapshot{Kind: "event", Body: "mira joined", Time: at})
	if !strings.HasPrefix(event, " ") {
		t.Fatalf("event row = %q, want it centered in the column", event)
	}
	for _, row := range []string{action, notice, event} {
		if strings.Count(row, "\n") != 0 {
			t.Fatalf("row = %q, want a single line", row)
		}
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

	lines, headerCount := m.transcriptLines()
	area := m.transcriptArea()
	if got, want := len(lines), headerCount+area.end(); got != want {
		t.Fatalf("line count = %d, want %d; a channel must append no footer", got, want)
	}
	dots := m.styles.MutedLine.Render("   ...")
	for _, line := range lines {
		if line == dots {
			t.Fatalf("channel transcript contains a typing dots row: %q", line)
		}
	}
}

// TestTranscriptHeaderCarriesTheTopicBand pins the header's read as a title: the
// topic row is a band spanning the transcript column, so the caption and the
// count sit on a surface instead of looking like one more transcript line. The
// band stays clear of the page (or it is invisible) and of the chip's raised fill
// (or the count disappears into it), and the blank separator row stays unfilled.
func TestTranscriptHeaderCarriesTheTopicBand(t *testing.T) {
	m := seededModel(t)
	band := backgroundParams(m.styles.TopicBar.GetBackground())
	if want := backgroundParams(mixColors(m.styles.Colors.Background, m.styles.Colors.Accent, topicBarTint)); band != want {
		t.Fatalf("band = %q, want the page mixed %.2f toward the accent (%q)", band, topicBarTint, want)
	}
	if band == backgroundParams(m.styles.Colors.Background) {
		t.Fatal("the band must differ from the page, or the header shows no band at all")
	}
	if band == backgroundParams(m.styles.Colors.SurfaceRaised) {
		t.Fatal("the band must differ from the chip's raised fill, or the count disappears into it")
	}

	header := m.transcriptHeader()
	if len(header) < 2 {
		t.Fatalf("channel header = %d rows, want the band plus its blank separator", len(header))
	}
	line := header[0]
	if got, want := lipgloss.Width(line), m.transcriptWidth(); got != want {
		t.Fatalf("band width = %d, want it to span the %d-cell transcript column", got, want)
	}
	if !strings.Contains(line, band) {
		t.Fatalf("header row carries no band background %q:\n%q", band, line)
	}
	// The band starts at the row's first cell: the topic is not rendered on the
	// page with the fill starting somewhere later.
	firstSGR := line
	if at := strings.Index(line, "m"); at >= 0 {
		firstSGR = line[:at]
	}
	if !strings.Contains(firstSGR, band) {
		t.Fatalf("header row does not open with the band:\n%q", line)
	}
	plain := ansiPattern.ReplaceAllString(line, "")
	for _, want := range []string{"A cozy corner for Omarchy users and builders.", "12 PEOPLE"} {
		if !strings.Contains(plain, want) {
			t.Fatalf("band row missing %q: %q", want, plain)
		}
	}
	if header[1] != "" {
		t.Fatalf("the row under the band must stay blank and unfilled: %q", header[1])
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
	// State the invariant directly: the band spans the column and the count
	// still ends one gutter before the member column.
	assertHeaderBandGutter(t, m.transcriptWidth(), header)
	if chip := m.peopleChip(12); lipgloss.Width(chip) != lipgloss.Width("12 PEOPLE")+2 {
		t.Fatalf("people chip width = %d, want the label width plus a one-cell pad each side", lipgloss.Width(chip))
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
