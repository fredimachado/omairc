//go:build windows

package main

import tea "charm.land/bubbletea/v2"

// stopWatcher is a no-op on Windows: Bubble Tea leaves suspendSupported false,
// and the console has no SIGTSTP job-control path matching Unix fg.
type stopWatcher struct{}

type messageSender interface {
	Send(tea.Msg)
}

func newStopWatcher(_ <-chan struct{}) *stopWatcher {
	return &stopWatcher{}
}

func (w *stopWatcher) bind(_ messageSender) {}
func (w *stopWatcher) Arm()                 {}
func (w *stopWatcher) Disarm()              {}
func (w *stopWatcher) run()                 {}

func suspendFilter(_ *stopWatcher) func(tea.Model, tea.Msg) tea.Msg {
	return func(_ tea.Model, msg tea.Msg) tea.Msg { return msg }
}
