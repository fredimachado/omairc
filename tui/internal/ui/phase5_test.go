package ui

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/session"
)

func storedProfile(id, host string) connection.NetworkProfile {
	profile := connection.CreateProfile()
	profile.NetworkID = id
	profile.Name = host
	profile.Host = host
	profile.Port = 6697
	profile.TLSEnabled = true
	profile.Nick = "omairc"
	return profile
}

// press folds one key press into the shell and returns the updated model.
func press(t *testing.T, m *Model, msg tea.KeyPressMsg) *Model {
	t.Helper()
	updated, _ := m.Update(msg)
	model, ok := updated.(*Model)
	if !ok {
		t.Fatalf("Update returned %T, want *Model", updated)
	}
	return model
}

func sizedModel(t *testing.T, conn *connection.Connection) (*Model, *controller.Controller) {
	t.Helper()
	ctrl := controller.New()
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	return updated.(*Model), ctrl
}

func seededModel(t *testing.T) *Model {
	t.Helper()
	ctrl := controller.New()
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := New(ctrl, nil)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	return updated.(*Model)
}

func TestFirstRunConnectSheet(t *testing.T) {
	conn := connection.New(controller.New(), nil)
	m, _ := sizedModel(t, conn)

	view := m.View()
	if view.WindowTitle != "irc.libera.chat Status" {
		t.Fatalf("first-run title = %q, want %q", view.WindowTitle, "irc.libera.chat Status")
	}
	for _, wanted := range []string{"Connect", "irc.libera.chat", "Nick is required", "Apply"} {
		if !strings.Contains(view.Content, wanted) {
			t.Fatalf("first-run sheet missing %q:\n%s", wanted, view.Content)
		}
	}

	// Tab, Shift+Tab, and Enter walk focus and leave the sheet open on first
	// run, exactly as the connect fence drives it.
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModShift})
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})

	if m.View().WindowTitle != "irc.libera.chat Status" {
		t.Fatalf("after the focus walk title = %q, want it unchanged", m.View().WindowTitle)
	}
	if !strings.Contains(m.View().Content, "Nick is required") {
		t.Fatal("the focus walk must keep the Nick is required problem line")
	}
}

func TestStoredProfilesListedInSidebarBeforeConnect(t *testing.T) {
	ctrl := controller.New()
	conn := connection.New(ctrl, nil)
	conn.SetStoredProfiles([]connection.NetworkProfile{
		storedProfile("net-a", "irc.a.example"),
		storedProfile("net-b", "irc.b.example"),
	})
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)
	sidebar := m.sidebarView(sidebarWidth(m.width), m.bodyHeight())
	for _, wanted := range []string{"irc.a.example", "irc.b.example"} {
		if !strings.Contains(sidebar, wanted) {
			t.Fatalf("sidebar missing %q before connect:\n%s", wanted, sidebar)
		}
	}
}

func TestConnectCtrlJAppliesFromField(t *testing.T) {
	ctrl := controller.New()
	conn := connection.New(ctrl, func() session.Transport { return session.NewLoopbackTransport() })
	conn.SetStoredProfiles([]connection.NetworkProfile{storedProfile("net-a", "irc.a.example")})
	conn.Select("net-a")
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)
	m.connectOpen = true
	// Windows Terminal encodes Ctrl+Enter as LF, which ultraviolet maps to ctrl+j.
	m = press(t, m, tea.KeyPressMsg{Code: 'j', Mod: tea.ModCtrl})
	if !ctrl.ConsoleOpen() {
		t.Fatal("ctrl+j must apply from a Connect field on Windows VT")
	}
	if m.connectVisible() {
		t.Fatal("Connect sheet must close after ctrl+j apply")
	}
}

func TestConnectCtrlEnterAppliesFromField(t *testing.T) {
	ctrl := controller.New()
	conn := connection.New(ctrl, func() session.Transport { return session.NewLoopbackTransport() })
	conn.SetStoredProfiles([]connection.NetworkProfile{storedProfile("net-a", "irc.a.example")})
	conn.Select("net-a")
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)
	m.connectOpen = true
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter, Mod: tea.ModCtrl})
	if !ctrl.ConsoleOpen() {
		t.Fatal("Ctrl+Enter must apply from a Connect field")
	}
	if m.connectVisible() {
		t.Fatal("Connect sheet must close after Ctrl+Enter apply")
	}
}

