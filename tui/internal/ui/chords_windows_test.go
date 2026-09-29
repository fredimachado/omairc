//go:build windows

package ui

import (
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/session"
)

func TestNormalizeChordKeyWindowsAliases(t *testing.T) {
	cases := map[string]string{
		"ctrl+_":              "ctrl+/",
		"ctrl+]":              "ctrl+,",
		"ctrl+alt+left":       "alt+left",
		"ctrl+alt+right":      "alt+right",
		"ctrl+alt+up":         "alt+up",
		"ctrl+alt+down":       "alt+down",
		"ctrl+shift+left":     "alt+shift+left",
		"ctrl+shift+right":    "alt+shift+right",
		"ctrl+alt+shift+up":   "alt+shift+up",
		"ctrl+alt+shift+down": "alt+shift+down",
		"ctrl+pgdown":         "ctrl+tab",
		"ctrl+k":              "ctrl+k",
	}
	for in, want := range cases {
		if got := normalizeChordKey(in); got != want {
			t.Fatalf("normalizeChordKey(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestDisplayShortcutKeysWindowsAliases(t *testing.T) {
	if got := displayShortcutKeys("Ctrl+,"); got != "Ctrl+, / Ctrl+]" {
		t.Fatalf("displayShortcutKeys(Ctrl+,) = %q", got)
	}
	if got := displayShortcutKeys("Ctrl+K"); got != "Ctrl+K" {
		t.Fatalf("displayShortcutKeys(Ctrl+K) = %q", got)
	}
}

func TestWindowsCtrlJOpensNickJumpOutsideConnect(t *testing.T) {
	m := seededModel(t)
	m.closeConnect()
	m = press(t, m, tea.KeyPressMsg{Code: 'j', Mod: tea.ModCtrl})
	if !m.nickVisible() {
		t.Fatal("ctrl+j must open nick jump outside Connect on Windows (ConPTY Ctrl+Shift+K alias)")
	}
}

func TestNickJumpChordKeyWindowsAliases(t *testing.T) {
	cases := map[string]string{
		"ctrl+j":       "ctrl+shift+k",
		"ctrl+shift+k": "ctrl+shift+k",
		"ctrl+k":       "ctrl+k",
	}
	for in, want := range cases {
		if got := nickJumpChordKey(in); got != want {
			t.Fatalf("nickJumpChordKey(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestWindowsLegacyCtrlSlashOpensShortcuts(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: '_', Mod: tea.ModCtrl})
	if !m.shortcutsOpen {
		t.Fatal("legacy ctrl+/ (ctrl+_) must open the shortcuts sheet on Windows")
	}
}

func TestWindowsCtrlBracketOpensConnect(t *testing.T) {
	ctrl := controller.New()
	conn := connection.New(ctrl, func() session.Transport { return session.NewLoopbackTransport() })
	conn.SetStoredProfiles([]connection.NetworkProfile{
		storedProfile("net-a", "irc.a.example"),
		storedProfile("net-b", "irc.b.example"),
	})
	conn.Select("net-a")
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)
	m.closeConnect()

	// Walk to net-b's header, then reopen Connect with the WT Ctrl+] alias.
	// It must open on the walked-to network, not the selected one (net-a).
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyRight, Mod: tea.ModAlt})
	if got := m.sidebarNetworkFocusID; got != "net-b" {
		t.Fatalf("header focus = %q, want net-b", got)
	}
	m = press(t, m, tea.KeyPressMsg{Code: ']', Mod: tea.ModCtrl})
	if !m.connectVisible() {
		t.Fatal("ctrl+] must open Connect on Windows")
	}
	if got := conn.SelectedNetworkID(); got != "net-b" {
		t.Fatalf("sheet selected network = %q, want net-b", got)
	}
}

func TestWindowsChordAliasesDriveNavigation(t *testing.T) {
	m := seededModel(t)
	m.closeConnect()
	if handled, _ := m.dispatchChord(normalizeChordKey("ctrl+alt+left"), tea.KeyPressMsg{}); !handled {
		t.Fatal("ctrl+alt+left must walk networks on Windows")
	}
	if m.sidebarNetworkFocusID != "oftc" {
		t.Fatalf("ctrl+alt+left focus = %q, want oftc", m.sidebarNetworkFocusID)
	}
}
