package ui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
)

// TestPreferencesTabSharesControllerPrefs proves the Connect sheet's
// Preferences tab reads and writes the controller's single preference source,
// so /pref and the sheet can never disagree. It mirrors the Phase 11 collapse
// of the connection's duplicate preference bools.
func TestPreferencesTabSharesControllerPrefs(t *testing.T) {
	ctrl := controller.New()
	conn := connection.New(ctrl, nil)
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)

	// The first-run sheet is already open on the Connection tab. Ctrl+Tab
	// switches to Preferences, then Tab moves focus from the network rail to
	// the first toggle.
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModCtrl})
	if !strings.Contains(m.View().Content, "Reopen direct messages on startup") {
		t.Fatalf("Preferences tab must render the toggles:\n%s", m.View().Content)
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})

	before := ctrl.ReopenDirects()
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeySpace})
	if got := ctrl.ReopenDirects(); got == before {
		t.Fatalf("Space on the first Preferences toggle must flip the controller pref")
	}
	if conn.ReopenDirects() != ctrl.ReopenDirects() {
		t.Fatalf("the sheet and the controller must share one preference source")
	}
}

// TestPreferencesTogglePersists drives the sheet toggle on a non-ephemeral
// controller and asserts the shared INI gets the Qt-compatible preferences
// line, so the setting survives a restart.
func TestPreferencesTogglePersists(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)

	ctrl := controller.New()
	ctrl.SetEphemeral(false)
	ctrl.LoadStoredPreferences()
	conn := connection.New(ctrl, nil)
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModCtrl})
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeySpace})
	if ctrl.ReopenDirects() {
		t.Fatal("the first toggle must turn reopen-directs off")
	}

	path := filepath.Join(dir, "omairc", "omairc.conf")
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("toggling a preference must write the config: %v", err)
	}
	if !strings.Contains(string(raw), "reopenDirectMessages=false") {
		t.Fatalf("config must store the Qt-compatible key:\n%s", raw)
	}

	// A fresh controller reads the same value back.
	reloaded := controller.New()
	reloaded.SetEphemeral(false)
	reloaded.LoadStoredPreferences()
	if reloaded.ReopenDirects() {
		t.Fatal("a restart must load reopenDirectMessages=false")
	}

	// The demo path stays ephemeral and writes nothing.
	demoDir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", demoDir)
	ephemeral := controller.New()
	ephemeral.SetReopenDirects(false)
	if _, err := os.Stat(filepath.Join(demoDir, "omairc", "omairc.conf")); !os.IsNotExist(err) {
		t.Fatalf("an ephemeral controller must not touch disk, err=%v", err)
	}
}
