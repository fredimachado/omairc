package ui

import (
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

const startupTestAgentID = "3e03bef6"

type configSnapshot struct {
	exists  bool
	bytes   []byte
	modTime time.Time
}

func configDir(t *testing.T) string {
	t.Helper()
	return filepath.Join(t.TempDir(), startupTestAgentID+"-xdg-config")
}

func snapshotConfig(t *testing.T) configSnapshot {
	t.Helper()
	path := storage.ConfigPath()
	info, err := os.Stat(path)
	if err != nil {
		if os.IsNotExist(err) {
			return configSnapshot{}
		}
		t.Fatalf("Stat(%q): %v", path, err)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile(%q): %v", path, err)
	}
	return configSnapshot{exists: true, bytes: raw, modTime: info.ModTime()}
}

func assertConfigUnchanged(t *testing.T, before, after configSnapshot) {
	t.Helper()
	if before.exists != after.exists {
		t.Fatalf("config existence changed: before=%v after=%v", before.exists, after.exists)
	}
	if !before.exists {
		return
	}
	if !after.modTime.Equal(before.modTime) {
		t.Fatalf("config mtime changed: before=%v after=%v", before.modTime, after.modTime)
	}
	if string(before.bytes) != string(after.bytes) {
		t.Fatalf("config bytes changed:\nbefore:\n%s\nafter:\n%s", before.bytes, after.bytes)
	}
}

func assertConfigHasNoNetworkOrderKey(t *testing.T, snap configSnapshot) {
	t.Helper()
	if strings.Contains(string(snap.bytes), "networkOrder") {
		t.Fatalf("config must not contain networkOrder:\n%s", snap.bytes)
	}
}

func storageStoredProfile(id, name string) storage.Profile {
	return storage.Profile{
		NetworkID: id,
		Name:      name,
		Host:      "irc.example",
		Port:      6697,
		TLSEnabled: true,
		Nick:       "omairc",
		Username:   "omairc",
		Realname:   "Omairc",
	}
}

func seedThreeNetworkOrder(t *testing.T, dir string) {
	t.Helper()
	t.Setenv("XDG_CONFIG_HOME", dir)
	store := storage.NewProfileStore()
	for _, spec := range []struct {
		id, name string
	}{
		{"net-alpha", "Alpha"},
		{"net-bravo", "Bravo"},
		{"net-charlie", "Charlie"},
	} {
		if status := store.Save(storageStoredProfile(spec.id, spec.name)); status != storage.StatusWritten {
			t.Fatalf("Save(%q) = %v", spec.id, status)
		}
	}
	settings := storage.OpenSettings("")
	settings.SetStringList("preferences", "networkOrder", []string{"net-charlie", "net-bravo", "net-alpha"})
	if status := settings.Sync(); status != storage.StatusWritten {
		t.Fatalf("Sync networkOrder = %v", status)
	}
}

func shellFromDisk(t *testing.T) *Model {
	t.Helper()
	ctrl := controller.New()
	ctrl.SetEphemeral(false)
	ctrl.LoadStoredPreferences()
	conn := connection.New(ctrl, func() session.Transport { return session.NewLoopbackTransport() })
	conn.SetProfileStore(storage.NewProfileStore())
	ctrl.RefreshStoredProfileIDs()
	return New(ctrl, conn)
}

// startupShellInit mirrors a plain omairc-tui launch before the event loop:
// disk stores, LoadStoredPreferences, connection wiring, and ui.New.
func startupShellInit(t *testing.T) {
	t.Helper()
	_ = shellFromDisk(t)
}

func TestStartupWithNoSavedNetworksDoesNotPersistNetworkOrder(t *testing.T) {
	cases := map[string]func(t *testing.T, dir string){
		"missing": func(t *testing.T, dir string) {},
		"empty": func(t *testing.T, dir string) {
			path := filepath.Join(dir, "omairc", "omairc.conf")
			if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
				t.Fatalf("MkdirAll: %v", err)
			}
			if err := os.WriteFile(path, nil, 0o600); err != nil {
				t.Fatalf("WriteFile: %v", err)
			}
		},
		"commentsOnly": func(t *testing.T, dir string) {
			path := filepath.Join(dir, "omairc", "omairc.conf")
			if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
				t.Fatalf("MkdirAll: %v", err)
			}
			body := "# hand-written\n; also a comment\n\n"
			if err := os.WriteFile(path, []byte(body), 0o600); err != nil {
				t.Fatalf("WriteFile: %v", err)
			}
		},
	}

	for name, seed := range cases {
		t.Run(name, func(t *testing.T) {
			dir := configDir(t)
			t.Setenv("XDG_CONFIG_HOME", dir)
			seed(t, dir)

			before := snapshotConfig(t)
			startupShellInit(t)
			mid := snapshotConfig(t)
			assertConfigUnchanged(t, before, mid)
			assertConfigHasNoNetworkOrderKey(t, mid)

			startupShellInit(t)
			after := snapshotConfig(t)
			assertConfigUnchanged(t, before, after)
			assertConfigHasNoNetworkOrderKey(t, after)
		})
	}
}

