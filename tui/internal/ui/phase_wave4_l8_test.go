package ui

// This file covers the L8 theme-watcher wiring: SetThemeWatcher adopts the
// watcher's current palette immediately and the read loop is armed on the next
// background-work pass, so a live Omarchy swap reaches the shell through a
// tea.Cmd instead of a synchronous callback.

import (
	"reflect"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/theme"
)

// TestSetThemeWatcherAdoptsCurrentAndArmsRead pins the L8 contract. The manual
// SetTheme first makes the adoption observable even when no Omarchy theme is
// installed (both palettes would otherwise be Fallback): SetThemeWatcher must
// replace the manual palette with watcher.Current().
func TestSetThemeWatcherAdoptsCurrentAndArmsRead(t *testing.T) {
	m := seededModel(t)
	m.SetTheme(theme.Derive(theme.Spec{Mode: theme.ModeLight}))

	watcher := theme.NewWatcher(nil, 0)
	defer watcher.Close()

	if m.themeArmed {
		t.Fatal("themeArmed must be false before a watcher is installed")
	}
	m.SetThemeWatcher(watcher)
	if !reflect.DeepEqual(m.styles.Colors, watcher.Current()) {
		t.Fatal("SetThemeWatcher must adopt watcher.Current()")
	}

	// The blocking Changes() read is armed by the next background-work pass,
	// which is what WindowSizeMsg triggers in the running program.
	cmd := m.startBackgroundWork()
	if !m.themeArmed {
		t.Fatal("startBackgroundWork must arm the theme read once a watcher is set")
	}
	if cmd == nil {
		t.Fatal("arming the theme read must return a command")
	}
}
