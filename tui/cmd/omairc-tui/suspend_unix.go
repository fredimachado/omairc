//go:build unix

package main

import (
	"os"
	"os/signal"
	"sync"
	"syscall"

	tea "charm.land/bubbletea/v2"
)

// messageSender is the *tea.Program.Send surface the stop watcher needs.
type messageSender interface {
	Send(tea.Msg)
}

// stopWatcher catches external SIGTSTP on the normal input path (Bubble Tea
// owns raw mode on os.Stdin). It hands the stop to tea.Suspend so release,
// stop, restore, and redraw stay in one place. Skipped when the avatar probe
// already holds the tty for tea.WithInput(replay).
type stopWatcher struct {
	mu    sync.Mutex
	stop  chan os.Signal
	armed bool
	done  <-chan struct{}
	p     messageSender
}

func newStopWatcher(done <-chan struct{}) *stopWatcher {
	return &stopWatcher{
		stop: make(chan os.Signal, 1),
		done: done,
	}
}

func (w *stopWatcher) bind(p messageSender) {
	if w == nil {
		return
	}
	w.p = p
}

// Arm starts listening for SIGTSTP. Safe to call more than once.
func (w *stopWatcher) Arm() {
	if w == nil {
		return
	}
	w.mu.Lock()
	defer w.mu.Unlock()
	if w.armed {
		return
	}
	signal.Notify(w.stop, syscall.SIGTSTP)
	w.armed = true
}

// Disarm stops listening so tea.Suspend's own SIGTSTP can take the default
// stop action instead of being swallowed by this Notify.
func (w *stopWatcher) Disarm() {
	if w == nil {
		return
	}
	w.mu.Lock()
	defer w.mu.Unlock()
	if !w.armed {
		return
	}
	signal.Stop(w.stop)
	for {
		select {
		case <-w.stop:
		default:
			w.armed = false
			return
		}
	}
}

func (w *stopWatcher) run() {
	if w == nil {
		return
	}
	w.Arm()
	for {
		select {
		case <-w.done:
			w.Disarm()
			return
		case <-w.stop:
			// Bubble Tea must see Suspend with our Notify cleared, or Kill
			// never stops the process. ResumeMsg re-arms through the filter.
			w.Disarm()
			if w.p != nil {
				w.p.Send(tea.Suspend())
			}
		}
	}
}

// suspendFilter disarms before Suspend runs and re-arms after ResumeMsg so
// Ctrl+Z (tea.Suspend from Update) and external SIGTSTP share one handler
// without double-registering SIGTSTP.
func suspendFilter(w *stopWatcher) func(tea.Model, tea.Msg) tea.Msg {
	return func(_ tea.Model, msg tea.Msg) tea.Msg {
		switch msg.(type) {
		case tea.SuspendMsg:
			w.Disarm()
		case tea.ResumeMsg:
			w.Arm()
		}
		return msg
	}
}
