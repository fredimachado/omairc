//go:build unix

package main

import (
	"syscall"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
)

type fakeSender struct {
	msgs chan tea.Msg
}

func (f fakeSender) Send(msg tea.Msg) {
	f.msgs <- msg
}

// TestSuspendFilterDisarmsAroundSuspend proves Ctrl+Z's tea.Suspend path and
// the external SIGTSTP watcher do not leave SIGTSTP Notify armed while
// Bubble Tea sends its own stop signal.
func TestSuspendFilterDisarmsAroundSuspend(t *testing.T) {
	done := make(chan struct{})
	defer close(done)
	w := newStopWatcher(done)
	w.Arm()
	if !w.armed {
		t.Fatal("Arm left watcher unarmed")
	}
	filter := suspendFilter(w)
	if msg := filter(nil, tea.SuspendMsg{}); msg == nil {
		t.Fatal("filter dropped SuspendMsg")
	}
	if w.armed {
		t.Fatal("SuspendMsg must Disarm so Kill can stop the process")
	}
	if msg := filter(nil, tea.ResumeMsg{}); msg == nil {
		t.Fatal("filter dropped ResumeMsg")
	}
	if !w.armed {
		t.Fatal("ResumeMsg must Arm for the next external SIGTSTP")
	}
}

// TestStopWatcherRunSendsSuspend proves an external SIGTSTP is turned into
// tea.Suspend once a sender is bound.
func TestStopWatcherRunSendsSuspend(t *testing.T) {
	done := make(chan struct{})
	w := newStopWatcher(done)
	sender := fakeSender{msgs: make(chan tea.Msg, 1)}
	w.bind(sender)
	go w.run()
	defer close(done)

	w.stop <- syscall.SIGTSTP
	select {
	case msg := <-sender.msgs:
		if _, ok := msg.(tea.SuspendMsg); !ok {
			t.Fatalf("Send(%T), want tea.SuspendMsg", msg)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("watcher did not Send SuspendMsg after SIGTSTP")
	}
}
