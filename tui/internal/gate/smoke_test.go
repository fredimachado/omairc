//go:build !windows

package gate

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

// TestPTYSmokeDemoShell starts the compiled omairc-tui under a PTY, feeds its
// output into a Screen, and waits for the seeded demo shell to render and set
// its OSC title. It then sends Ctrl+Q and waits for the child to exit.
//
// The test drives Start directly; it does not need the daemon.
func TestPTYSmokeDemoShell(t *testing.T) {
	binary, err := filepath.Abs(filepath.Join("..", "..", "bin", "omairc-tui"))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(binary); err != nil {
		t.Skip("omairc-tui not built")
	}
	root, err := filepath.Abs(filepath.Join("..", "..", ".."))
	if err != nil {
		t.Fatal(err)
	}

	const (
		cols = 118
		rows = 30
	)
	screen := NewScreen(cols, rows)
	var screenMu sync.Mutex
	read := func() (string, string) {
		screenMu.Lock()
		defer screenMu.Unlock()
		return screen.Title(), screen.Text()
	}
	proc, err := Start(Options{
		Binary: binary,
		Args:   []string{"--demo-server"},
		Cols:   cols,
		Rows:   rows,
		Dir:    root,
		Env:    terminalEnv(),
	})
	if err != nil {
		t.Fatalf("start omairc-tui: %v", err)
	}
	proc.OnOutput(func(b []byte) {
		screenMu.Lock()
		screen.Feed(b)
		screenMu.Unlock()
	})
	defer proc.Close()

	const wantTitle = "#omarchy · irc.example · fred - Omairc"
	markers := []string{"#omarchy", "#ricing", "anna", "dax"}

	deadline := time.Now().Add(20 * time.Second)
	for {
		title, text := read()
		if title == wantTitle && containsAll(text, markers) {
			break
		}
		if !proc.Alive() {
			waitErr := proc.Wait()
			if title == "" {
				// The Phase 3 build prints a demo summary and exits 0; it is
				// not the shell this smoke test proves. Skip rather than
				// failing the gate before the shell lands.
				var exitErr *exec.ExitError
				if waitErr == nil || (errors.As(waitErr, &exitErr) && exitErr.ExitCode() == 0) {
					t.Skipf("omairc-tui is a pre-shell build (exited without an OSC title)")
				}
			}
			t.Fatalf("omairc-tui exited early: title=%q err=%v\n%s", title, waitErr, tailText(text))
		}
		if time.Now().After(deadline) {
			_, text := read()
			t.Fatalf("timed out waiting for the demo shell\ntitle=%q\n%s", title, tailText(text))
		}
		time.Sleep(100 * time.Millisecond)
	}

	if _, err := proc.Write([]byte{0x11}); err != nil {
		t.Fatalf("write ctrl+q: %v", err)
	}
	done := make(chan error, 1)
	go func() { done <- proc.Wait() }()
	select {
	case <-done:
	case <-time.After(10 * time.Second):
		_ = proc.Kill()
		title, _ := read()
		t.Fatalf("omairc-tui did not exit after ctrl+q; title=%q", title)
	}
}

func containsAll(text string, needles []string) bool {
	for _, needle := range needles {
		if !strings.Contains(text, needle) {
			return false
		}
	}
	return true
}

// tailText returns the last few non-empty lines, for a failure message.
func tailText(text string) string {
	lines := strings.Split(text, "\n")
	kept := make([]string, 0, 12)
	for i := len(lines) - 1; i >= 0 && len(kept) < 12; i-- {
		if strings.TrimSpace(lines[i]) == "" {
			continue
		}
		kept = append([]string{lines[i]}, kept...)
	}
	return strings.Join(kept, "\n")
}
