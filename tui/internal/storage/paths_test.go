//go:build linux

package storage

import (
	"path/filepath"
	"testing"
)

func TestConfigPathHonoursXDGConfigHome(t *testing.T) {
	root := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", root)

	want := filepath.Join(root, "omairc", "omairc.conf")
	if got := ConfigPath(); got != want {
		t.Errorf("ConfigPath() = %q, want %q", got, want)
	}
	if got := GenericConfigRoot(); got != root {
		t.Errorf("GenericConfigRoot() = %q, want %q", got, root)
	}
}

func TestTranscriptRootHonoursOverride(t *testing.T) {
	root := t.TempDir()
	t.Setenv("OMAIRC_TRANSCRIPT_ROOT", root)

	if got := TranscriptRoot(); got != root {
		t.Errorf("TranscriptRoot() = %q, want the override %q", got, root)
	}
}

func TestTranscriptRootFallsBackToStateHome(t *testing.T) {
	t.Setenv("OMAIRC_TRANSCRIPT_ROOT", "")
	state := t.TempDir()
	t.Setenv("XDG_STATE_HOME", state)
	t.Setenv("OMAIRC_DEV_HOME", "")

	want := filepath.Join(state, "omairc", "logs")
	if got := TranscriptRoot(); got != want {
		t.Errorf("TranscriptRoot() = %q, want %q", got, want)
	}
	if got := GenericStateRoot(); got != state {
		t.Errorf("GenericStateRoot() = %q, want %q", got, state)
	}
	if got := ScrollPlacesPath(); got != filepath.Join(state, "omairc", ScrollPlacesFile) {
		t.Errorf("ScrollPlacesPath() = %q, want under XDG_STATE_HOME", got)
	}
}

func TestScrollPlacesPathHonoursDevHome(t *testing.T) {
	dev := t.TempDir()
	t.Setenv("OMAIRC_DEV_HOME", dev)
	t.Setenv("XDG_STATE_HOME", t.TempDir())
	want := filepath.Join(dev, "state", ScrollPlacesFile)
	if got := ScrollPlacesPath(); got != want {
		t.Errorf("ScrollPlacesPath() = %q, want %q", got, want)
	}
}

func TestScrollPlacesPathMatchesDesktopWithoutXDGOverride(t *testing.T) {
	// On Linux the default is ~/.local/state/omairc/scrollplaces.ini, same as
	// the desktop. Clear XDG_STATE_HOME so the shared helper cannot hide a
	// Windows-style LocalAppData mismatch.
	t.Setenv("XDG_STATE_HOME", "")
	t.Setenv("OMAIRC_DEV_HOME", "")
	home := t.TempDir()
	t.Setenv("HOME", home)
	want := filepath.Join(home, ".local", "state", "omairc", ScrollPlacesFile)
	if got := ScrollPlacesPath(); got != want {
		t.Errorf("ScrollPlacesPath() = %q, want %q", got, want)
	}
}
