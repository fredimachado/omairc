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
		"chat.freenode.net", "6697", "Nick is required", "Discard", "Apply",
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

// TestWindowCardRowsPinsHeaderAndFooter pins the viewport arithmetic a card uses
// to window its content: the header and footer rows stay put and the middle
// scrolls the least amount that keeps the focused row visible.
func TestWindowCardRowsPinsHeaderAndFooter(t *testing.T) {
	rows := make([]string, 12)
	for index := range rows {
		rows[index] = string(rune('a' + index))
	}
	cases := []struct {
		name   string
		focus  int
		budget int
		want   []string
	}{
		{"top focus needs no scroll", 2, 6, []string{"a", "b", "c", "d", "k", "l"}},
		{"mid focus scrolls just past the header", 6, 6, []string{"a", "b", "f", "g", "k", "l"}},
		{"bottom focus shows the last middle rows", 9, 6, []string{"a", "b", "i", "j", "k", "l"}},
		{"footer focus shows the end of the middle", 11, 6, []string{"a", "b", "i", "j", "k", "l"}},
		{"no room for a middle keeps header and tail", 5, 4, []string{"a", "b", "k", "l"}},
	}
	for _, c := range cases {
		if got := windowCardRows(rows, 2, 2, c.focus, c.budget); !reflect.DeepEqual(got, c.want) {
			t.Fatalf("%s: window = %v, want %v", c.name, got, c.want)
		}
	}
	// Content that already fits is returned untouched.
	if got := windowCardRows(rows, 2, 2, 5, 20); !reflect.DeepEqual(got, rows) {
		t.Fatalf("fitting content changed: %v", got)
	}
}

// TestConnectSheetScrollsToTheFocusedField pins the short-terminal viewport: at
// a height where the sheet does not fit, the title and tabs stay pinned at the
// top and the validation/action rows at the bottom while the middle scrolls the
// focused field into view, so Ctrl+Enter still has an Apply to act on.
func TestConnectSheetScrollsToTheFocusedField(t *testing.T) {
	m := connectSheetModel(t)
	m = resizeModel(t, m, 90, 14)

	// Focus the last text field, which sits near the bottom of the form.
	target := -1
	for index, stop := range m.connectStops() {
		if stop.kind == stopField && connectField(stop.index) == fieldNickServPassword {
			target = index
		}
	}
	if target < 0 {
		t.Fatal("the Connection tab must expose the NickServ password field")
	}
	m.sheet.focus = target
	m.syncSheetField()

	view := m.View().Content
	plain := ansiPattern.ReplaceAllString(view, "")
	for _, want := range []string{"Connect", "NickServ password", "Apply"} {
		if !strings.Contains(plain, want) {
			t.Fatalf("windowed sheet missing %q:\n%s", want, plain)
		}
	}
	if got, want := len(strings.Split(view, "\n")), m.height; got != want {
		t.Fatalf("windowed sheet frame = %d rows, want the %d-row terminal", got, want)
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