func TestConnectApplyKeyMatchesReturnAlias(t *testing.T) {
	ctrlEnter := tea.KeyPressMsg{Code: tea.KeyEnter, Mod: tea.ModCtrl}
	if !isConnectApplyKey("ctrl+return", ctrlEnter) {
		t.Fatal("ctrl+return must count as Connect apply")
	}
	if !isConnectApplyKey("ctrl+j", tea.KeyPressMsg{Code: 'j', Mod: tea.ModCtrl}) {
		t.Fatal("ctrl+j must count as Connect apply")
	}
	if !isConnectApplyKey("enter", ctrlEnter) {
		t.Fatal("enter keystroke with ctrl modifier must count as Connect apply")
	}
	if isConnectApplyKey("enter", tea.KeyPressMsg{Code: tea.KeyEnter}) {
		t.Fatal("plain Enter must not count as Connect apply")
	}
	if isConnectApplyKey("j", tea.KeyPressMsg{Code: 'j'}) {
		t.Fatal("j without ctrl modifier must not count as Connect apply")
	}
}

func TestApplyOpensStatusTranscript(t *testing.T) {
	ctrl := controller.New()
	conn := connection.New(ctrl, func() session.Transport { return session.NewLoopbackTransport() })
	conn.SetStoredProfiles([]connection.NetworkProfile{storedProfile("net-a", "irc.a.example")})
	conn.Select("net-a")
	m := New(ctrl, conn)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)
	m.applyConnect()
	if !ctrl.ConsoleOpen() {
		t.Fatal("ConsoleOpen = false after Apply, want true")
	}
	if got := ctrl.FocusedNetworkID(); got != "net-a" {
		t.Fatalf("FocusedNetworkID = %q, want net-a", got)
	}
	if got := m.View().WindowTitle; got != "irc.a.example Status" {
		t.Fatalf("title after Apply = %q, want %q", got, "irc.a.example Status")
	}
	if m.connectVisible() {
		t.Fatal("Connect sheet still open after successful Apply")
	}
}

func TestWalkLandsOnRicing(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyDown, Mod: tea.ModAlt})
	if got := m.View().WindowTitle; got != "#ricing - Omairc" {
		t.Fatalf("after Alt+Down title = %q, want %q", got, "#ricing - Omairc")
	}
}

func TestUnreadLandsOnAnnaAfterRicing(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyDown, Mod: tea.ModAlt})
	m = press(t, m, tea.KeyPressMsg{Code: 'a', Mod: tea.ModAlt})
	if got := m.View().WindowTitle; got != "anna - Omairc" {
		t.Fatalf("after Alt+A title = %q, want %q", got, "anna - Omairc")
	}
}

func TestJumpFiltersAndSelectsDesktop(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: 'k', Mod: tea.ModCtrl})
	if !m.jumpVisible() {
		t.Fatal("Ctrl+K must open the jump overlay")
	}
	m.jump.input.SetValue("#desktop")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.jumpVisible() {
		t.Fatal("Enter must dismiss the jump overlay")
	}
	if got := m.View().WindowTitle; got != "#desktop - Omairc" {
		t.Fatalf("after jump title = %q, want %q", got, "#desktop - Omairc")
	}
}

func TestStatusToggleAndEscapeReturns(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: '`', Mod: tea.ModCtrl})
	if got := m.View().WindowTitle; got != "irc.example · fred Status" {
		t.Fatalf("after Ctrl+` title = %q, want %q", got, "irc.example · fred Status")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if got := m.View().WindowTitle; got != "#omarchy · irc.example · fred - Omairc" {
		t.Fatalf("after Escape title = %q, want %q", got, "#omarchy · irc.example · fred - Omairc")
	}
}

func TestDraftsFollowTheConversation(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("hello")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyDown, Mod: tea.ModAlt})
	if got := m.composer.Value(); got != "" {
		t.Fatalf("composer on the new conversation = %q, want empty", got)
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyUp, Mod: tea.ModAlt})
	if got := m.composer.Value(); got != "hello" {
		t.Fatalf("restored composer = %q, want %q", got, "hello")
	}
}
