package ui

import (
	"reflect"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
)

// This file covers the Wave 4 L7 restyle of the Connect sheet: the shared
// overlay frame stays model.go's job, and the sheet keeps its labels and
// problem text while swapping checkboxes for chips and pills. Behavior,
// chords, and the title are untouched, so the assertions here are presentation
// plus the observables the feature map pins.

// connectSheetModel is the first-run sheet over a fresh connection.
func connectSheetModel(t *testing.T) *Model {
	t.Helper()
	conn := connection.New(controller.New(), nil)
	m, _ := sizedModel(t, conn)
	return m
}

// TestConnectCardFitsSharedFrame pins that the Connect sheet still routes
// through the single overlay-card frame: one shared outer width and one panel
// border, so it never re-derives its own frame.
func TestConnectCardFitsSharedFrame(t *testing.T) {
	m := connectSheetModel(t)
	card := m.connectCard(m.width)
	if got, want := lipgloss.Width(card), m.width-2*overlayCardInset; got != want {
		t.Fatalf("connect card width = %d, want the shared outer width %d", got, want)
	}
	plain := strings.TrimRight(ansiPattern.ReplaceAllString(card, ""), "\n")
	if !strings.HasPrefix(plain, "╭") || !strings.HasSuffix(plain, "╯") {
		t.Fatalf("connect card must be the shared panel frame:\n%s", card)
	}
}

// TestConnectSheetKeepsItsObservables guards the labels, tabs, and problem
// strings the tests and the feature map assert. The restyle may move them, but
// it may not rename or drop them.
func TestConnectSheetKeepsItsObservables(t *testing.T) {
	m := connectSheetModel(t)
	view := ansiPattern.ReplaceAllString(m.View().Content, "")
	for _, wanted := range []string{
		"Connect", "Connection", "Preferences", "NETWORKS",
		"irc.libera.chat", "6697", "Nick is required", "Discard", "Apply",
	} {
		if !strings.Contains(view, wanted) {
			t.Fatalf("first-run sheet missing %q:\n%s", wanted, view)
		}
	}

	// The Preferences tab keeps its own toggle labels.
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModCtrl})
	view = ansiPattern.ReplaceAllString(m.View().Content, "")
	for _, wanted := range []string{
		"Reopen direct messages on startup", "Show peer avatars", "Open conversations at unread",
	} {
		if !strings.Contains(view, wanted) {
			t.Fatalf("Preferences tab missing %q:\n%s", wanted, view)
		}
	}
}

// TestConnectProblemLineIsDangerStyled pins the L7 change: the validation line
// renders through the danger style, not the old sheet-problem style, while the
// problem text itself is unchanged.
func TestConnectProblemLineIsDangerStyled(t *testing.T) {
	m := connectSheetModel(t)
	card := m.connectCard(m.width)
	if !strings.Contains(card, "Nick is required") {
		t.Fatalf("the problem line must keep its text:\n%s", card)
	}
	if want := m.styles.StatusErr.Render("✗ Nick is required"); !strings.Contains(card, want) {
		t.Fatalf("the problem line must render through the danger style:\n%s", card)
	}
	if old := m.styles.SheetProblem.Render("Nick is required"); strings.Contains(card, old) {
		t.Fatalf("the problem line must not use the old SheetProblem style:\n%s", card)
	}
}

// TestConnectTogglesAreChips pins that the bracketed checkboxes became on/off
// chips while the toggle state still reads from the connection.
func TestConnectTogglesAreChips(t *testing.T) {
	m := connectSheetModel(t)
	card := m.connectCard(m.width)
	if strings.Contains(card, "[x]") || strings.Contains(card, "[ ]") {
		t.Fatalf("toggles must not render bracketed checkboxes:\n%s", card)
	}
	onChip := m.connectChip(true)
	offChip := m.connectChip(false)
	if !strings.Contains(card, onChip) {
		t.Fatalf("TLS on must render the on chip:\n%s", card)
	}
	if !strings.Contains(card, offChip) {
		t.Fatalf("Connect automatically off must render the off chip:\n%s", card)
	}
}

// TestConnectTabsUseTheTabStyles pins the tab rail: the active tab takes the
// active style and the inactive tab stays muted.
func TestConnectTabsUseTheTabStyles(t *testing.T) {
	m := connectSheetModel(t)
	card := m.connectCard(m.width)
	active := m.styles.SheetTabActive.Background(m.styles.Colors.SurfaceRaised).Render(" Connection ")
	inactive := m.styles.SheetTab.Background(m.styles.Colors.SurfaceRaised).Render(" Preferences ")
	if !strings.Contains(card, active) || !strings.Contains(card, inactive) {
		t.Fatalf("Connection tab must be active and Preferences muted:\n%s", card)
	}
}

// TestConnectSheetAdoptsSharedTextInput pins the input adoption: the sheet's
// live text field is built by newTextInput (default palette) and re-applies the
// live palette when focus lands on a field, so it shares the composer's styles.
func TestConnectSheetAdoptsSharedTextInput(t *testing.T) {
	if got, want := newConnectSheetState().input.Styles(), defaultStyles().Input; !reflect.DeepEqual(got, want) {
		t.Fatal("newConnectSheetState must build its input through newTextInput")
	}

	m := connectSheetModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab}) // network row -> Name
	if !m.sheet.input.Focused() {
		t.Fatal("Tab from the network rail must focus the Name field")
	}
	if got, want := m.sheet.input.Styles(), m.styles.Input; !reflect.DeepEqual(got, want) {
		t.Fatal("the focused sheet field must carry the live theme input styles")
	}
	if m.sheet.input.Prompt != "" {
		t.Fatalf("the sheet field must keep an empty prompt, got %q", m.sheet.input.Prompt)
	}
}
