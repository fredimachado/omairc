package ui

// This file covers the Ctrl+Shift+/ About sheet: the chord opens it from the
// composer and over the Connect sheet, Escape/Enter/Space dismiss it, the chord
// does not toggle it, and the content carries the name, version, source URL,
// and copyright from src/qml/AboutSheet.qml.

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/version"
)

func aboutChord() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: '/', Mod: tea.ModCtrl | tea.ModShift}
}

func TestAboutOpensWithChordAndClosesWithEscape(t *testing.T) {
	m := seededModel(t)

	m = press(t, m, aboutChord())
	if !m.aboutOpen {
		t.Fatal("Ctrl+Shift+/ must open the About sheet")
	}
	content := m.View().Content
	for _, wanted := range []string{
		"About Omairc",
		version.Value,
		"This project is open-source.",
		"View the source on GitHub",
		aboutRepoURL,
		aboutCopyright,
		"OK",
	} {
		if !strings.Contains(content, wanted) {
			t.Fatalf("About sheet missing %q:\n%s", wanted, content)
		}
	}

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.aboutOpen {
		t.Fatal("Escape must close the About sheet")
	}
	if got := m.View().WindowTitle; got != "#omarchy · irc.example · fred - Omairc" {
		t.Fatalf("title after closing About = %q, want the conversation", got)
	}
}

func TestAboutClosesOnEnterAndSpace(t *testing.T) {
	for _, key := range []tea.KeyPressMsg{
		{Code: tea.KeyEnter},
		{Code: tea.KeySpace},
	} {
		m := seededModel(t)
		m = press(t, m, aboutChord())
		if !m.aboutOpen {
			t.Fatal("ctrl+shift+/ must open About before the dismiss check")
		}
		m = press(t, m, key)
		if m.aboutOpen {
			t.Fatalf("%v must close the About sheet", key)
		}
	}
}

// The Qt Ctrl+Shift+/ shortcut is disabled while the sheet is visible, so the
// chord must not toggle it closed.
func TestAboutChordDoesNotToggle(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, aboutChord())
	m = press(t, m, aboutChord())
	if !m.aboutOpen {
		t.Fatal("a second Ctrl+Shift+/ must leave About open, matching the Qt shortcut")
	}
}

func TestAboutOpensOverConnectAndEscapeReturnsToIt(t *testing.T) {
	conn := connection.New(controller.New(), nil)
	m, _ := sizedModel(t, conn)
	if !m.connectVisible() {
		t.Fatal("first run must show the Connect sheet")
	}

	m = press(t, m, aboutChord())
	if !m.aboutOpen {
		t.Fatal("Ctrl+Shift+/ must open About over the Connect sheet")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.aboutOpen {
		t.Fatal("Escape must close About first")
	}
	if !m.connectVisible() {
		t.Fatal("closing About must leave the first-run Connect sheet open")
	}
}

func TestAboutBlocksNavigationChords(t *testing.T) {
	m := seededModel(t)
	before := m.ctrl.SelectedTarget()
	m = press(t, m, aboutChord())

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyDown, Mod: tea.ModAlt})
	if got := m.ctrl.SelectedTarget(); got != before {
		t.Fatalf("Alt+Down moved selection to %q while About was open", got)
	}
	m = press(t, m, ctrlKey('/'))
	if m.shortcutsOpen {
		t.Fatal("Ctrl+/ must not open the shortcuts sheet over About")
	}
	if !m.aboutOpen {
		t.Fatal("About must stay open while the blocked chords are pressed")
	}
}

func TestShortcutSheetListsAbout(t *testing.T) {
	// The WINDOW group is the last block, so the sheet must be tall enough for
	// it to stay on screen instead of windowing it off the bottom.
	m := resizeModel(t, seededModel(t), 140, 70)
	m = press(t, m, ctrlKey('/'))
	if !m.shortcutsOpen {
		t.Fatal("Ctrl+/ must open the shortcuts sheet")
	}
	content := m.View().Content
	if !strings.Contains(content, "Ctrl+Shift+/") || !strings.Contains(content, "About") {
		t.Fatalf("shortcuts sheet missing the Ctrl+Shift+/ About row:\n%s", content)
	}
}
