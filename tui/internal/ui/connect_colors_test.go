package ui

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
)

// TestPreferencesColorsRowRendersAndOrders pins the Connect Preferences
// "Show mIRC colors" row: label, catalog position after unread / before
// palette, and chip state mirroring the controller preference.
func TestPreferencesColorsRowRendersAndOrders(t *testing.T) {
	ctrl := controller.New()
	conn := connection.New(ctrl, nil)
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModCtrl})
	view := ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(view, "Show mIRC colors") {
		t.Fatalf("Preferences tab missing Show mIRC colors:\n%s", view)
	}
	unreadAt := strings.Index(view, "Open conversations at unread")
	colorsAt := strings.Index(view, "Show mIRC colors")
	paletteAt := strings.Index(view, "Color palette")
	if unreadAt < 0 || colorsAt < 0 || paletteAt < 0 {
		t.Fatalf("Preferences catalog missing unread/colors/palette:\n%s", view)
	}
	if !(unreadAt < colorsAt && colorsAt < paletteAt) {
		t.Fatalf("colors must sit after unread and before palette; unread=%d colors=%d palette=%d",
			unreadAt, colorsAt, paletteAt)
	}
	if !conn.ShowIrcColors() || !ctrl.ShowIrcColors() {
		t.Fatal("ShowIrcColors must default on")
	}
	assertColorsChip(t, m, true)

	if got := prefFieldLabels[prefColors]; got != "Show mIRC colors" {
		t.Fatalf("prefColors label = %q, want Show mIRC colors", got)
	}
	if prefColors != prefUnread+1 || prefPalette != prefColors+1 {
		t.Fatalf("pref enum order unread=%d colors=%d palette=%d", prefUnread, prefColors, prefPalette)
	}
}

// TestPreferencesColorsSpaceTogglesWithoutApply drives Space on the colors
// row and checks the chip and controller flip immediately; Apply is not
// required for preference toggles.
func TestPreferencesColorsSpaceTogglesWithoutApply(t *testing.T) {
	ctrl := controller.New()
	conn := connection.New(ctrl, nil)
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModCtrl})
	focusPreferenceColors(t, m)
	assertColorsChip(t, m, true)

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeySpace})
	if ctrl.ShowIrcColors() || conn.ShowIrcColors() {
		t.Fatal("Space on Show mIRC colors must flip the shared preference off")
	}
	assertColorsChip(t, m, false)

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeySpace})
	if !ctrl.ShowIrcColors() || !conn.ShowIrcColors() {
		t.Fatal("Space again must flip ShowIrcColors back on")
	}
	assertColorsChip(t, m, true)
}

// TestPreferencesColorsKeyboardReachable walks Tab/Enter onto the colors row
// at a short terminal height so the focus walk keeps the row reachable.
func TestPreferencesColorsKeyboardReachable(t *testing.T) {
	ctrl := controller.New()
	conn := connection.New(ctrl, nil)
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 90, Height: 14})
	m = updated.(*Model)

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModCtrl})
	// Network rail -> first pref (reopen). Enter walks field to field.
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
	for step := 0; step < int(prefFieldCount); step++ {
		stop, ok := m.focusedStop()
		if ok && stop.kind == stopField && prefField(stop.index) == prefColors {
			view := ansiPattern.ReplaceAllString(m.View().Content, "")
			if !strings.Contains(view, "Show mIRC colors") {
				t.Fatalf("focused colors row must stay visible:\n%s", view)
			}
			return
		}
		m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	}
	t.Fatal("Enter/Tab walk never reached Show mIRC colors")
}

// TestPreferencesColorsSlashPrefSyncsRow proves /pref colors updates the
// Connect Preferences chip without touching Apply, so slash and sheet share
// one controller preference.
func TestPreferencesColorsSlashPrefSyncsRow(t *testing.T) {
	ctrl := controller.New()
	conn := connection.New(ctrl, nil)
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModCtrl})
	assertColorsChip(t, m, true)

	if !ctrl.ConsoleSubmit("/pref colors off") {
		t.Fatal("ConsoleSubmit(/pref colors off) must be accepted")
	}
	if ctrl.ShowIrcColors() || conn.ShowIrcColors() {
		t.Fatal("/pref colors off must clear ShowIrcColors on controller and connection")
	}
	assertColorsChip(t, m, false)

	if !ctrl.ConsoleSubmit("/pref colors on") {
		t.Fatal("ConsoleSubmit(/pref colors on) must be accepted")
	}
	if !ctrl.ShowIrcColors() || !conn.ShowIrcColors() {
		t.Fatal("/pref colors on must restore ShowIrcColors")
	}
	assertColorsChip(t, m, true)
}

// TestPreferencesColorsEnterWalksPastRow pins Enter on the colors toggle walks
// to the next preference instead of applying the network profile.
func TestPreferencesColorsEnterWalksPastRow(t *testing.T) {
	ctrl := controller.New()
	conn := connection.New(ctrl, nil)
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModCtrl})
	focusPreferenceColors(t, m)
	before := ctrl.ShowIrcColors()
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if ctrl.ShowIrcColors() != before {
		t.Fatal("Enter on Show mIRC colors must not flip the preference")
	}
	stop, ok := m.focusedStop()
	if !ok || stop.kind != stopField || prefField(stop.index) != prefPalette {
		t.Fatalf("Enter must walk from colors to palette, got stop=%v ok=%v", stop, ok)
	}
}

func focusPreferenceColors(t *testing.T, m *Model) {
	t.Helper()
	for index, stop := range m.connectStops() {
		if stop.kind == stopField && prefField(stop.index) == prefColors {
			m.sheet.focus = index
			return
		}
	}
	t.Fatal("Preferences Show mIRC colors stop not found")
}

func assertColorsChip(t *testing.T, m *Model, on bool) {
	t.Helper()
	view := ansiPattern.ReplaceAllString(m.View().Content, "")
	chip := m.connectChip(on)
	plainChip := ansiPattern.ReplaceAllString(chip, "")
	if !strings.Contains(view, "Show mIRC colors") {
		t.Fatalf("Preferences view missing Show mIRC colors:\n%s", view)
	}
	if m.conn.ShowIrcColors() != on {
		t.Fatalf("ShowIrcColors = %v, want %v", m.conn.ShowIrcColors(), on)
	}
	// Prefer matching the chip next to the label when both on/off chips appear.
	labelAt := strings.Index(view, "Show mIRC colors")
	if labelAt < 0 {
		t.Fatal("missing label")
	}
	windowStart := labelAt - 12
	if windowStart < 0 {
		windowStart = 0
	}
	window := view[windowStart:labelAt]
	if !strings.Contains(window, strings.TrimSpace(plainChip)) {
		t.Fatalf("colors row chip window %q missing %q (on=%v):\n%s",
			window, plainChip, on, view)
	}
}