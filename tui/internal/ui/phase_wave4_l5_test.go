package ui

// This file covers workstream L5: the /list table, the Ctrl+/ keycap sheet, and
// the inline slash-completion menu. It reuses the demo-seeded harness in
// phase5_test.go and the ansiPattern helper in composer_test.go.

import (
	"fmt"
	"image/color"
	"regexp"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// backgroundParams is the 24-bit SGR fragment lipgloss emits for a background
// fill, e.g. "48;2;24;106;154". It appears inside a combined SGR sequence, so a
// test asserts on the fragment rather than the whole escape.
func backgroundParams(c color.Color) string {
	rgb := color.RGBAModel.Convert(c).(color.RGBA)
	return fmt.Sprintf("48;2;%d;%d;%d", rgb.R, rgb.G, rgb.B)
}

// openListOverlay drives the seeded demo through /list and returns the model
// with the overlay open.
func openListOverlay(t *testing.T) *Model {
	t.Helper()
	m := seededModel(t)
	m.composer.SetValue("/list")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if !m.list.open {
		t.Fatal("/list must open the channel-list overlay")
	}
	return m
}

// TestListOverlayRendersTable pins the /list restyle: a lipgloss table with the
// CHANNEL / USERS / TOPIC headers, right-aligned user counts, zebra fills, and
// a selection fill on the highlighted row, all inside the shared card frame.
func TestListOverlayRendersTable(t *testing.T) {
	m := openListOverlay(t)
	card := m.channelListCard(m.width)
	if strings.TrimSpace(ansiPattern.ReplaceAllString(card, "")) == "" {
		t.Fatal("list card is empty")
	}

	plain := ansiPattern.ReplaceAllString(card, "")
	for _, header := range []string{"CHANNEL", "USERS", "TOPIC"} {
		if !strings.Contains(plain, header) {
			t.Fatalf("list card missing %q header:\n%s", header, plain)
		}
	}

	// The first demo row is #linux with 42 users and its topic.
	rowLine := ""
	for _, line := range strings.Split(plain, "\n") {
		if strings.Contains(line, "#linux") {
			rowLine = line
			break
		}
	}
	if rowLine == "" {
		t.Fatalf("list card missing the #linux row:\n%s", plain)
	}
	if !regexp.MustCompile(`#linux\s+42\s+Kernel discussion`).MatchString(rowLine) {
		t.Fatalf("user count is not right-aligned with a gutter before the topic:\n%q", rowLine)
	}

	// The striping and the highlight are real background fills.
	if !strings.Contains(card, backgroundParams(m.styles.Colors.Surface)) {
		t.Fatalf("list card has no zebra row fill:\n%q", card)
	}
	if !strings.Contains(card, backgroundParams(m.styles.Colors.Selection)) {
		t.Fatalf("list card has no selected-row fill:\n%q", card)
	}

	// The filter input still lives in the body and keeps its placeholder.
	if !strings.Contains(plain, "Filter channels") {
		t.Fatalf("list card lost the filter input:\n%s", plain)
	}
}

// TestListOverlayKeepsRowCap pins the controller's 200-row render cap: a larger
// stream renders exactly 200 data rows plus the header and rule.
func TestListOverlayKeepsRowCap(t *testing.T) {
	m := openListOverlay(t)
	rows := make([]controller.ChannelListRow, channelListRowCap+25)
	for index := range rows {
		rows[index] = controller.ChannelListRow{
			Channel: fmt.Sprintf("#chan%03d", index),
			Users:   index,
			Topic:   "topic",
		}
	}
	lines := m.channelListTableLines(rows, 100)
	// header + header rule + capped data rows.
	if want := channelListRowCap + 2; len(lines) != want {
		t.Fatalf("rendered %d table lines, want %d (200-row cap)", len(lines), want)
	}
}

// TestShortcutsSheetRendersKeycaps pins the sheet restyle: every group header,
// chord, and action from the source table is still rendered, with the chord in
// a keycap chip (a raised-surface fill).
func TestShortcutsSheetRendersKeycaps(t *testing.T) {
	m := seededModel(t)
	m.openShortcuts()
	card := m.shortcutsCard(m.width)
	inner := m.shortcutsInnerWidth()
	plain := shortcutsPlainFold(strings.Join(m.shortcutsCardBody(inner), "\n"))

	for _, group := range shortcutGroups {
		if !strings.Contains(plain, group.title) {
			t.Fatalf("shortcuts sheet missing group %q:\n%s", group.title, plain)
		}
		for _, row := range group.rows {
			keys := displayShortcutKeys(row.keys)
			if !strings.Contains(plain, keys) {
				t.Fatalf("shortcuts sheet missing keys %q:\n%s", keys, plain)
			}
			if !strings.Contains(plain, row.action) {
				t.Fatalf("shortcuts sheet missing action %q:\n%s", row.action, plain)
			}
		}
	}
	if !strings.Contains(card, backgroundParams(m.styles.Colors.SurfaceRaised)) {
		t.Fatalf("shortcuts sheet has no keycap chip fill:\n%q", card)
	}
}

// TestSlashMenuFloatsWithSelectedFill pins the inline slash menu's bordered,
// filled look: the framed menu is one row per hit plus a top and bottom border,
// the highlighted row carries the selection fill, and every label and usage is
// still present.
func TestSlashMenuFloatsWithSelectedFill(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("/j")
	m.syncSlash()
	if !m.slash.open() {
		t.Fatal("slash list must open for /j")
	}
	lines := m.slashLines()
	if want := len(m.slash.probe.Hits) + slashMenuBorderRows; len(lines) != want {
		t.Fatalf("slash menu has %d lines, want %d (hits + border)", len(lines), want)
	}
	menu := strings.Join(lines, "\n")
	plain := ansiPattern.ReplaceAllString(menu, "")
	trimmed := strings.TrimLeft(plain, " ")
	if !strings.HasPrefix(trimmed, "╭") || !strings.HasSuffix(strings.TrimRight(plain, " "), "╯") {
		t.Fatalf("slash menu is not a rounded border box:\n%s", plain)
	}
	for _, hit := range m.slash.probe.Hits {
		if !strings.Contains(plain, hit.Label) {
			t.Fatalf("slash menu missing label %q:\n%s", hit.Label, plain)
		}
		if hit.Usage != "" && !strings.Contains(plain, hit.Usage) {
			t.Fatalf("slash menu missing usage %q:\n%s", hit.Usage, plain)
		}
	}
	if !strings.Contains(menu, backgroundParams(m.styles.Colors.Selection)) {
		t.Fatalf("slash menu has no selected-row fill:\n%q", menu)
	}
}

// TestSlashMenuDropsFrameWhenCramped pins the tiny-window degradation: when the
// frame would push the composer block off the grid, the menu renders the bare
// filled rows. It also pins the row cap, so a window this short can never
// scroll the top of the frame away.
func TestSlashMenuDropsFrameWhenCramped(t *testing.T) {
	m := resizeModel(t, seededModel(t), defaultWidth, 8)
	m.slash.probe = controller.SlashProbe{Open: true, Needle: "/", Hits: []controller.SlashHit{
		{Label: "/a", Usage: "one"},
		{Label: "/b", Usage: "two"},
		{Label: "/c", Usage: "three"},
	}}
	lines := m.slashLines()
	if want := len(m.slash.probe.Hits); len(lines) != want {
		t.Fatalf("cramped slash menu has %d lines, want %d without borders", len(lines), want)
	}
	plain := ansiPattern.ReplaceAllString(strings.Join(lines, "\n"), "")
	if strings.ContainsAny(plain, "╭╰") {
		t.Fatalf("cramped slash menu must drop the frame:\n%s", plain)
	}
	// The whole frame still fits the window, so nothing scrolls away.
	if got, want := len(strings.Split(m.render(), "\n")), m.height; got != want {
		t.Fatalf("cramped frame = %d rows, want the %d-row window", got, want)
	}
}

// TestSlashMenuCapsRowsToTheGrid pins the cap itself: a window too short for
// every hit shows only what fits above the composer block and one body row,
// instead of overflowing the frame and scrolling its top off-screen.
func TestSlashMenuCapsRowsToTheGrid(t *testing.T) {
	m := resizeModel(t, seededModel(t), defaultWidth, minHeight)
	hits := make([]controller.SlashHit, 0, 12)
	for _, label := range []string{"/a", "/b", "/c", "/d", "/e", "/f"} {
		hits = append(hits, controller.SlashHit{Label: label, Usage: "usage"})
	}
	m.slash.probe = controller.SlashProbe{Open: true, Needle: "/", Hits: hits}

	lines := m.slashLines()
	if room := m.height - composerFieldHeight - 1; len(lines) != room {
		t.Fatalf("capped slash menu has %d lines, want the %d rows that fit", len(lines), room)
	}
	if len(lines) >= len(hits) {
		t.Fatalf("the %d-hit menu was not capped at %d rows", len(hits), len(lines))
	}
	// The probe is untouched: the cap is display-only, so completion still sees
	// every hit.
	if got := len(m.slash.probe.Hits); got != len(hits) {
		t.Fatalf("probe hits = %d, want the %d real hits", got, len(hits))
	}
	if got, want := len(strings.Split(m.render(), "\n")), m.height; got != want {
		t.Fatalf("capped frame = %d rows, want the %d-row window", got, want)
	}
}
