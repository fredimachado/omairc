package ui

// This file covers the transcript-grouped composer chrome and the sidebar's
// inter-network rule: the composer is inset under the transcript column with a
// raised fill, its real cursor moves with the field, the slash menu opens over
// the same column, and the roster draws a separator above every network after
// the first.

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
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

// TestComposerIsIndependentOfTheSidebar pins the layout contract: the composer is
// a window-level bar whose left edge and width come from the window alone. It
// spans the window (minus the two insets) and does not move or resize when the
// sidebar is toggled, so a column change can never drag the composer around.
func TestComposerIsIndependentOfTheSidebar(t *testing.T) {
	m := seededModel(t)
	if !m.serverListVisible {
		t.Fatal("seeded layout must show the sidebar")
	}

	shownLeft, shownWidth := m.composerLeft(), m.composerWidth()
	if want := composerInset; shownLeft != want {
		t.Fatalf("composer left = %d, want the window inset %d", shownLeft, want)
	}
	if want := m.width - 2*composerInset; shownWidth != want {
		t.Fatalf("composer width = %d, want the window minus both insets %d", shownWidth, want)
	}
	if right := shownLeft + shownWidth; right > m.width {
		t.Fatalf("composer reaches %d, past the %d-cell window", right, m.width)
	}

	// Hiding the sidebar must not move or resize the composer.
	m.toggleServerList()
	if m.serverListVisible {
		t.Fatal("Ctrl+Shift+S must hide the sidebar")
	}
	if got := m.composerLeft(); got != shownLeft {
		t.Fatalf("composer left moved %d -> %d when the sidebar hid", shownLeft, got)
	}
	if got := m.composerWidth(); got != shownWidth {
		t.Fatalf("composer width changed %d -> %d when the sidebar hid", shownWidth, got)
	}

	// The slash menu still opens over the transcript column, right of the
	// sidebar, so it never covers the sidebar it is independent of.
	m.toggleServerList()
	if m.slashMenuLeft() < sidebarWidth(m.width) {
		t.Fatalf("slash menu left = %d, must clear the %d-wide sidebar",
			m.slashMenuLeft(), sidebarWidth(m.width))
	}
}

// TestSlashMenuFloatsWithoutResizingColumns pins that opening the
// slash-completion menu does not resize the columns. The menu used to be a band
// stacked between the body and the composer, so it took its rows from the column
// budget and shrank the sidebar (and the transcript and member panel) while the
// user typed a slash command. It now floats over the body's last rows, so the
// columns keep their height and the menu still renders on screen.
func TestSlashMenuFloatsWithoutResizingColumns(t *testing.T) {
	m := seededModel(t)
	sidebarHeight := func(mm *Model) int {
		return lipgloss.Height(mm.framedColumn(mm.sidebarView, sidebarWidth(mm.width), mm.bodyHeight(), false))
	}
	closedBody, closedSidebar := m.bodyHeight(), sidebarHeight(m)

	m.composer.SetValue("/j")
	m.syncSlash()
	if !m.slash.open() {
		t.Fatal("/j must open the slash menu")
	}
	if got := m.bodyHeight(); got != closedBody {
		t.Fatalf("body height changed %d -> %d when the menu opened", closedBody, got)
	}
	if got := sidebarHeight(m); got != closedSidebar {
		t.Fatalf("sidebar height changed %d -> %d when the menu opened", closedSidebar, got)
	}
	// The floated menu is still rendered, and the grid still fits exactly.
	content := m.render()
	if got, want := len(strings.Split(content, "\n")), m.height; got != want {
		t.Fatalf("frame = %d rows, want the %d-row window", got, want)
	}
	if plain := ansiPattern.ReplaceAllString(content, ""); !strings.Contains(plain, "/join") {
		t.Fatalf("the floated slash menu is not rendered:\n%s", plain)
	}

	// The menu floats over the transcript column, so every sidebar row keeps its
	// left and right border cells. An indented layer drawn from column zero used
	// to blank them out and erase the sidebar on the menu's rows.
	sidebar := sidebarWidth(m.width)
	for index, line := range strings.Split(content, "\n") {
		if index >= closedBody {
			break // the composer and footer sit below the body
		}
		plain := []rune(ansiPattern.ReplaceAllString(line, ""))
		if len(plain) < sidebar {
			t.Fatalf("sidebar row %d is narrower than the %d-cell column:\n%s", index, sidebar, line)
		}
		left, right := plain[0], plain[sidebar-1]
		if !strings.ContainsRune("│╭╮╰╯", left) || !strings.ContainsRune("│╭╮╰╯", right) {
			t.Fatalf("sidebar borders erased on row %d (menu rows are %d..%d): %q",
				index, closedBody-len(m.slashLines()), closedBody-1, string(plain[:sidebar]))
		}
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