func TestNetworkOrderSurvivesRestartWithoutRewritingConf(t *testing.T) {
	dir := configDir(t)
	seedThreeNetworkOrder(t, dir)
	wantOrder := []string{"net-charlie", "net-bravo", "net-alpha"}
	wantDisplay := wantOrder

	before := snapshotConfig(t)
	m := shellFromDisk(t)
	if got := m.sidebarNetworkIDs(); !slices.Equal(got, wantDisplay) {
		t.Fatalf("sidebarNetworkIDs() = %v, want %v", got, wantDisplay)
	}
	mid := snapshotConfig(t)
	assertConfigUnchanged(t, before, mid)

	m2 := shellFromDisk(t)
	if got := m2.sidebarNetworkIDs(); !slices.Equal(got, wantDisplay) {
		t.Fatalf("after re-init sidebarNetworkIDs() = %v, want %v", got, wantDisplay)
	}
	after := snapshotConfig(t)
	assertConfigUnchanged(t, before, after)

	settings := storage.OpenSettings("")
	if got := settings.StringList("preferences", "networkOrder"); !slices.Equal(got, wantOrder) {
		t.Fatalf("networkOrder on disk = %v, want %v", got, wantOrder)
	}
}

func connectionStoredIDs(t *testing.T, conn *connection.Connection) []string {
	t.Helper()
	ids := make([]string, 0, len(conn.Networks()))
	for _, row := range conn.Networks() {
		if row.Stored {
			ids = append(ids, row.NetworkID)
		}
	}
	return ids
}

func TestNetworkOrderMoveAfterRestart(t *testing.T) {
	dir := configDir(t)
	seedThreeNetworkOrder(t, dir)
	start := []string{"net-charlie", "net-bravo", "net-alpha"}

	before := snapshotConfig(t)
	m := shellFromDisk(t)
	assertConfigUnchanged(t, before, snapshotConfig(t))
	if got := m.sidebarNetworkIDs(); !slices.Equal(got, start) {
		t.Fatalf("sidebarNetworkIDs() = %v, want %v", got, start)
	}
	if got := connectionStoredIDs(t, m.conn); !slices.Equal(got, start) {
		t.Fatalf("roster before move = %v, want %v", got, start)
	}

	m.sidebarNetworkFocusID = "net-charlie"
	m.moveFocusedNetwork(1)
	afterCharlieDown := []string{"net-bravo", "net-charlie", "net-alpha"}
	if got := m.sidebarNetworkIDs(); !slices.Equal(got, afterCharlieDown) {
		t.Fatalf("after charlie down sidebar = %v, want %v", got, afterCharlieDown)
	}
	if got := connectionStoredIDs(t, m.conn); !slices.Equal(got, afterCharlieDown) {
		t.Fatalf("after charlie down roster = %v, want %v", got, afterCharlieDown)
	}

	m2 := shellFromDisk(t)
	if got := m2.sidebarNetworkIDs(); !slices.Equal(got, afterCharlieDown) {
		t.Fatalf("after restart sidebar = %v, want %v", got, afterCharlieDown)
	}

	m2.sidebarNetworkFocusID = "net-alpha"
	m2.moveFocusedNetwork(-1)
	afterAlphaUp := []string{"net-bravo", "net-alpha", "net-charlie"}
	if got := m2.sidebarNetworkIDs(); !slices.Equal(got, afterAlphaUp) {
		t.Fatalf("after alpha up sidebar = %v, want %v", got, afterAlphaUp)
	}

	m2.sidebarNetworkFocusID = "net-bravo"
	beforeBoundary := m2.sidebarNetworkIDs()
	m2.moveFocusedNetwork(-1)
	if got := m2.sidebarNetworkIDs(); !slices.Equal(got, beforeBoundary) {
		t.Fatalf("move bravo up past top changed order to %v", got)
	}
	m2.sidebarNetworkFocusID = "net-charlie"
	m2.moveFocusedNetwork(1)
	if got := m2.sidebarNetworkIDs(); !slices.Equal(got, beforeBoundary) {
		t.Fatalf("move charlie down past bottom changed order to %v", got)
	}

	m3 := shellFromDisk(t)
	if got := m3.sidebarNetworkIDs(); !slices.Equal(got, afterAlphaUp) {
		t.Fatalf("after second restart sidebar = %v, want %v", got, afterAlphaUp)
	}
}

