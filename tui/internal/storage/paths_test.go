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

	want := filepath.Join(state, "omairc", "logs")
	if got := TranscriptRoot(); got != want {
		t.Errorf("TranscriptRoot() = %q, want %q", got, want)
	}
	if got := GenericStateRoot(); got != state {
		t.Errorf("GenericStateRoot() = %q, want %q", got, state)
	}
}
