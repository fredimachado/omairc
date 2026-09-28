//go:build windows

package storage

import (
	"path/filepath"
	"testing"
)

func TestConfigPathHonoursLocalAppData(t *testing.T) {
	root := t.TempDir()
	t.Setenv("LOCALAPPDATA", root)
	t.Setenv("XDG_CONFIG_HOME", "")

	want := filepath.Join(root, "omairc", "omairc.conf")
	if got := ConfigPath(); got != want {
		t.Errorf("ConfigPath() = %q, want %q", got, want)
	}
	if got := GenericConfigRoot(); got != root {
		t.Errorf("GenericConfigRoot() = %q, want %q", got, root)
	}
}

func TestConfigPathHonoursXDGConfigHome(t *testing.T) {
	root := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", root)
	t.Setenv("LOCALAPPDATA", t.TempDir())

	want := filepath.Join(root, "omairc", "omairc.conf")
	if got := ConfigPath(); got != want {
		t.Errorf("ConfigPath() = %q, want %q", got, want)
	}
}

func TestTranscriptRootHonoursOverride(t *testing.T) {
	root := t.TempDir()
	t.Setenv("OMAIRC_TRANSCRIPT_ROOT", root)

	if got := TranscriptRoot(); got != root {
		t.Errorf("TranscriptRoot() = %q, want the override %q", got, root)
	}
}

func TestTranscriptRootFallsBackToLocalAppData(t *testing.T) {
	t.Setenv("OMAIRC_TRANSCRIPT_ROOT", "")
	root := t.TempDir()
	t.Setenv("LOCALAPPDATA", root)
	t.Setenv("XDG_STATE_HOME", "")

	want := filepath.Join(root, "omairc", "logs")
	if got := TranscriptRoot(); got != want {
		t.Errorf("TranscriptRoot() = %q, want %q", got, want)
	}
	if got := GenericStateRoot(); got != root {
		t.Errorf("GenericStateRoot() = %q, want %q", got, root)
	}
}

func TestTranscriptRootHonoursXDGStateHome(t *testing.T) {
	t.Setenv("OMAIRC_TRANSCRIPT_ROOT", "")
	state := t.TempDir()
	t.Setenv("XDG_STATE_HOME", state)
	t.Setenv("LOCALAPPDATA", t.TempDir())

	want := filepath.Join(state, "omairc", "logs")
	if got := TranscriptRoot(); got != want {
		t.Errorf("TranscriptRoot() = %q, want %q", got, want)
	}
}

func TestWindowsCredentialTarget(t *testing.T) {
	key := "omairc/v1/6:libera/5:alice/15:irc.libera.chat"
	want := key + "@omairc"
	if got := windowsCredentialTarget(key); got != want {
		t.Fatalf("windowsCredentialTarget = %q, want %q", got, want)
	}
	if got := windowsCredentialTarget(""); got != "omairc" {
		t.Fatalf("empty key target = %q, want omairc", got)
	}
}
