package ui

// This file covers the transcript-grouped composer chrome and the sidebar's
// inter-network rule: the composer is inset under the transcript column with a
// raised fill, its real cursor moves with the field, the slash menu opens over
// the same column, and the roster draws a separator above every network after
// the first.

import (
	"reflect"
	"strings"
	"testing"

	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/version"
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

// TestComposerLivesInTheTranscriptColumn pins the layout contract: the composer
// is the middle column's last block, not a window-level bar. Its left edge and
// width come from the transcript column, so toggling the sidebar carries the
// field with the transcript instead of leaving it behind.
func TestComposerLivesInTheTranscriptColumn(t *testing.T) {
	m := seededModel(t)
	if !m.serverListVisible {
		t.Fatal("seeded layout must show the sidebar")
	}

	shownLeft, shownWidth := m.composerLeft(), m.composerWidth()
	if want := m.transcriptLeft() + composerInset; shownLeft != want {
		t.Fatalf("composer left = %d, want the transcript column edge plus inset %d", shownLeft, want)
	}
	if want := m.transcriptWidth() - 2*composerInset; shownWidth != want {
		t.Fatalf("composer width = %d, want the transcript column minus both insets %d", shownWidth, want)
	}
	if right := shownLeft + shownWidth; right > m.transcriptLeft()+m.transcriptWidth() {
		t.Fatalf("composer reaches %d, past the %d-cell transcript column",
			right, m.transcriptLeft()+m.transcriptWidth())
	}

	// Hiding the sidebar carries the composer left and widens it: it follows the
	// transcript column it belongs to.
	m.toggleServerList()
	if m.serverListVisible {
		t.Fatal("Ctrl+Shift+S must hide the sidebar")
	}
	if got, want := m.composerLeft(), m.transcriptLeft()+composerInset; got != want {
		t.Fatalf("composer left = %d after the sidebar hid, want the column edge %d", got, want)
	}
	if got := m.composerLeft(); got >= shownLeft {
		t.Fatalf("composer left did not move left with the column: %d -> %d", shownLeft, got)
	}
	if got := m.composerWidth(); got <= shownWidth {
		t.Fatalf("composer width did not grow with the column: %d -> %d", shownWidth, got)
	}

	// The slash menu opens over the composer's own column, right of the sidebar,
	// so it never covers the sidebar the field no longer spans.
	m.toggleServerList()
	if got, want := m.slashMenuLeft(), m.composerLeft(); got != want {
		t.Fatalf("slash menu left = %d, want the composer's column edge %d", got, want)
	}
	if m.slashMenuLeft() < sidebarWidth(m.width) {
		t.Fatalf("slash menu left = %d, must clear the %d-wide sidebar",
			m.slashMenuLeft(), sidebarWidth(m.width))
	}
}

// TestSideColumnsRunBesideTheComposer pins that the floor under the composer
// belongs to the side rails too: because the composer is the middle column's
// last block, the sidebar and member cards run the full body height and still
// frame the composer's own row instead of stopping above the field.
func TestSideColumnsRunBesideTheComposer(t *testing.T) {
	m := seededModel(t)
	if !m.membersVisible() {
		t.Fatal("seeded channel must show the member column")
	}
	footerRows := 0
	if m.footerVisible() {
		footerRows = footerHeight
	}
	want := m.height - footerRows
	if got := lipgloss.Height(m.framedColumn(m.sidebarView, sidebarWidth(m.width), m.bodyHeight(), false)); got != want {
		t.Fatalf("framed sidebar = %d rows, want the %d-row body", got, want)
	}
	if got := lipgloss.Height(m.framedColumn(m.membersView, membersWidth, m.bodyHeight(), false)); got != want {
		t.Fatalf("framed member panel = %d rows, want the %d-row body", got, want)
	}

	// On the composer's own row the two side rails are still present, with the
	// field between them.
	row := ansiPattern.ReplaceAllString(strings.Split(m.View().Content, "\n")[m.composerRow()], "")
	runes := []rune(row)
	if len(runes) != m.width {
		t.Fatalf("composer row = %d cells, want the %d-cell window: %q", len(runes), m.width, row)
	}
	sidebarEdge := m.transcriptLeft() - 1
	if runes[sidebarEdge] != '│' {
		t.Fatalf("sidebar rail missing on the composer row (cell %d = %q): %q", sidebarEdge, runes[sidebarEdge], row)
	}
	membersEdge := m.transcriptLeft() + m.transcriptWidth()
	if runes[membersEdge] != '│' {
		t.Fatalf("member rail missing on the composer row (cell %d = %q): %q", membersEdge, runes[membersEdge], row)
	}
	if field := string(runes[m.composerLeft():membersEdge]); !strings.Contains(field, composerPrompt) {
		t.Fatalf("composer box is not between the side rails: %q", field)
	}
}

// TestStatusConsoleHidesMembersColumn pins the Qt parity of MembersColumn's
// !consoleVisible gate (src/OmaircWindow.qml). Opening Status keeps the last
// channel as the controller's selection, so IsChannel() stays true; the member
// column must still hide. Connecting a second network was the visible symptom:
// applying a profile opens Status while the previous channel's panel stayed on
// screen beside the console transcript.
func TestStatusConsoleHidesMembersColumn(t *testing.T) {
	m := seededModel(t)
	if !m.membersVisible() {
		t.Fatal("precondition: seeded channel must show the member column")
	}
	widthWithMembers := m.transcriptWidth()
	m.toggleStatus()
	if !m.ctrl.ConsoleOpen() {
		t.Fatal("Ctrl+` must open the Status console")
	}
	if m.membersVisible() {
		t.Fatal("Status console must hide the member column")
	}
	if got := m.transcriptWidth(); got <= widthWithMembers {
		t.Fatalf("transcript width = %d, want it to grow past %d once the member column hides", got, widthWithMembers)
	}
	if strings.Contains(ansiPattern.ReplaceAllString(m.View().Content, ""), "ONLINE - ") {
		t.Fatalf("Status transcript still renders the member column:\n%s", m.View().Content)
	}
}

// TestSlashMenuFloatsWithoutResizingColumns pins that opening the
// slash-completion menu does not resize the columns. The menu used to be a band
// stacked between the body and the composer, so it took its rows from the column
// budget and shrank the sidebar (and the transcript and member panel) while the
// user typed a slash command. It now floats over the transcript just above the
// composer, so the columns keep their height, the field stays visible, and the
// menu still renders on screen.
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

	// The menu floats above the composer block, so it never covers the field:
	// its rows end on the row before the composer starts.
	plainRows := strings.Split(ansiPattern.ReplaceAllString(content, ""), "\n")
	menuTop := closedBody - composerFieldHeight - len(m.slashLines())
	if !strings.Contains(plainRows[menuTop], "╭") {
		t.Fatalf("slash menu top row %d is not above the composer:\n%s", menuTop, plainRows[menuTop])
	}
	composerRow := plainRows[m.composerRow()]
	if strings.ContainsAny(composerRow, "╭╰") || strings.Contains(composerRow, "/join") {
		t.Fatalf("slash menu must not cover the composer row %d: %q", m.composerRow(), composerRow)
	}

	// The menu floats over the transcript column, so every sidebar row keeps its
	// left and right border cells. An indented layer drawn from column zero used
	// to blank them out and erase the sidebar on the menu's rows.
	sidebar := sidebarWidth(m.width)
	for index, line := range strings.Split(content, "\n") {
		if index >= closedBody {
			break // the footer sits below the body
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

// TestComposerBottomBorderSharesTheRailRow pins the composer's bottom edge: the
// field is a framed box, and its bottom border lands on the very row the side
// rails bottom out on. A terminal cell fills whole, so an unframed fill stopped
// half a cell below the rails' rounded border line and the composer read as
// hanging past them; a border row is a line at the same height as theirs.
func TestComposerBottomBorderSharesTheRailRow(t *testing.T) {
	m := seededModel(t)
	if !m.serverListVisible || !m.membersVisible() {
		t.Fatal("seeded layout must show both side rails")
	}
	rows := strings.Split(m.View().Content, "\n")
	last := m.bodyHeight() - 1
	if last < 0 || last >= len(rows) {
		t.Fatalf("body bottom row %d outside the %d rendered rows", last, len(rows))
	}
	row := rows[last]
	plain := []rune(ansiPattern.ReplaceAllString(row, ""))

	// The sidebar's bottom-left corner: the rails bottom out on this very row.
	if plain[0] != '╰' {
		t.Fatalf("body bottom row does not start with the sidebar's bottom corner: %q", string(plain[0]))
	}
	// The member panel's bottom-left corner, at the transcript column's end.
	memberStart := m.transcriptLeft() + m.transcriptWidth()
	if memberStart >= len(plain) || plain[memberStart] != '╰' {
		t.Fatalf("member rail bottom corner is not on row %d at cell %d: %q",
			last, memberStart, string(plain))
	}
	// The composer's own bottom border sits between them, in the same row, so
	// all three boxes bottom out together.
	if got := plain[m.composerLeft()]; got != '╰' {
		t.Fatalf("composer bottom-left corner at cell %d is %q, want ╰:\n%q",
			m.composerLeft(), string(got), string(plain))
	}
	if got := plain[m.composerLeft()+m.composerWidth()-1]; got != '╯' {
		t.Fatalf("composer bottom-right corner is %q, want ╯:\n%q", string(got), string(plain))
	}
	// The interior above the bottom border is the input row, framed by the box's
	// vertical borders and carrying no fill.
	above := ansiPattern.ReplaceAllString(rows[last-1], "")
	aboveRunes := []rune(above)
	if got, want := aboveRunes[m.composerLeft()], '│'; got != want {
		t.Fatalf("composer input row left border = %q, want %q:\n%q", string(got), string(want), above)
	}
	if strings.Contains(above, backgroundParams(m.styles.Colors.Surface)) {
		t.Fatalf("the composer's input row carries a surface fill:\n%q", above)
	}
}

// TestComposerFrameTracksFocus pins the box's border: it wears the focused panel
// border while the composer owns the keyboard, like the focused side rail, and
// drops to the plain border while an overlay does.
func TestComposerFrameTracksFocus(t *testing.T) {
	m := seededModel(t)
	if got, want := m.composerFrameStyle().GetBorderStyle(), m.styles.PanelFocused.GetBorderStyle(); got != want {
		t.Fatalf("focused composer border = %v, want the focused panel border", got)
	}
	// A blur that no overlay compensates for must drop the frame to the plain
	// border, so a modal never leaves the composer lit as if it still had keys.
	m.composer.Blur()
	if got, want := m.composerFrameStyle().GetBorderStyle(), m.styles.Panel.GetBorderStyle(); got != want {
		t.Fatalf("blurred composer border = %v, want the plain panel border", got)
	}
}

// TestFooterVersionSitsAfterTheShortcuts pins the build version's placement: it
// trails the shortcut list at the far right of the shell footer, flush to the
// window edge, and it is gone from the sidebar identity footer.
func TestFooterVersionSitsAfterTheShortcuts(t *testing.T) {
	m := seededModel(t)
	rows := strings.Split(m.render(), "\n")
	footer := ansiPattern.ReplaceAllString(rows[len(rows)-1], "")

	if !strings.HasSuffix(footer, version.Value) {
		t.Fatalf("footer must end with the version %q: %q", version.Value, footer)
	}
	if got := lipgloss.Width(rows[len(rows)-1]); got != m.width {
		t.Fatalf("footer width = %d, want the version flush to the %d-cell window", got, m.width)
	}
	// It follows the shortcut list, not the status line.
	versionAt := strings.Index(footer, version.Value)
	shortcutsAt := strings.Index(footer, "Ctrl+/")
	if shortcutsAt < 0 || versionAt <= shortcutsAt {
		t.Fatalf("the version must trail the shortcut list: %q", footer)
	}

	sidebar := ansiPattern.ReplaceAllString(m.sidebarView(sidebarWidth(m.width), m.bodyHeight()), "")
	if strings.Contains(sidebar, version.Value) {
		t.Fatalf("the sidebar identity footer still shows the version:\n%s", sidebar)
	}
}

// TestFooterVersionDropsWhenTooNarrow pins the degradation: a window too narrow
// for the status line, the shortcut list, and the version keeps the status and
// drops the version, rather than rendering it cut in half.
func TestFooterVersionDropsWhenTooNarrow(t *testing.T) {
	m := resizeModel(t, seededModel(t), 40, 24)
	rows := strings.Split(m.render(), "\n")
	footer := ansiPattern.ReplaceAllString(rows[len(rows)-1], "")
	if strings.Contains(footer, version.Value) {
		t.Fatalf("a %d-cell footer must drop the version: %q", m.width, footer)
	}
}

// TestIdentityFooterGivesTheNickItsOwnRow pins the two-row identity block: the
// initials chip and the nick own one row, and the presence word sits on the row
// below, indented under the nick, instead of being packed beside it.
func TestIdentityFooterGivesTheNickItsOwnRow(t *testing.T) {
	m := seededModel(t)
	footer := m.identityFooterLines(sidebarWidth(m.width))
	if len(footer) != 2 {
		t.Fatalf("identity footer = %d rows, want 2", len(footer))
	}
	nickRow := ansiPattern.ReplaceAllString(footer[0], "")
	statusRow := ansiPattern.ReplaceAllString(footer[1], "")
	if !strings.Contains(nickRow, "fred") {
		t.Fatalf("the nick must be on its own row: %q", nickRow)
	}
	if strings.Contains(nickRow, "available") {
		t.Fatalf("the presence word must not share the nick's row: %q", nickRow)
	}
	if !strings.Contains(statusRow, "available") {
		t.Fatalf("the status row must show the presence word: %q", statusRow)
	}
	if !strings.HasPrefix(statusRow, "  ") {
		t.Fatalf("the status row must be indented under the nick: %q", statusRow)
	}

	// The block stays the sidebar's last two rows and the column is exactly the
	// body height, so the split never overflows the grid.
	rows := strings.Split(ansiPattern.ReplaceAllString(
		m.sidebarView(sidebarWidth(m.width), m.bodyHeight()), ""), "\n")
	if len(rows) != m.bodyHeight() {
		t.Fatalf("sidebar = %d rows, want the body height %d", len(rows), m.bodyHeight())
	}
}

// TestIdentityFooterHasARuleAboveTheNick pins the divider the Qt identityFooter
// draws along its top edge: a full-width rule sits directly above the nick row
// and separates the roster from the identity block, so the nick is not flush
// with the last conversation.
func TestIdentityFooterHasARuleAboveTheNick(t *testing.T) {
	m := seededModel(t)
	const width = 30
	rows := strings.Split(ansiPattern.ReplaceAllString(
		m.sidebarView(width, m.bodyHeight()), ""), "\n")
	if len(rows) < 3 {
		t.Fatalf("sidebar = %d rows, too short for the footer block", len(rows))
	}
	rule := rows[len(rows)-3]
	nickRow := rows[len(rows)-2]
	if strings.TrimRight(rule, " ") != strings.Repeat("─", width) {
		t.Fatalf("row above the nick is not a rule: %q", rule)
	}
	if !strings.Contains(nickRow, "fred") {
		t.Fatalf("the rule must sit directly above the nick: %q", nickRow)
	}
}

// TestComposerInteriorHasNoSurfaceFill pins that the box's interior carries no
// fill: the composer is framed text over the window background, not a filled
// block. The border still marks its edges and the prompt still starts past the
// left border.
func TestComposerInteriorHasNoSurfaceFill(t *testing.T) {
	m := seededModel(t)
	row := composerRowOf(t, m)
	plain := []rune(ansiPattern.ReplaceAllString(row, ""))

	if got := len(plain); got < m.composerLeft()+m.composerWidth() {
		t.Fatalf("composer row is %d cells, want the whole box inside the transcript column", got)
	}
	// The box starts at its inset inside the transcript column, not at the
	// window edge: the sidebar occupies the cells before it. The border owns the
	// first cell and the prompt follows it.
	if got, want := plain[m.composerLeft()], '│'; got != want {
		t.Fatalf("composer box left border = %q, want %q:\n%q", string(got), string(want), string(plain))
	}
	interior := string(plain[m.composerTextLeft():])
	if !strings.HasPrefix(interior, composerPrompt) {
		t.Fatalf("composer interior must start past the border with the prompt:\n%q", interior)
	}
	if got := len([]rune(strings.TrimRight(interior, " "))); got == 0 {
		t.Fatal("composer row rendered no content")
	}
	if got, want := plain[m.composerLeft()+m.composerWidth()-1], '│'; got != want {
		t.Fatalf("composer box right border = %q, want %q:\n%q", string(got), string(want), string(plain))
	}
	// No state carries a background: not the interior padding, the prompt, the
	// typed text, nor the box frame. A fill would make the composer read as its
	// own block again instead of framed text on the window background. The check
	// reads the composer's own block rather than the whole grid row, which also
	// carries the sidebar's badge fills.
	box := m.composerView()
	fills := []struct {
		name   string
		params string
	}{
		{"surface", backgroundParams(m.styles.Colors.Surface)},
		{"raised surface", backgroundParams(m.styles.Colors.SurfaceRaised)},
		{"window", backgroundParams(m.styles.Colors.Background)},
		{"selection", backgroundParams(m.styles.Colors.Selection)},
		{"accent", backgroundParams(m.styles.Colors.Accent)},
	}
	for _, fill := range fills {
		if strings.Contains(box, fill.params) {
			t.Fatalf("composer carries the %s background %q:\n%q", fill.name, fill.params, box)
		}
	}
}

// TestComposerInputStylesAreUnfilled pins the style seam: the live composer
// input wears exactly the shared Input set, so its prompt, placeholder, text,
// and caret carry no background in any focus state. Deriving a filled variant
// for the composer was how the field's glyphs painted a block behind themselves;
// now there is no composer-only input set, so a fill cannot come back that way.
func TestComposerInputStylesAreUnfilled(t *testing.T) {
	m := seededModel(t)
	if got, want := m.composer.Styles(), m.styles.Input; !reflect.DeepEqual(got, want) {
		t.Fatalf("composer input styles diverged from the shared unfilled Input set\ngot:  %#v\nwant: %#v", got, want)
	}
}

// TestComposerCursorSitsInTheField pins that the real cursor follows the box's
// inset and border instead of staying at the window's left edge.
func TestComposerCursorSitsInTheField(t *testing.T) {
	m := seededModel(t)
	cursor := m.View().Cursor
	if cursor == nil {
		t.Fatal("focused composer must expose a terminal cursor")
	}
	if want := m.composerTextLeft() + composerPrefixWidth; cursor.Position.X < want {
		t.Fatalf("cursor column = %d, want at least composerTextLeft()+prompt = %d",
			cursor.Position.X, want)
	}
	if limit := m.composerTextLeft() + m.composerInteriorWidth(); cursor.Position.X >= limit {
		t.Fatalf("cursor column = %d, past the interior end %d", cursor.Position.X, limit)
	}
	if cursor.Position.Y != m.composerRow() {
		t.Fatalf("cursor row = %d, want composerRow() %d", cursor.Position.Y, m.composerRow())
	}
}

// framedInnerWidth is the content width framedColumn passes to a side column.
func framedInnerWidth(m *Model, outer int) int {
	frameX, _ := m.styles.Panel.GetFrameSize()
	inner := outer - frameX
	if inner < 1 {
		return outer
	}
	return inner
}

// TestSidebarNetworkHeaderEllipsizes pins the roster header at the widths where
// the seeded network name does not fit: the row ends with … and stays the
// column width, at an 80-column window and a 50-column window.
func TestSidebarNetworkHeaderEllipsizes(t *testing.T) {
	for _, window := range []int{80, 50} {
		m := resizeModel(t, seededModel(t), window, 30)
		inner := framedInnerWidth(m, sidebarWidth(window))
		rendered := m.sidebarView(inner, m.bodyHeight())
		rows := strings.Split(rendered, "\n")
		for _, row := range rows {
			if got := lipgloss.Width(row); got != inner {
				t.Fatalf("window %d sidebar row width %d, want %d:\n%s", window, got, inner, rendered)
			}
		}
		header := plainLine(rows[0])
		name := m.sidebarNetworkDisplayName("omarchy")
		full := "▾ " + name
		if lipgloss.Width(full) <= inner {
			t.Fatalf("window %d header %q fits %d cells; the proof needs an overflow", window, full, inner)
		}
		if !strings.HasSuffix(header, "…") {
			t.Fatalf("window %d header %q does not end with an ellipsis", window, header)
		}
		if strings.Contains(header, name) {
			t.Fatalf("window %d header kept the full name %q: %q", window, name, header)
		}
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
// network rules: one above each network after the first, never a leading one.
// The identity footer adds one more rule above the nick, so the net count is N.
func TestSidebarSeparatorCount(t *testing.T) {
	m := seededModel(t)
	const width = 30
	rows := strings.Split(m.sidebarView(width, m.bodyHeight()), "\n")
	plain := make([]string, len(rows))
	rule := strings.Repeat("─", width)
	rules := 0
	for index, row := range rows {
		plain[index] = strings.TrimRight(ansiPattern.ReplaceAllString(row, ""), " ")
		if plain[index] == rule {
			rules++
		}
	}
	// The footer rule is the last rule in the column, directly above the nick.
	if len(plain) < 3 || plain[len(plain)-3] != rule {
		t.Fatalf("no rule above the identity nick:\n%s", strings.Join(plain, "\n"))
	}
	if want := len(m.sidebarNetworkIDs()) - 1; rules-1 != want {
		t.Fatalf("network separators = %d, want %d", rules-1, want)
	}
	if len(m.sidebarNetworkIDs()) < 2 {
		t.Fatal("seeded roster must have more than one network to separate")
	}
}

// TestSidebarDividerWidth pins the rule renderer: a rule reaches the column
// width, and a zero width yields no line so a tiny column never draws a stray
// glyph.
func TestSidebarDividerWidth(t *testing.T) {
	m := seededModel(t)
	if got := ansiPattern.ReplaceAllString(m.sidebarDivider(20), ""); got != strings.Repeat("─", 20) {
		t.Fatalf("separator = %q, want a 20-cell rule", got)
	}
	if got := m.sidebarDivider(0); got != "" {
		t.Fatalf("zero-width separator = %q, want empty", got)
	}
}
