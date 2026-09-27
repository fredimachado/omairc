//go:build linux

package theme

import (
	"os"
	"path/filepath"
	"testing"
)

func TestColorsPath(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		t.Skipf("no home directory: %v", err)
	}
	want := filepath.Join(home, ".local", "state", "omarchy", "current", "theme", "colors.toml")
	if got := colorsPath(); got != want {
		t.Fatalf("colorsPath() = %q, want %q", got, want)
	}
}

func TestWatchDirsCoversCurrentAndTheme(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		t.Skipf("no home directory: %v", err)
	}
	current := filepath.Join(home, ".local", "state", "omarchy", "current")
	want := []string{current, filepath.Join(current, "theme")}
	got := watchDirs()
	if len(got) != len(want) {
		t.Fatalf("watchDirs() = %v, want %v", got, want)
	}
	for index := range want {
		if got[index] != want[index] {
			t.Fatalf("watchDirs()[%d] = %q, want %q", index, got[index], want[index])
		}
	}
}
