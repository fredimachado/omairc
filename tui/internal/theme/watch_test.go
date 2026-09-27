//go:build linux

package theme

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/session"
)

const testInterval = 250 * time.Millisecond

// writeColors writes a colors.toml and forces a distinct mtime so a rewrite
// within the same test cannot be missed by timestamp granularity.
func writeColors(t *testing.T, path, mode, background string, mtime time.Time) {
	t.Helper()
	content := "mode = \"" + mode + "\"\nbackground = \"" + background + "\"\n"
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatalf("write %s: %v", path, err)
	}
	if err := os.Chtimes(path, mtime, mtime); err != nil {
		t.Fatalf("chtimes %s: %v", path, err)
	}
}

func TestWatcherDetectsRewrite(t *testing.T) {
	root := t.TempDir()
	current := filepath.Join(root, "current")
	themeDir := filepath.Join(current, "theme")
	if err := os.MkdirAll(themeDir, 0o755); err != nil {
		t.Fatal(err)
	}
	colorsFile := filepath.Join(themeDir, "colors.toml")
	base := time.Unix(1000, 0)
	writeColors(t, colorsFile, ModeDark, "#101010", base)

	clock := session.NewFakeClock(time.Unix(0, 0))
	watcher := newWatcher(clock, testInterval, colorsFile, []string{current, themeDir})
	defer watcher.Close()

	assertHex(t, "baseline", watcher.Current().Background, "#101010")

	writeColors(t, colorsFile, ModeLight, "#f5f5f5", base.Add(time.Second))
	clock.Advance(testInterval)

	select {
	case change := <-watcher.Changes():
		if !change.Found {
			t.Fatal("rewrite should report found=true")
		}
		if change.Path != colorsFile {
			t.Errorf("Path = %q, want %q", change.Path, colorsFile)
		}
		assertHex(t, "rewritten", change.Colors.Background, "#f5f5f5")
	default:
		t.Fatal("no change emitted after a rewrite")
	}
}

func TestWatcherMissingFileFallsBack(t *testing.T) {
	root := t.TempDir()
	current := filepath.Join(root, "current")
	themeDir := filepath.Join(current, "theme")
	if err := os.MkdirAll(themeDir, 0o755); err != nil {
		t.Fatal(err)
	}
	colorsFile := filepath.Join(themeDir, "colors.toml")
	writeColors(t, colorsFile, ModeDark, "#101010", time.Unix(1000, 0))

	clock := session.NewFakeClock(time.Unix(0, 0))
	watcher := newWatcher(clock, testInterval, colorsFile, []string{current, themeDir})
	defer watcher.Close()

	if err := os.Remove(colorsFile); err != nil {
		t.Fatal(err)
	}
	clock.Advance(testInterval)

	select {
	case change := <-watcher.Changes():
		if change.Found {
			t.Fatal("a missing file should report found=false")
		}
		if got, want := Hex(change.Colors.Background), Hex(Fallback().Background); got != want {
			t.Fatalf("fallback Background = %s, want %s", got, want)
		}
	default:
		t.Fatal("no change emitted after the file disappeared")
	}
}

func TestWatcherDetectsSymlinkSwap(t *testing.T) {
	root := t.TempDir()
	themeA := filepath.Join(root, "theme-a")
	themeB := filepath.Join(root, "theme-b")
	for _, dir := range []string{themeA, themeB} {
		if err := os.MkdirAll(filepath.Join(dir, "theme"), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	writeColors(t, filepath.Join(themeA, "theme", "colors.toml"), ModeDark, "#101010", time.Unix(1000, 0))
	writeColors(t, filepath.Join(themeB, "theme", "colors.toml"), ModeLight, "#f5f5f5", time.Unix(2000, 0))

	current := filepath.Join(root, "current")
	if err := os.Symlink(themeA, current); err != nil {
		t.Skipf("symlinks unavailable: %v", err)
	}
	colorsFile := filepath.Join(current, "theme", "colors.toml")

	clock := session.NewFakeClock(time.Unix(0, 0))
	watcher := newWatcher(clock, testInterval, colorsFile, []string{current, filepath.Join(current, "theme")})
	defer watcher.Close()
	assertHex(t, "baseline", watcher.Current().Background, "#101010")

	if err := os.Remove(current); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(themeB, current); err != nil {
		t.Fatal(err)
	}
	clock.Advance(testInterval)

	select {
	case change := <-watcher.Changes():
		assertHex(t, "swapped", change.Colors.Background, "#f5f5f5")
	default:
		t.Fatal("no change emitted after the symlink swap")
	}
}

func TestWatcherCloseIsIdempotentAndStops(t *testing.T) {
	root := t.TempDir()
	current := filepath.Join(root, "current")
	themeDir := filepath.Join(current, "theme")
	if err := os.MkdirAll(themeDir, 0o755); err != nil {
		t.Fatal(err)
	}
	colorsFile := filepath.Join(themeDir, "colors.toml")
	writeColors(t, colorsFile, ModeDark, "#101010", time.Unix(1000, 0))

	clock := session.NewFakeClock(time.Unix(0, 0))
	watcher := newWatcher(clock, testInterval, colorsFile, []string{current, themeDir})
	watcher.Close()
	watcher.Close()

	if _, ok := <-watcher.Changes(); ok {
		t.Fatal("Changes should be closed after Close")
	}

	// Advancing past the poll deadline must not panic or emit after Close.
	writeColors(t, colorsFile, ModeLight, "#f5f5f5", time.Unix(2000, 0))
	clock.Advance(2 * testInterval)
}
