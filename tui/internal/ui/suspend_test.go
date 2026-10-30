package ui

import (
	"testing"

	tea "charm.land/bubbletea/v2"
)

// TestCtrlZSuspends pins the normal-path job-control chord: raw mode clears
// ISIG, so Ctrl+Z arrives as a key and must ask Bubble Tea to suspend.
func TestCtrlZSuspends(t *testing.T) {
	m := seededModel(t)
	_, cmd := m.Update(ctrlKey('z'))
	if cmd == nil {
		t.Fatal("ctrl+z cmd = nil, want tea.Suspend")
	}
	if _, ok := cmd().(tea.SuspendMsg); !ok {
		t.Fatalf("ctrl+z message = %T, want tea.SuspendMsg", cmd())
	}
}

// TestCtrlZSuspendsUnderConnect keeps suspend reachable while the Connect
// sheet owns the other chords, matching Ctrl+Q.
func TestCtrlZSuspendsUnderConnect(t *testing.T) {
	m := resizeModel(t, seededModel(t), 100, 30)
	m.openConnect()
	_, cmd := m.Update(ctrlKey('z'))
	if cmd == nil {
		t.Fatal("ctrl+z under Connect cmd = nil, want tea.Suspend")
	}
	if _, ok := cmd().(tea.SuspendMsg); !ok {
		t.Fatalf("ctrl+z under Connect = %T, want tea.SuspendMsg", cmd())
	}
}
