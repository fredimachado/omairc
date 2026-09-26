//go:build !windows

package gate

import (
	"testing"
)

// TestProcessStartAndWait proves the PTY wrapper starts a child, exposes its
// pid, and reaps it.
func TestProcessStartAndWait(t *testing.T) {
	proc, err := Start(Options{
		Binary: "/bin/sh",
		Args:   []string{"-c", "exit 0"},
		Cols:   40,
		Rows:   10,
	})
	if err != nil {
		t.Fatalf("Start: %v", err)
	}
	defer proc.Close()
	if proc.Pid() == 0 {
		t.Fatalf("Pid() = 0, want a live child")
	}
	if err := proc.Wait(); err != nil {
		t.Fatalf("Wait: %v", err)
	}
	if proc.Alive() {
		t.Fatalf("child is still alive after Wait")
	}
}

// TestProcessDefaultsSize proves a zero size still starts.
func TestProcessDefaultsSize(t *testing.T) {
	proc, err := Start(Options{Binary: "/bin/sh", Args: []string{"-c", "exit 0"}})
	if err != nil {
		t.Fatalf("Start with default size: %v", err)
	}
	defer proc.Close()
	if err := proc.Wait(); err != nil {
		t.Fatalf("Wait: %v", err)
	}
}

func TestStartRejectsEmptyBinary(t *testing.T) {
	if _, err := Start(Options{}); err == nil {
		t.Fatalf("Start with no binary should error")
	}
}
