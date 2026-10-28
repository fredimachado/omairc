package ui

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/theme"
)

// TestPreferencesPaletteSpaceCycles drives Space on the Color palette row and
// checks the chip, controller id, and NickPalette after NotifyMsg. The
// controller stays ephemeral so nothing touches omairc.conf.
func TestPreferencesPaletteSpaceCycles(t *testing.T) {
	ctrl := controller.New()
	conn := connection.New(ctrl, nil)
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModCtrl})
	focusPreferencePalette(t, m)

	assertPaletteChip(t, m, "classic")
	if ctrl.ColorPalette() != irc.ColorPaletteClassic {
		t.Fatalf("ColorPalette = %v, want classic", ctrl.ColorPalette())
	}

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeySpace})
	assertPaletteChip(t, m, "vivid")
	if ctrl.ColorPalette() != irc.ColorPaletteVivid {
		t.Fatalf("ColorPalette = %v, want vivid", ctrl.ColorPalette())
	}
	updated, _ = m.Update(NotifyMsg{})
	m = updated.(*Model)
	if got := theme.Hex(m.styles.NickPalette[0]); got == "#7aa2f7" {
		t.Fatalf("NickPalette[0] still classic #7aa2f7 after vivid")
	}
	wantVivid := theme.Hex(theme.PaletteColors(theme.ColorPaletteVivid, true, theme.FixedAccent())[0])
	if got := theme.Hex(m.styles.NickPalette[0]); got != wantVivid {
		t.Fatalf("NickPalette[0] = %s, want %s", got, wantVivid)
	}

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeySpace})
	assertPaletteChip(t, m, "classic")
	if ctrl.ColorPalette() != irc.ColorPaletteClassic {
		t.Fatalf("ColorPalette = %v, want classic", ctrl.ColorPalette())
	}
	updated, _ = m.Update(NotifyMsg{})
	m = updated.(*Model)
	if got := theme.Hex(m.styles.NickPalette[0]); got != "#7aa2f7" {
		t.Fatalf("NickPalette[0] = %s, want #7aa2f7", got)
	}
}

func focusPreferencePalette(t *testing.T, m *Model) {
	t.Helper()
	for index, stop := range m.connectStops() {
		if stop.kind == stopField && prefField(stop.index) == prefPalette {
			m.sheet.focus = index
			return
		}
	}
	t.Fatal("Preferences Color palette stop not found")
}

func assertPaletteChip(t *testing.T, m *Model, token string) {
	t.Helper()
	view := ansiPattern.ReplaceAllString(m.View().Content, "")
	chip := m.connectTokenChip(token)
	plainChip := ansiPattern.ReplaceAllString(chip, "")
	if !strings.Contains(view, plainChip) && !strings.Contains(view, token) {
		t.Fatalf("Preferences view missing palette chip %q:\n%s", token, view)
	}
	if m.conn.ColorPaletteToken() != token {
		t.Fatalf("ColorPaletteToken = %q, want %q", m.conn.ColorPaletteToken(), token)
	}
}