func TestNetworkOrderSaveNewNetworkAfterRestart(t *testing.T) {
	dir := configDir(t)
	seedThreeNetworkOrder(t, dir)
	wantBase := []string{"net-charlie", "net-bravo", "net-alpha"}

	before := snapshotConfig(t)
	m := shellFromDisk(t)
	assertConfigUnchanged(t, before, snapshotConfig(t))

	if !m.conn.Add() {
		t.Fatal("Add() = false")
	}
	m.conn.SetName("Delta")
	m.conn.SetHost("irc.delta.example")
	m.conn.SetNick("omairc")
	m.conn.SetUsername("omairc")
	m.conn.SetRealname("Omairc")
	deltaID := m.conn.SelectedNetworkID()
	if !m.conn.Apply() {
		t.Fatal("Apply() = false")
	}
	m.ctrl.RefreshStoredProfileIDs()
	m.syncSidebarNetworkOrder()
	m.alignConnectionStoredOrder()

	want := append(slices.Clone(wantBase), deltaID)
	if got := m.sidebarNetworkIDs(); !slices.Equal(got, want) {
		t.Fatalf("sidebarNetworkIDs() = %v, want %v", got, want)
	}
	if got := connectionStoredIDs(t, m.conn); !slices.Equal(got, want) {
		t.Fatalf("roster after save = %v, want %v", got, want)
	}
	settings := storage.OpenSettings("")
	if got := settings.StringList("preferences", "networkOrder"); !slices.Equal(got, want) {
		t.Fatalf("networkOrder = %v, want %v", got, want)
	}
}

func TestNetworkOrderAfterSaveContainsOnlyStoredIDs(t *testing.T) {
	dir := configDir(t)
	t.Setenv("XDG_CONFIG_HOME", dir)
	store := storage.NewProfileStore()

	ctrl := controller.New()
	ctrl.SetEphemeral(false)
	ctrl.LoadStoredPreferences()
	conn := connection.New(ctrl, func() session.Transport { return session.NewLoopbackTransport() })
	conn.SetProfileStore(store)

	first := connection.SuggestedProfile()
	first.NetworkID = "net-first"
	first.Nick = "omairc"
	first.Username = "omairc"
	first.Realname = "Omairc"
	first.Name = "First"
	second := first
	second.NetworkID = "net-second"
	second.Name = "Second"
	for _, profile := range []connection.NetworkProfile{first, second} {
		if status := store.Save(storageStoredProfile(profile.NetworkID, profile.Name)); status != storage.StatusWritten {
			t.Fatalf("Save(%q) = %v", profile.NetworkID, status)
		}
	}
	conn.SetProfileStore(store)
	ctrl.RefreshStoredProfileIDs()

	m := New(ctrl, conn)
	if !conn.Add() {
		t.Fatal("Add() = false, want a Connect draft")
	}
	draftID := conn.SelectedNetworkID()
	if draftID == "" || draftID == first.NetworkID || draftID == second.NetworkID {
		t.Fatalf("draft id %q must be a fresh unsaved profile", draftID)
	}
	m.sidebarNetworkFocusID = second.NetworkID
	m.moveFocusedNetwork(-1)

	settings := storage.OpenSettings("")
	order := settings.StringList("preferences", "networkOrder")
	wantStored := []string{second.NetworkID, first.NetworkID}
	if !slices.Equal(order, wantStored) {
		t.Fatalf("networkOrder = %#v, want %#v", order, wantStored)
	}
	for _, id := range order {
		if id == draftID {
			t.Fatalf("networkOrder contains draft id %q", draftID)
		}
	}
}
