package ui

// This file covers the Wave 4 integration that no single workstream could
// exercise: re-applying the live palette to an already-open overlay input, and
// the composer cursor row under L5's taller framed slash menu. It reuses the
// demo-seeded harness in phase5_test.go, the connectSheetModel helper in
// phase_wave4_l7_test.go, and the ansiPattern helper in composer_test.go.

import (
	"reflect"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/theme"
)

// TestThemeChangeRestylesOpenOverlayInputs pins STEP 2(a): a live theme change
// re-applies the shared input style set to every open overlay filter and the
// Connect sheet, not just the composer. The filter overlays build their input
// when they open, so without the re-apply an already-open sheet would keep the
// previous palette.
func TestThemeChangeRestylesOpenOverlayInputs(t *testing.T) {
	light := theme.Derive(theme.Spec{Mode: theme.ModeLight})
	if theme.Hex(light.Background) == theme.Hex(defaultStyles().Colors.Background) {
		t.Fatal("light palette unexpectedly matches the fallback background")
	}

	cases := []struct {
		name  string
		open  func(m *Model)
		check func(m *Model) bool
	}{
		{"jump", func(m *Model) { m.openJump() }, func(m *Model) bool {
			return reflect.DeepEqual(m.jump.input.Styles(), m.styles.Input)
		}},
		{"nick", func(m *Model) { m.openNickJump() }, func(m *Model) bool {
			return reflect.DeepEqual(m.nick.input.Styles(), m.styles.Input)
		}},
		{"link", func(m *Model) { m.toggleLink() }, func(m *Model) bool {
			return reflect.DeepEqual(m.link.input.Styles(), m.styles.Input)
		}},
		{"list", func(m *Model) { m.openChannelList() }, func(m *Model) bool {
			return reflect.DeepEqual(m.list.input.Styles(), m.styles.Input)
		}},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			m := seededModel(t)
			c.open(m)
			updated, _ := m.Update(ThemeChangedMsg{Colors: light})
			m = updated.(*Model)
			if !c.check(m) {
				t.Fatalf("open %s input did not adopt the live theme palette", c.name)
			}
		})
	}

	// The Connect sheet keeps one live input mirrored to whichever field is
	// focused, so focus a field before the theme change.
	t.Run("connect", func(t *testing.T) {
		m := connectSheetModel(t)
		m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
		if !m.sheet.input.Focused() {
			t.Fatal("Tab must focus the sheet's Name field")
		}
		updated, _ := m.Update(ThemeChangedMsg{Colors: light})
		m = updated.(*Model)
		if got, want := m.sheet.input.Styles(), m.styles.Input; !reflect.DeepEqual(got, want) {
			t.Fatal("open Connect sheet input did not adopt the live theme palette")
		}
	})
}

// TestFramedSlashMenuKeepsComposerCursorRow pins STEP 2(c): L5's taller framed
// slash menu still leaves the composer's real cursor on its frame row. At the
// seeded size the frame is present, the footer still fits, and the cursor sits
// at composerRow() on the row above the footer.
func TestFramedSlashMenuKeepsComposerCursorRow(t *testing.T) {
	m := seededModel(t)
	m.slash.probe = controller.SlashProbe{Open: true, Needle: "/", Hits: []controller.SlashHit{
		{Label: "/a", Usage: "one"},
		{Label: "/b", Usage: "two"},
	}}
	lines := m.slashLines()
	if want := len(m.slash.probe.Hits) + slashMenuBorderRows; len(lines) != want {
		t.Fatalf("slash menu has %d lines, want %d with the frame", len(lines), want)
	}
	plain := ansiPattern.ReplaceAllString(strings.Join(lines, "\n"), "")
	if !strings.Contains(plain, "╭") || !strings.Contains(plain, "╯") {
		t.Fatalf("slash menu must be framed at the seeded size:\n%s", plain)
	}
	if !m.footerVisible() {
		t.Fatal("footer must remain visible at the seeded size")
	}
	cursor := m.View().Cursor
	if cursor == nil {
		t.Fatal("focused composer must expose a terminal cursor")
	}
	if want := m.composerRow(); cursor.Position.Y != want {
		t.Fatalf("cursor row = %d, want composerRow() %d", cursor.Position.Y, want)
	}
	if want := m.height - 1 - footerHeight; cursor.Position.Y != want {
		t.Fatalf("cursor row = %d, want %d above the footer", cursor.Position.Y, want)
	}
}
