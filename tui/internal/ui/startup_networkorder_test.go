package ui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

const startupTestAgentID = "cb7e"

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
	if strings.Contains(string(after.bytes), "networkOrder") {
		t.Fatalf("config must not contain networkOrder:\n%s", after.bytes)
	}
}

// startupShellInit mirrors a plain omairc-tui launch before the event loop:
// disk stores, LoadStoredPreferences, connection wiring, and ui.New (which
// syncs sidebar network order once).
func startupShellInit(t *testing.T) {
	t.Helper()
	ctrl := controller.New()
	ctrl.SetEphemeral(false)
	ctrl.LoadStoredPreferences()
	conn := connection.New(ctrl, func() session.Transport { return session.NewLoopbackTransport() })
	conn.SetProfileStore(storage.NewProfileStore())
	_ = New(ctrl, conn)
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

			startupShellInit(t)
			after := snapshotConfig(t)
			assertConfigUnchanged(t, before, after)
		})
	}
}

func TestNetworkOrderAfterSaveContainsOnlyStoredIDs(t *testing.T) {
	dir := configDir(t)
	t.Setenv("XDG_CONFIG_HOME", dir)

	ctrl := controller.New()
	ctrl.SetEphemeral(false)
	ctrl.LoadStoredPreferences()
	conn := connection.New(ctrl, func() session.Transport { return session.NewLoopbackTransport() })
	conn.SetProfileStore(storage.NewProfileStore())

	saved := connection.SuggestedProfile()
	saved.Nick = "omairc"
	saved.Username = "omairc"
	saved.Realname = "Omairc"
	conn.SetStoredProfiles([]connection.NetworkProfile{saved})

	_ = New(ctrl, conn)

	settings := storage.OpenSettings("")
	order := settings.StringList("preferences", "networkOrder")
	if len(order) != 1 || order[0] != saved.NetworkID {
		t.Fatalf("networkOrder = %#v, want [%q]", order, saved.NetworkID)
	}
	if draftID := conn.SelectedNetworkID(); draftID != saved.NetworkID {
		t.Fatalf("selected draft id %q must not appear alone in order", draftID)
	}
}
