package ui

import (
	"strings"
	"testing"
	"time"

	"charm.land/lipgloss/v2"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/irc"
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
// carries the channel name and topic, not the member count: the count belongs to
// the member column's "ONLINE - N" heading. The band still spans the transcript
// column, and the count still renders once, in the member panel.
func TestPhase8TranscriptChannelHeaderHasNoPeopleCount(t *testing.T) {
	m := seededModel(t)
	channel := m.ctrl.SelectedTarget()
	if !m.ctrl.IsChannel() {
		t.Fatalf("seeded selection target = %q, want a channel", channel)
	}
	if got := m.ctrl.PeopleCount(); got != 12 {
		t.Fatalf("PeopleCount = %d, want 12", got)
	}
	if !m.membersVisible() {
		t.Fatal("the member column must be visible at the seeded width")
	}

	lines, headerCount := m.transcriptLines()
	if headerCount < 2 {
		t.Fatal("a channel transcript must have a name band, topic band, and blank")
	}
	titlePlain := ansiPattern.ReplaceAllString(lines[0], "")
	if !strings.Contains(titlePlain, channel) {
		t.Fatalf("channel title = %q, want the selected target %q", titlePlain, channel)
	}
	headerPlain := ansiPattern.ReplaceAllString(strings.Join(lines[:headerCount], "\n"), "")
	if !strings.Contains(headerPlain, "A cozy corner for Omarchy users and builders.") {
		t.Fatalf("channel header = %q, want the seeded topic", headerPlain)
	}
	for _, unwanted := range []string{"PEOPLE", "(12)"} {
		if strings.Contains(headerPlain, unwanted) {
			t.Fatalf("channel header = %q, must not carry %q", headerPlain, unwanted)
		}
	}
	// The band still spans the column.
	if got, want := lipgloss.Width(lines[0]), m.transcriptWidth(); got != want {
		t.Fatalf("channel title width = %d, want the full %d-cell band", got, want)
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

// TestPhase8TranscriptDirectHeaderShowsPeerFacts proves a direct message header
// shows who you are talking to: a presence dot, the nick, short labels, and
// the real name. The topic property stays the direct-message caption, and the
// header still has no people count.
func TestPhase8TranscriptDirectHeaderShowsPeerFacts(t *testing.T) {
	m := openPhase8AnnaDirect(t, seededModel(t))
	if got := m.ctrl.Topic(); got != "Direct message with anna" {
		t.Fatalf("Topic() = %q, want the direct-message caption", got)
	}

	lines, _ := m.transcriptLines()
	if len(lines) < 3 {
		t.Fatalf("anna header has %d lines, want the title, real name, and blank", len(lines))
	}
	title := ansiPattern.ReplaceAllString(lines[0], "")
	name := ansiPattern.ReplaceAllString(lines[1], "")
	if !strings.Contains(title, "anna") {
		t.Fatalf("DM title = %q, want the nick", title)
	}
	if strings.Contains(title, "Direct message with anna") || strings.Contains(name, "Direct message with anna") {
		t.Fatalf("DM header still shows the caption:\n%s\n%s", title, name)
	}
	if strings.Contains(title, "PEOPLE") || strings.Contains(name, "PEOPLE") {
		t.Fatalf("DM header must not carry a people count:\n%s", title)
	}
	if !strings.Contains(lines[0], m.styles.MemberPresenceOnline.Render("●")) {
		t.Fatalf("anna's header dot is not online: %q", lines[0])
	}
	if !strings.Contains(name, "Anna Vale") {
		t.Fatalf("DM subtitle = %q, want Anna Vale", name)
	}
	if strings.Contains(name, "Anna Docs") {
		t.Fatalf("DM subtitle used the display-name: %q", name)
	}
	if strings.TrimSpace(ansiPattern.ReplaceAllString(lines[2], "")) != "" {
		t.Fatalf("the header separator is not blank: %q", lines[2])
	}
	if got, want := lipgloss.Width(lines[0]), m.transcriptWidth(); got != want {
		t.Fatalf("DM title width = %d, want the full %d-cell band", got, want)
	}
	if got, want := lipgloss.Width(lines[1]), m.transcriptWidth(); got != want {
		t.Fatalf("DM name width = %d, want the full %d-cell band", got, want)
	}

	if !m.ctrl.OpenDirectMessage("ivy") {
		t.Fatal("opening ivy's query failed")
	}
	lines, _ = m.transcriptLines()
	if len(lines) < 2 {
		t.Fatal("ivy's query must have a header")
	}
	ivy := ansiPattern.ReplaceAllString(lines[0], "")
	if !strings.Contains(ivy, "ivy") || !strings.Contains(ivy, "unauthenticated") {
		t.Fatalf("ivy header = %q, want the nick and unauthenticated", ivy)
	}
	if !strings.Contains(lines[0], m.styles.MemberPresenceAway.Render("●")) {
		t.Fatalf("ivy's header dot is not away: %q", lines[0])
	}
	if strings.TrimSpace(ansiPattern.ReplaceAllString(lines[1], "")) != "" {
		t.Fatalf("ivy has no real name, so the next header line must be blank: %q", lines[1])
	}

	assertQueryHeader(t, m, "dax", "Packet Bot", []string{"bot"},
		m.styles.MemberPresenceOnline.Render("●"))
	assertQueryHeader(t, m, "lena", "Lena Pink", []string{"pinkieval"},
		m.styles.MemberPresenceAway.Render("●"))
	assertQueryHeader(t, m, "fred", "Fred Machado", []string{"fredm", "server operator"},
		m.styles.MemberPresenceOnline.Render("●"))
	assertQueryHeader(t, m, "ghost", "", nil, m.styles.MemberAccount.Render("●"))

	// rio already has a query on OFTC. Opening one on Omarchy makes the nick
	// ambiguous, so the title uses the same network suffix as the jump list.
	m.ctrl.SelectConversation("omarchy", "#omarchy")
	if !m.ctrl.OpenDirectMessage("rio") {
		t.Fatal("opening rio on omarchy failed")
	}
	lines, _ = m.transcriptLines()
	rio := ansiPattern.ReplaceAllString(lines[0], "")
	if !strings.Contains(rio, "rio · irc.example · fred") {
		t.Fatalf("duplicated rio header = %q, want the jump-list network suffix", rio)
	}

	dropAwayNotify(m)
	if !m.ctrl.OpenDirectMessage("anna") {
		t.Fatal("reopening anna after dropping away-notify failed")
	}
	lines, _ = m.transcriptLines()
	anna := ansiPattern.ReplaceAllString(lines[0], "")
	if strings.Contains(anna, "●") {
		t.Fatalf("anna header without away-notify still paints a presence glyph: %q", anna)
	}
	if !strings.Contains(anna, "anna") || !strings.Contains(lines[1], "Anna Vale") {
		t.Fatalf("anna header without away-notify = %q / %q, want the nick and real name",
			anna, ansiPattern.ReplaceAllString(lines[1], ""))
	}
	m.ctrl.SelectConversation("omarchy", "#omarchy")
	m.memberFocus = true
	m.memberIndex = phase8MemberIndex(t, m, "dax")
	panel := ansiPattern.ReplaceAllString(m.membersView(membersWidth, 40), "")
	if !panelHasFact(panel, "Packet Bot") {
		t.Fatalf("dax tip without away-notify = %q, want it to start at the real name", panel)
	}
	for _, line := range strings.Split(panel, "\n") {
		if strings.TrimSpace(line) == "online" {
			t.Fatalf("member tip without away-notify starts with online:\n%s", panel)
		}
	}
}

// assertQueryHeader opens nick and checks the painted title, subtitle, labels,
// and presence glyph.
func assertQueryHeader(t *testing.T, m *Model, nick, realname string, labels []string, dot string) {
	t.Helper()
	if !m.ctrl.OpenDirectMessage(nick) {
		t.Fatalf("opening %s failed", nick)
	}
	lines, _ := m.transcriptLines()
	if len(lines) < 2 {
		t.Fatalf("%s header has %d lines", nick, len(lines))
	}
	title := ansiPattern.ReplaceAllString(lines[0], "")
	if !strings.Contains(title, nick) {
		t.Fatalf("%s title = %q, want the nick", nick, title)
	}
	for _, label := range labels {
		if !strings.Contains(title, label) {
			t.Fatalf("%s title = %q, want label %q", nick, title, label)
		}
	}
	if !strings.Contains(lines[0], dot) {
		t.Fatalf("%s presence glyph missing from %q", nick, lines[0])
	}
	subtitle := strings.TrimSpace(ansiPattern.ReplaceAllString(lines[1], ""))
	if subtitle != realname {
		t.Fatalf("%s subtitle = %q, want %q", nick, subtitle, realname)
	}
}

// dropAwayNotify replaces the negotiated set with one that lacks away-notify
// and keeps the capabilities that do not gate the header glyph.
func dropAwayNotify(m *Model) {
	replacement := irc.CapabilitySet(0)
	for _, capability := range []irc.Capability{
		irc.CapabilityBatch,
		irc.CapabilityMemberMetadata,
		irc.CapabilityMessageTags,
		irc.CapabilityAccountNotify,
		irc.CapabilityExtendedJoin,
	} {
		replacement.Insert(capability)
	}
	m.ctrl.CapabilitiesChanged("omarchy", replacement)
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
	if !strings.Contains(dots, m.typingDots()) {
		t.Fatalf("footer dots = %q, want the animated typing glyph", dots)
	}
	if plain := ansiPattern.ReplaceAllString(dots, ""); !strings.HasPrefix(plain, " ") {
		t.Fatalf("footer dots = %q, want it indented to the body column", plain)
	}
}

// groupedTypingModel seeds the demo on a clock inside the anna DM row's
// displayed minute, in UTC+10, so the typing footer groups under that row. It
// mirrors the grouped case Qt reaches while the peer has just spoken.
func groupedTypingModel(t *testing.T) *Model {
	t.Helper()
	zone := time.FixedZone("UTC+10", 10*60*60)
	ctrl := controller.New()
	ctrl.SetClock(session.NewFakeClock(
		time.Date(2026, 9, 12, 10, 12, 30, 0, time.UTC).In(zone)))
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := New(ctrl, nil)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	return updated.(*Model)
}

// TestPhase8TranscriptDirectTypingFooterGrouped proves the grouped footer drops
// the byline but keeps the dots on their own line at the body column. Grouping
// must not fold them into the peer's message: cover the avatar/header only.
func TestPhase8TranscriptDirectTypingFooterGrouped(t *testing.T) {
	m := openPhase8AnnaDirect(t, groupedTypingModel(t))

	nick, grouped, show := m.ctrl.TranscriptTypingIndicator()
	if nick != "anna" || !grouped || !show {
		t.Fatalf("indicator = (%q, %v, %v), want (anna, true, true)", nick, grouped, show)
	}

	lines, headerCount := m.transcriptLines()
	area := m.transcriptArea()
	// Only the dots line follows the last row; the byline is suppressed.
	if got, want := len(lines), headerCount+area.end()+1; got != want {
		t.Fatalf("line count = %d, want %d (rows plus the one grouped dots line)", got, want)
	}
	dots := lines[headerCount+area.end()]
	if !strings.Contains(dots, m.typingDots()) {
		t.Fatalf("grouped footer = %q, want the typing dots", dots)
	}
	if repeated := m.nickStyle("anna").Render("anna"); strings.Contains(dots, repeated) {
		t.Fatalf("grouped footer = %q, must not repeat the peer's nick", dots)
	}
	if plain := ansiPattern.ReplaceAllString(dots, ""); !strings.HasPrefix(plain, " ") {
		t.Fatalf("grouped footer = %q, want it indented to the body column", plain)
	}
}

// TestGroupedTypingFooterStaysOffTheWrappedBody proves the dots never share a
// line with the peer's body. A message row is one entry that can hold several
// physical lines once the body wraps, so folding the dots into it left them
// glued to the last word of the body instead of standing on their own row,
// which is what Qt's typingRow renders (avatar and header hidden, dots still on
// their own row).
func TestGroupedTypingFooterStaysOffTheWrappedBody(t *testing.T) {
	// A narrow window wraps the seeded anna body across several lines, which is
	// the shape that made the folded footer visible.
	ctrl := controller.New()
	ctrl.SetClock(session.NewFakeClock(
		time.Date(2026, 9, 12, 10, 12, 30, 0, time.UTC).In(time.FixedZone("UTC+10", 10*60*60))))
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	narrow := New(ctrl, nil)
	updated, _ := narrow.Update(tea.WindowSizeMsg{Width: 58, Height: 30})
	m := openPhase8AnnaDirect(t, updated.(*Model))

	if _, grouped, show := m.ctrl.TranscriptTypingIndicator(); !grouped || !show {
		t.Fatalf("indicator = (grouped %v, show %v), want the grouped footer", grouped, show)
	}

	area := m.transcriptArea()
	lastRow := area.count() - 1
	if lastRow < 0 {
		t.Fatal("the anna DM must have a message row")
	}
	if area.heights[lastRow] < 2 {
		t.Fatalf("the seeded body must wrap to exercise the bug (row height %d)",
			area.heights[lastRow])
	}

	lines, _ := m.transcriptLines()
	dots := m.typingDots()
	body := "Nice work."
	for index, line := range lines {
		if strings.Contains(line, dots) && strings.Contains(line, body) {
			t.Fatalf("line %d mixes the body and the dots: %q",
				index, ansiPattern.ReplaceAllString(line, ""))
		}
	}
	if footer := lines[len(lines)-1]; !strings.Contains(footer, dots) {
		t.Fatalf("last line = %q, want the dots on their own footer line",
			ansiPattern.ReplaceAllString(footer, ""))
	}
}

// TestTranscriptRendersClocksInTheClockZone pins the displayed clock end to
// end. The seeded anna DM row carries a 10:12Z server-time tag, and the byline
// must read that instant in the reader's zone, the way Qt's displayTime calls
// toLocalTime, instead of echoing the raw UTC cell.
func TestTranscriptRendersClocksInTheClockZone(t *testing.T) {
	zone := time.FixedZone("UTC+10", 10*60*60)
	ctrl := controller.New()
	ctrl.SetClock(session.NewFakeClock(time.Date(2026, 9, 12, 10, 12, 30, 0, time.UTC).In(zone)))
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := New(ctrl, nil)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = openPhase8AnnaDirect(t, updated.(*Model))

	lines, _ := m.transcriptLines()
	plain := ansiPattern.ReplaceAllString(strings.Join(lines, "\n"), "")
	if !strings.Contains(plain, "20:12") {
		t.Fatalf("transcript clock = %q, want the local 20:12", plain)
	}
	if strings.Contains(plain, "10:12") {
		t.Fatalf("transcript clock = %q, want the UTC cell localized away", plain)
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
	for _, frame := range m.typingSpinner.Spinner.Frames {
		stamped := m.styles.MemberTyping.Render(frame)
		for _, line := range lines {
			if strings.Contains(line, stamped) {
				t.Fatalf("channel transcript contains a typing dots row: %q", line)
			}
		}
	}
}

// TestTranscriptHeaderCarriesTheTopicBand pins the header's read as a title: each
// header row is a band spanning the transcript column, so the caption sits on a
// surface instead of looking like one more transcript line. The band stays clear
// of the page (or it is invisible) and of the chip's raised fill (or the count
// disappears into it), and the blank separator row stays unfilled.
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
	if len(header) < 3 {
		t.Fatalf("channel header = %d rows, want the name band, topic band, and blank", len(header))
	}
	for index, line := range header[:2] {
		if got, want := lipgloss.Width(line), m.transcriptWidth(); got != want {
			t.Fatalf("band %d width = %d, want it to span the %d-cell transcript column", index, got, want)
		}
		if !strings.Contains(line, band) {
			t.Fatalf("header row %d carries no band background %q:\n%q", index, band, line)
		}
		// The band starts at the row's first cell.
		firstSGR := line
		if at := strings.Index(line, "m"); at >= 0 {
			firstSGR = line[:at]
		}
		if !strings.Contains(firstSGR, band) {
			t.Fatalf("header row %d does not open with the band:\n%q", index, line)
		}
	}
	titlePlain := ansiPattern.ReplaceAllString(header[0], "")
	if !strings.Contains(titlePlain, m.ctrl.SelectedTarget()) {
		t.Fatalf("name band missing the channel: %q", titlePlain)
	}
	topicPlain := ansiPattern.ReplaceAllString(header[1], "")
	if !strings.Contains(topicPlain, "A cozy corner for Omarchy users and builders.") {
		t.Fatalf("topic band missing the caption: %q", topicPlain)
	}
	if strings.Contains(topicPlain, "PEOPLE") {
		t.Fatalf("topic band must not carry the member count: %q", topicPlain)
	}
	if header[2] != "" {
		t.Fatalf("the row under the bands must stay blank and unfilled: %q", header[2])
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
	if len(header) < 2 {
		t.Fatal("the seeded channel must have a name band and a topic band")
	}
	line := header[1]
	if got, want := lipgloss.Width(line), m.transcriptWidth(); got != want {
		t.Fatalf("header band width = %d, want the full %d-cell column", got, want)
	}
	plain := []rune(ansiPattern.ReplaceAllString(line, ""))
	if !strings.Contains(string(plain), "↓ new") {
		t.Fatalf("topic band = %q, want the armed tail marker", string(plain))
	}
	if len(plain) < headerTailGutter {
		t.Fatalf("topic band = %q, too narrow for the %d-cell gutter", string(plain), headerTailGutter)
	}
	if trailing := string(plain[len(plain)-headerTailGutter:]); strings.TrimSpace(trailing) != "" {
		t.Fatalf("topic band = %q, want the tail to end %d gutter cells short of the column edge",
			string(plain), headerTailGutter)
	}
}

// TestPhase8TranscriptChannelHeaderSurvivesSidebarHide proves the channel name
// stays in the pinned header when the server list is collapsed.
func TestPhase8TranscriptChannelHeaderSurvivesSidebarHide(t *testing.T) {
	m := seededModel(t)
	channel := m.ctrl.SelectedTarget()
	m = press(t, m, ctrlShiftKey('s'))
	if m.serverListVisible {
		t.Fatal("Ctrl+Shift+S must hide the sidebar")
	}
	plain := ansiPattern.ReplaceAllString(strings.Join(m.transcriptHeader(), "\n"), "")
	if !strings.Contains(plain, channel) {
		t.Fatalf("header with sidebar hidden = %q, want %q", plain, channel)
	}
}

// TestPhase8TranscriptChannelHeaderWithoutTopic proves a topic-less channel
// still shows its name in the pinned header.
func TestPhase8TranscriptChannelHeaderWithoutTopic(t *testing.T) {
	m := seededModel(t)
	channel := m.ctrl.SelectedTarget()
	m.ctrl.Apply(irc.TopicEvent{NetworkID: "omarchy", Channel: channel, Topic: ""})
	if topic := strings.TrimSpace(m.ctrl.PlainIrcText(m.ctrl.Topic())); topic != "" {
		t.Fatalf("Topic() = %q, want empty after clearing", topic)
	}
	header := m.transcriptHeader()
	if len(header) != 2 {
		t.Fatalf("header = %d rows, want the name band and blank", len(header))
	}
	plain := ansiPattern.ReplaceAllString(header[0], "")
	if !strings.Contains(plain, channel) {
		t.Fatalf("title band = %q, want %q", plain, channel)
	}
	if strings.Contains(plain, "A cozy corner") {
		t.Fatalf("title band still shows the old topic: %q", plain)
	}
}

// TestPhase8TranscriptChannelHeaderDuplicateTarget proves a channel that exists
// on more than one network disambiguates with the focused network display name.
func TestPhase8TranscriptChannelHeaderDuplicateTarget(t *testing.T) {
	m := seededModel(t)
	m.ctrl.SelectConversation("oftc", "#omarchy")
	networkName := m.sidebarNetworkDisplayName("oftc")
	want := "#omarchy · " + networkName
	plain := ansiPattern.ReplaceAllString(m.transcriptHeader()[0], "")
	if !strings.Contains(plain, want) {
		t.Fatalf("duplicate header = %q, want %q", plain, want)
	}
}
