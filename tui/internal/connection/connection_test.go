package connection

import (
	"slices"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// loopbackFactory records every transport it hands out so a test can inspect
// the host, port, and TLS flag the session connected with, and can assert that
// an unchanged Apply did not rebuild the session.
type loopbackFactory struct {
	transports []*session.LoopbackTransport
}

func (f *loopbackFactory) newTransport() session.Transport {
	transport := session.NewLoopbackTransport()
	f.transports = append(f.transports, transport)
	return transport
}

func newFixture(t *testing.T) (*Connection, *controller.Controller, *loopbackFactory) {
	t.Helper()
	ctrl := controller.New()
	factory := &loopbackFactory{}
	return New(ctrl, factory.newTransport), ctrl, factory
}

func completeProfile(id, host string) NetworkProfile {
	profile := CreateProfile()
	profile.NetworkID = id
	profile.Name = host
	profile.Host = host
	profile.Port = 6697
	profile.TLSEnabled = true
	profile.Nick = "omairc"
	return profile
}

func networkIDs(conn *Connection) []string {
	rows := conn.Networks()
	ids := make([]string, 0, len(rows))
	for _, row := range rows {
		ids = append(ids, row.NetworkID)
	}
	return ids
}

func TestFreshConnectionRequiresSetup(t *testing.T) {
	conn, _, _ := newFixture(t)
	if !conn.SetupRequired() {
		t.Fatal("SetupRequired() = false, want true with no stored profiles")
	}
	if got := conn.DisplayName(); got != "irc.libera.chat" {
		t.Fatalf("DisplayName() = %q, want irc.libera.chat", got)
	}
	if got := conn.Problem(); got != "Nick is required" {
		t.Fatalf("Problem() = %q, want %q", got, "Nick is required")
	}
	if conn.CanAdd() {
		t.Fatal("CanAdd() = true before setup")
	}
	if conn.CanRemove() {
		t.Fatal("CanRemove() = true with nothing stored")
	}
	if !conn.Dirty() {
		t.Fatal("Dirty() = false for a non-stored draft")
	}
}

func TestApplyStoresProfileAndStartsSession(t *testing.T) {
	conn, ctrl, factory := newFixture(t)
	conn.SetNick("omairc")
	if !conn.Apply() {
		t.Fatal("Apply() = false, want true")
	}
	if conn.SetupRequired() {
		t.Fatal("SetupRequired() = true after applying a complete profile")
	}
	if len(factory.transports) != 1 {
		t.Fatalf("transports = %d, want 1", len(factory.transports))
	}
	transport := factory.transports[0]
	if got := transport.ConnectedHost(); got != "irc.libera.chat" {
		t.Fatalf("ConnectedHost() = %q", got)
	}
	if got := transport.ConnectedPort(); got != 6697 {
		t.Fatalf("ConnectedPort() = %d", got)
	}
	if !transport.TLSRequested() {
		t.Fatal("TLSRequested() = false")
	}
	id := conn.SelectedNetworkID()
	if ctrl.Session(id) == nil {
		t.Fatal("session not registered")
	}
	if !conn.CanDisconnect() {
		t.Fatal("CanDisconnect() = false while live")
	}
	if conn.Dirty() {
		t.Fatal("Dirty() = true after apply")
	}
	if !conn.CanRemove() || !conn.CanAdd() {
		t.Fatalf("CanRemove()/CanAdd() = %v/%v, want true/true", conn.CanRemove(), conn.CanAdd())
	}
	rows := conn.Networks()
	if len(rows) != 1 || !rows[0].Stored || !rows[0].Selected {
		t.Fatalf("Networks() = %+v, want one stored selected row", rows)
	}
	if rows[0].DisplayName != "irc.libera.chat" {
		t.Fatalf("row display = %q", rows[0].DisplayName)
	}

	// An unchanged Apply restarts the existing session; it must not build a
	// second transport.
	if !conn.Apply() {
		t.Fatal("second Apply() = false")
	}
	if len(factory.transports) != 1 {
		t.Fatalf("transports after unchanged apply = %d, want 1", len(factory.transports))
	}
}

func TestApplyRebuildsSessionWhenPasswordChanges(t *testing.T) {
	conn, _, factory := newFixture(t)
	conn.SetNick("omairc")
	if !conn.Apply() {
		t.Fatal("Apply() = false")
	}
	conn.SetPassword("hunter2")
	if !conn.Apply() {
		t.Fatal("Apply() with a new password = false")
	}
	if len(factory.transports) != 2 {
		t.Fatalf("transports = %d, want 2 after a secret change", len(factory.transports))
	}
}

func TestDisconnectSelected(t *testing.T) {
	conn, _, _ := newFixture(t)
	conn.SetNick("omairc")
	if !conn.Apply() {
		t.Fatal("Apply() = false")
	}
	if !conn.CanDisconnect() {
		t.Fatal("CanDisconnect() = false before DisconnectSelected")
	}
	if !conn.DisconnectSelected() {
		t.Fatal("DisconnectSelected() = false")
	}
	if conn.CanDisconnect() {
		t.Fatal("CanDisconnect() = true after DisconnectSelected")
	}
}

func TestAddAndDiscard(t *testing.T) {
	conn, _, _ := newFixture(t)
	conn.SetNick("omairc")
	if !conn.Apply() {
		t.Fatal("Apply() = false")
	}
	original := conn.SelectedNetworkID()
	if !conn.CanAdd() {
		t.Fatal("CanAdd() = false after setup")
	}
	if !conn.Add() {
		t.Fatal("Add() = false")
	}
	draftID := conn.SelectedNetworkID()
	if draftID == original {
		t.Fatal("Add() reused the stored network id")
	}
	if !conn.Dirty() {
		t.Fatal("Dirty() = false for a fresh draft")
	}
	rows := conn.Networks()
	if len(rows) != 2 {
		t.Fatalf("Networks() = %+v, want 2 rows", rows)
	}
	last := rows[len(rows)-1]
	if last.NetworkID != draftID || last.Stored || !last.Selected {
		t.Fatalf("draft row = %+v", last)
	}
	conn.Discard()
	if conn.SelectedNetworkID() != original {
		t.Fatalf("after Discard selected = %q, want %q", conn.SelectedNetworkID(), original)
	}
	if len(conn.Networks()) != 1 {
		t.Fatalf("Networks() after Discard = %+v", conn.Networks())
	}
}

func TestRemoveSelectedSelectsNeighborAndTogglesSetup(t *testing.T) {
	conn, _, _ := newFixture(t)
	conn.SetStoredProfiles([]NetworkProfile{
		completeProfile("net-a", "irc.a.example"),
		completeProfile("net-b", "irc.b.example"),
	})
	if conn.SetupRequired() {
		t.Fatal("SetupRequired() = true after seeding complete profiles")
	}
	conn.Select("net-b")
	if conn.SelectedNetworkID() != "net-b" {
		t.Fatalf("selected = %q, want net-b", conn.SelectedNetworkID())
	}
	if !conn.RemoveSelected() {
		t.Fatal("RemoveSelected() = false")
	}
	if conn.SelectedNetworkID() != "net-a" {
		t.Fatalf("selected = %q, want the neighbor net-a", conn.SelectedNetworkID())
	}
	if conn.SetupRequired() {
		t.Fatal("SetupRequired() = true while net-a is still complete")
	}
	if !conn.RemoveSelected() {
		t.Fatal("RemoveSelected() = false for the last profile")
	}
	if !conn.SetupRequired() {
		t.Fatal("SetupRequired() = false with no stored profiles")
	}
	rows := conn.Networks()
	if len(rows) != 1 || rows[0].Stored {
		t.Fatalf("Networks() = %+v, want one non-stored draft row", rows)
	}
}

func TestMoveNetworkReordersRoster(t *testing.T) {
	conn, _, _ := newFixture(t)
	conn.SetStoredProfiles([]NetworkProfile{
		completeProfile("a", "irc.a.example"),
		completeProfile("b", "irc.b.example"),
		completeProfile("c", "irc.c.example"),
	})
	if !conn.MoveNetwork("a", 1) {
		t.Fatal("MoveNetwork(a, +1) = false")
	}
	if want := []string{"b", "a", "c"}; !slices.Equal(networkIDs(conn), want) {
		t.Fatalf("after move = %v, want %v", networkIDs(conn), want)
	}
	if !conn.MoveNetwork("c", -2) {
		t.Fatal("MoveNetwork(c, -2) = false")
	}
	if want := []string{"c", "b", "a"}; !slices.Equal(networkIDs(conn), want) {
		t.Fatalf("after second move = %v, want %v", networkIDs(conn), want)
	}
	if conn.MoveNetwork("a", 1) {
		t.Fatal("MoveNetwork out of range = true, want false")
	}
	if conn.MoveNetwork("missing", 1) {
		t.Fatal("MoveNetwork on a missing id = true, want false")
	}
	if conn.MoveNetwork("a", 0) {
		t.Fatal("MoveNetwork with delta 0 = true, want false")
	}
}

func TestDirtyBlocksSelectionUntilDiscard(t *testing.T) {
	conn, _, _ := newFixture(t)
	conn.SetStoredProfiles([]NetworkProfile{
		completeProfile("a", "irc.a.example"),
		completeProfile("b", "irc.b.example"),
	})
	conn.SetNick("changed")
	if !conn.Dirty() {
		t.Fatal("Dirty() = false after editing a stored draft")
	}
	conn.Select("b")
	if conn.SelectedNetworkID() != "a" {
		t.Fatalf("Select() while dirty moved to %q", conn.SelectedNetworkID())
	}
	conn.Discard()
	if conn.Dirty() {
		t.Fatal("Dirty() = true after Discard restored the stored profile")
	}
	if conn.Nick() != "omairc" {
		t.Fatalf("Nick() = %q, want the restored omairc", conn.Nick())
	}
	conn.Select("b")
	if conn.SelectedNetworkID() != "b" {
		t.Fatalf("selected = %q, want b", conn.SelectedNetworkID())
	}
}

func TestRosterDisplayNameDisambiguatesDuplicates(t *testing.T) {
	conn, _, _ := newFixture(t)
	first := completeProfile("a", "irc.example")
	first.Name = ""
	first.Nick = "alpha"
	second := completeProfile("b", "irc.example")
	second.Name = ""
	second.Nick = "beta"
	conn.SetStoredProfiles([]NetworkProfile{first, second})
	if got := conn.DisplayName(); got != "irc.example · alpha" {
		t.Fatalf("DisplayName() = %q, want %q", got, "irc.example · alpha")
	}
	rows := conn.Networks()
	if len(rows) != 2 {
		t.Fatalf("Networks() = %+v, want 2 rows", rows)
	}
	if rows[0].DisplayName != "irc.example · alpha" || rows[1].DisplayName != "irc.example · beta" {
		t.Fatalf("rows = %+v", rows)
	}
}

func TestSetAutojoinParsesAndRetainsKeys(t *testing.T) {
	profile := completeProfile("a", "irc.example")
	profile.AutojoinChannels = []string{"#one", "#two"}
	profile.AutojoinKeys = map[string]string{"#ONE": "secret"}
	conn, _, _ := newFixture(t)
	conn.SetStoredProfiles([]NetworkProfile{profile})
	if got := conn.Autojoin(); got != "#one #two" {
		t.Fatalf("Autojoin() = %q, want %q", got, "#one #two")
	}
	conn.SetAutojoin("#one,#two key")
	if got := conn.Autojoin(); got != "#one #two" {
		t.Fatalf("Autojoin() after SetAutojoin = %q, want %q", got, "#one #two")
	}
	if got := conn.draft.AutojoinKeys["#one"]; got != "secret" {
		t.Fatalf("retained key = %q, want secret", got)
	}
}

func TestPreferencesDefaultTrue(t *testing.T) {
	conn, _, _ := newFixture(t)
	if !conn.ReopenDirects() || !conn.ShowAvatars() || !conn.OpenAtUnread() {
		t.Fatalf("preferences = %v/%v/%v, want all true",
			conn.ReopenDirects(), conn.ShowAvatars(), conn.OpenAtUnread())
	}
	conn.SetReopenDirects(false)
	conn.SetShowAvatars(false)
	conn.SetOpenAtUnread(false)
	if conn.ReopenDirects() || conn.ShowAvatars() || conn.OpenAtUnread() {
		t.Fatal("preferences did not stick")
	}
}

func TestCallbacksFire(t *testing.T) {
	conn, _, _ := newFixture(t)
	draftCalls, networkCalls, setupCalls := 0, 0, 0
	conn.OnDraftChanged = func() { draftCalls++ }
	conn.OnNetworksChanged = func() { networkCalls++ }
	conn.OnSetupRequiredChanged = func() { setupCalls++ }

	conn.SetNick("omairc")
	if draftCalls == 0 || networkCalls == 0 {
		t.Fatalf("SetNick fired draft=%d networks=%d", draftCalls, networkCalls)
	}
	conn.Apply()
	if setupCalls == 0 {
		t.Fatal("OnSetupRequiredChanged did not fire when setup cleared")
	}
}
