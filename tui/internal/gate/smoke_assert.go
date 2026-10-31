package gate

import (
	"errors"
	"os/exec"
	"strings"
	"testing"
)

// assertDemoQuitExitClean checks the real omairc-tui process after Ctrl+Q
// through main's quit path: exit 0 and no panic text on the PTY stream.
func assertDemoQuitExitClean(t *testing.T, waitErr error, captured string) {
	t.Helper()
	if waitErr != nil {
		var exitErr *exec.ExitError
		if errors.As(waitErr, &exitErr) {
			t.Fatalf("ctrl+q exit = %d, want 0; err=%v\n%s", exitErr.ExitCode(), waitErr, tailCaptured(captured))
		}
		t.Fatalf("ctrl+q wait: %v\n%s", waitErr, tailCaptured(captured))
	}
	if strings.Contains(captured, "panic:") {
		t.Fatalf("ctrl+q output contains panic:\n%s", tailCaptured(captured))
	}
}

func tailCaptured(text string) string {
	if len(text) > 8000 {
		text = text[len(text)-8000:]
	}
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
