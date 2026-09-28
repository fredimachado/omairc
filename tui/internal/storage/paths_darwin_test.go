//go:build darwin

package storage

import (
	"path/filepath"
	"testing"
)

func TestGenericConfigRootDefaultsToLibraryPreferences(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", "")
	home := t.TempDir()
	t.Setenv("HOME", home)

	want := filepath.Join(home, "Library", "Preferences")
	if got := GenericConfigRoot(); got != want {
		t.Errorf("GenericConfigRoot() = %q, want %q", got, want)
	}
}

func TestGenericStateRootDefaultsToLibraryPreferencesState(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", "")
	home := t.TempDir()
	t.Setenv("HOME", home)

	want := filepath.Join(home, "Library", "Preferences", "State")
	if got := GenericStateRoot(); got != want {
		t.Errorf("GenericStateRoot() = %q, want %q", got, want)
	}
}

func TestConfigPathHonoursXDGConfigHomeOnDarwin(t *testing.T) {
	root := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", root)

	want := filepath.Join(root, "omairc", "omairc.conf")
	if got := ConfigPath(); got != want {
		t.Errorf("ConfigPath() = %q, want %q", got, want)
	}
}

func TestTranscriptRootFallsBackToStateHomeOnDarwin(t *testing.T) {
	t.Setenv("OMAIRC_TRANSCRIPT_ROOT", "")
	state := t.TempDir()
	t.Setenv("XDG_STATE_HOME", state)

	want := filepath.Join(state, "omairc", "logs")
	if got := TranscriptRoot(); got != want {
		t.Errorf("TranscriptRoot() = %q, want %q", got, want)
	}
}
