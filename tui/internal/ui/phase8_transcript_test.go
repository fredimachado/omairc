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

// TestPhase8TranscriptChannelHeaderHasNoPeopleCount proves the channel header
// carries the topic only: the member count belongs to the member column's
// "ONLINE - N" heading, so the transcript header must not repeat it. The band
// still spans the transcript column, and the count still renders once, in the
// member panel.
func TestPhase8TranscriptChannelHeaderHasNoPeopleCount(t *testing.T) {
	m := seededModel(t)
	if !m.ctrl.IsChannel() {
		t.Fatalf("seeded selection target = %q, want a channel", m.ctrl.SelectedTarget())
	}
	if got := m.ctrl.PeopleCount(); got != 12 {
		t.Fatalf("PeopleCount = %d, want 12", got)
	}
	if !m.membersVisible() {
		t.Fatal("the member column must be visible at the seeded width")
	}

	lines, headerCount := m.transcriptLines()
	if headerCount < 1 {
		t.Fatal("a channel transcript must have a topic header")
	}
	header := lines[0]
	plain := ansiPattern.ReplaceAllString(header, "")
	for _, unwanted := range []string{"PEOPLE", "(12)"} {
		if strings.Contains(plain, unwanted) {
			t.Fatalf("channel header = %q, must not carry %q", plain, unwanted)
		}
	}
	// The band still spans the column: dropping the count leaves the topic's
	// title strip intact.
	if got, want := lipgloss.Width(header), m.transcriptWidth(); got != want {
		t.Fatalf("channel header width = %d, want the full %d-cell band", got, want)
	}

	// The count survives in exactly one place: the member column's heading.
	content := m.View().Content
	if !strings.Contains(content, "ONLINE - 12") {
		t.Fatalf("rendered content missing the member heading %q:\n%s", "ONLINE - 12", content)
	}
	if got := strings.Count(ansiPattern.ReplaceAllString(content, ""), "12 PEOPLE"); got != 0 {
		t.Fatalf("the transcript still repeats the count %d times:\n%s", got, content)
	}
}

// TestPhase8TranscriptDirectHeaderIsJustTheTopic proves a direct message header
// is only the topic caption: no count, and no right-aligned tail.
func TestPhase8TranscriptDirectHeaderIsJustTheTopic(t *testing.T) {
	m := openPhase8AnnaDirect(t, seededModel(t))

	lines, _ := m.transcriptLines()
	if len(lines) == 0 {
		t.Fatal("the anna DM transcript must have a header")
	}
	header := lines[0]
	plain := ansiPattern.ReplaceAllString(header, "")
	if !strings.Contains(plain, "Direct message with anna") {
		t.Fatalf("DM header = %q, want the direct-message caption", plain)
	}
	if strings.Contains(plain, "PEOPLE") {
		t.Fatalf("DM header = %q, must not carry a people count", plain)
	}
	if got, want := lipgloss.Width(header), m.transcriptWidth(); got != want {
		t.Fatalf("DM header width = %d, want the full %d-cell band", got, want)
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
		t.Fatal("the band must differ from the raised surface, or it reads as a raised card")
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
	if !strings.Contains(plain, "A cozy corner for Omarchy users and builders.") {
		t.Fatalf("band row missing the topic caption: %q", plain)
	}
	if strings.Contains(plain, "PEOPLE") {
		t.Fatalf("band row must carry the topic only, not the member count: %q", plain)
	}
	if header[1] != "" {
		t.Fatalf("the row under the band must stay blank and unfilled: %q", header[1])
	}
}

// TestPhase8TranscriptHeaderTailKeepsItsGutter pins the header's right inset:
// the band spans the transcript column, and the "↓ new" tail ends
// headerTailGutter cells short of it so it never crowds the member column's
// "ONLINE - N" heading, which shares the row on the other side of the border.
func TestPhase8TranscriptHeaderTailKeepsItsGutter(t *testing.T) {
	m, d := unseenDemoModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	d.InjectOmarchy([]byte("@msgid=gutter-1 :nora!u@h PRIVMSG #omarchy :while reading\r\n"))
	m = updateMsg(t, m, NotifyMsg{})
	if !m.jumpArmed() {
		t.Fatal("an arrival while scrolled up must arm the tail marker")
	}

	header := m.transcriptHeader()
	if len(header) == 0 {
		t.Fatal("the seeded channel must have a topic header")
	}
	line := header[0]
	if got, want := lipgloss.Width(line), m.transcriptWidth(); got != want {
		t.Fatalf("header band width = %d, want the full %d-cell column", got, want)
	}
	plain := []rune(ansiPattern.ReplaceAllString(line, ""))
	if !strings.Contains(string(plain), "↓ new") {
		t.Fatalf("header = %q, want the armed tail marker", string(plain))
	}
	if len(plain) < headerTailGutter {
		t.Fatalf("header = %q, too narrow for the %d-cell gutter", string(plain), headerTailGutter)
	}
	if trailing := string(plain[len(plain)-headerTailGutter:]); strings.TrimSpace(trailing) != "" {
		t.Fatalf("header = %q, want the tail to end %d gutter cells short of the column edge",
			string(plain), headerTailGutter)
	}
}
