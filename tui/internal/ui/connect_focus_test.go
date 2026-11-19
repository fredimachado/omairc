package ui

import (
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
)

// storedConnectSheet opens Connect over one stored, clean profile, so the rail
// offers "+ Add network" until the draft is edited.
func storedConnectSheet(t *testing.T) *Model {
	t.Helper()
	ctrl := controller.New()
	conn := connection.New(ctrl, nil)
	conn.SetStoredProfiles([]connection.NetworkProfile{storedProfile("net-a", "irc.a.example")})
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)
	m.openConnect()
	if !m.conn.CanAdd() {
		t.Fatal("a clean stored profile should offer Add network")
	}
	return m
}

func requireFocusedField(t *testing.T, m *Model, want connectField) {
	t.Helper()
	stop, ok := m.focusedStop()
	if !ok || stop.kind != stopField || connectField(stop.index) != want {
		t.Fatalf("focused stop = %+v (ok=%v), want field %d", stop, ok, want)
	}
}

// TestConnectEditKeepsFocusOnTheEditedField pins the focus walk across a stop
// that leaves: typing into Name makes the draft dirty, which hides "+ Add
// network" above the form. Focus stays on Name, Name's text stays in Name, and
// Tab walks to Host, as the desktop's identity-based stops do.
func TestConnectEditKeepsFocusOnTheEditedField(t *testing.T) {
	m := storedConnectSheet(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
	requireFocusedField(t, m, fieldName)

	m = press(t, m, tea.KeyPressMsg{Code: 'Z', Text: "Z"})
	if m.conn.CanAdd() {
		t.Fatal("an edited draft should hide Add network")
	}
	requireFocusedField(t, m, fieldName)
	m = press(t, m, tea.KeyPressMsg{Code: 'Y', Text: "Y"})
	if got := m.conn.Name(); got != "irc.a.exampleZY" {
		t.Fatalf("Name = %q, want irc.a.exampleZY", got)
	}
	if got := m.conn.Host(); got != "irc.a.example" {
		t.Fatalf("Host = %q, want it untouched by Name's text", got)
	}
	if got := m.fieldDisplayValue(connectStop{kind: stopField, index: int(fieldHost)}); got != "irc.a.example" {
		t.Fatalf("Host row shows %q, want irc.a.example", got)
	}

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
	requireFocusedField(t, m, fieldHost)
	if got := m.conn.Host(); got != "irc.a.example" {
		t.Fatalf("Host = %q after Tab, want irc.a.example", got)
	}
}

// TestConnectAddFocusesName pins Ctrl+N and Enter on "+ Add network": the new
// draft lands on Name, as the desktop's add() requests Stop::Field(Name).
func TestConnectAddFocusesName(t *testing.T) {
	m := storedConnectSheet(t)
	m = press(t, m, tea.KeyPressMsg{Code: 'n', Mod: tea.ModCtrl})
	if len(m.conn.Networks()) != 2 {
		t.Fatalf("Ctrl+N networks = %d, want a new draft row", len(m.conn.Networks()))
	}
	requireFocusedField(t, m, fieldName)

	m = storedConnectSheet(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
	if stop, _ := m.focusedStop(); stop.kind != stopAddNetwork {
		t.Fatalf("one Tab from the rail = %+v, want Add network", stop)
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	requireFocusedField(t, m, fieldName)
}
