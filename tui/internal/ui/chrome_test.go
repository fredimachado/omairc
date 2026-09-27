package ui

// This file covers the transcript-grouped composer chrome and the sidebar's
// inter-network rule: the composer is inset under the transcript column with a
// raised fill, its real cursor moves with the field, the slash menu opens over
// the same column, and the roster draws a separator above every network after
// the first.

import (
	"strings"
	"testing"
)

// composerRowOf returns the rendered composer line from the framed frame, so a
// test can assert the field chrome without re-deriving the layout.
func composerRowOf(t *testing.T, m *Model) string {
	t.Helper()
	lines := strings.Split(m.View().Content, "\n")
	row := m.composerRow()
	if row < 0 || row >= len(lines) {
		t.Fatalf("composerRow() = %d outside %d rendered lines", row, len(lines))
	}
	return lines[row]
}

// TestComposerRowFitsTheGrid pins that the inset composer still leaves the
// frame exactly as tall as the terminal, so moving it cannot push a row off
// the bottom or scroll the alternate screen.
func TestComposerRowFitsTheGrid(t *testing.T) {
	m := seededModel(t)
	if got, want := len(strings.Split(m.View().Content, "\n")), m.height; got != want {
		t.Fatalf("rendered rows = %d, want the %d-row terminal", got, want)
	}
}

// TestComposerGroupsWithTranscript pins the new layout: the composer field is
// inset under the transcript column, so its visible block is a subset of the
// transcript's span rather than the full window width starting at the sidebar.
func TestComposerGroupsWithTranscript(t *testing.T) {
	m := seededModel(t)
	if !m.serverListVisible {
		t.Fatal("seeded layout must show the sidebar")
	}

	field := m.composerWidth()
	if field >= m.width {
		t.Fatalf("composer width = %d, want narrower than the %d-cell window", field, m.width)
	}
	if field != m.transcriptWidth()-2*composerInset {
		t.Fatalf("composer width = %d, want transcript column %d minus both insets",
			field, m.transcriptWidth())
	}
	// It sits inside the transcript column and leaves the inset on each side.
	if left := m.composerLeft(); left != sidebarWidth(m.width)+composerInset {
		t.Fatalf("composer left = %d, want sidebar + inset", left)
	}
	if right := m.composerLeft() + m.composerWidth(); right > m.width {
		t.Fatalf("composer reaches %d, past the %d-cell window", right, m.width)
	}
	// The slash menu, when open, opens over the same column.
	shifted := m.alignToComposer([]string{"menu"})
	if got := strings.Index(shifted[0], "menu"); got != m.composerLeft() {
		t.Fatalf("slash menu indent = %d, want composerLeft() %d", got, m.composerLeft())
	}
}

// TestComposerFieldCarriesSurfaceFill pins that the field's fill is the raised
// surface and that it reaches the end of the field, so the composer reads as
// its own block instead of a window-coloured bare line.
func TestComposerFieldCarriesSurfaceFill(t *testing.T) {
	m := seededModel(t)
	row := composerRowOf(t, m)
	plain := ansiPattern.ReplaceAllString(row, "")

	if !strings.HasPrefix(plain, strings.Repeat(" ", m.composerLeft())) {
		t.Fatalf("composer row must start with the column indent:\n%q", plain)
	}
	if got := len([]rune(strings.TrimRight(plain, " "))); got == 0 {
		t.Fatal("composer row rendered no content")
	}
	// The fill covers the whole field, not just its glyphs: the prompt, the
	// typed text, and the blank padding to the field's end each carry it.
	fill := backgroundParams(m.styles.Colors.Surface)
	if !strings.Contains(row, fill) {
		t.Fatalf("composer row missing the ComposerField fill %q:\n%q", fill, row)
	}
	if got := strings.Count(row, fill); got < 2 {
		t.Fatalf("surface fill appears %d times, want it behind the whole field:\n%q", got, row)
	}
}

// TestComposerCursorSitsInTheField pins that the real cursor follows the field's
// inset instead of staying at the window's left edge.
func TestComposerCursorSitsInTheField(t *testing.T) {
	m := seededModel(t)
	cursor := m.View().Cursor
	if cursor == nil {
		t.Fatal("focused composer must expose a terminal cursor")
	}
	if cursor.Position.X < m.composerLeft()+composerPrefixWidth {
		t.Fatalf("cursor column = %d, want at least composerLeft()+prompt = %d",
			cursor.Position.X, m.composerLeft()+composerPrefixWidth)
	}
	if limit := m.composerLeft() + m.composerWidth(); cursor.Position.X >= limit {
		t.Fatalf("cursor column = %d, past the field end %d", cursor.Position.X, limit)
	}
	if cursor.Position.Y != m.composerRow() {
		t.Fatalf("cursor row = %d, want composerRow() %d", cursor.Position.Y, m.composerRow())
	}
}

// TestSidebarSeparatesNetworks pins the roster rule: a divider line sits above
// every network after the first and directly under the previous section, while
// the first network keeps the header at the top of the column.
func TestSidebarSeparatesNetworks(t *testing.T) {
	m := seededModel(t)
	const width = 30
	rendered := m.sidebarView(width, m.bodyHeight())

	// The separator sits directly above the second network's header, and the
	// first network's header stays on the first roster line.
	rows := strings.Split(rendered, "\n")
	plain := make([]string, len(rows))
	for index, row := range rows {
		plain[index] = strings.TrimRight(ansiPattern.ReplaceAllString(row, ""), " ")
	}
	headerIndex := -1
	for index, row := range plain {
		if strings.Contains(row, "▾ irc.example · oak") {
			headerIndex = index
			break
		}
	}
	if headerIndex < 1 || plain[headerIndex-1] != strings.Repeat("─", width) {
		t.Fatalf("separator is not above the second network header:\n%s", strings.Join(plain, "\n"))
	}
	if !strings.Contains(plain[0], "▾ irc.example · fred") {
		t.Fatalf("first network header is not the top roster line: %q", plain[0])
	}
}

// TestSidebarSeparatorCount pins that a roster of N networks draws exactly N-1
// rules: one above each network after the first, never a leading one.
func TestSidebarSeparatorCount(t *testing.T) {
	m := seededModel(t)
	const width = 30
	rules := 0
	for _, row := range strings.Split(m.sidebarView(width, m.bodyHeight()), "\n") {
		if strings.TrimRight(ansiPattern.ReplaceAllString(row, ""), " ") == strings.Repeat("─", width) {
			rules++
		}
	}
	if want := len(m.ctrl.NetworkOrder()) - 1; rules != want {
		t.Fatalf("sidebar separators = %d, want %d", rules, want)
	}
	if rules == 0 {
		t.Fatal("seeded roster must have more than one network to separate")
	}
}

// TestNetworkSeparatorWidth pins the rule renderer: a rule reaches the column
// width, and a zero width yields no line so a tiny column never draws a stray
// glyph.
func TestNetworkSeparatorWidth(t *testing.T) {
	m := seededModel(t)
	if got := ansiPattern.ReplaceAllString(m.networkSeparator(20), ""); got != strings.Repeat("─", 20) {
		t.Fatalf("separator = %q, want a 20-cell rule", got)
	}
	if got := m.networkSeparator(0); got != "" {
		t.Fatalf("zero-width separator = %q, want empty", got)
	}
}
