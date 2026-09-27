//go:build !linux

package theme

import (
	"sync"
	"time"

	"github.com/fredimachado/omairc/tui/internal/session"
)

// Watcher is a no-op off Linux: the Omarchy theme tree only exists on Omarchy.
// It exposes the same API as the Linux watcher so the shell wires it unchanged,
// and it never emits.
type Watcher struct {
	mu     sync.Mutex
	closed bool
	ch     chan Change
}

// NewWatcher returns a watcher that never fires off Linux. The clock and
// interval are accepted for API parity with the Linux constructor.
func NewWatcher(session.Clock, time.Duration) *Watcher {
	return &Watcher{ch: make(chan Change)}
}

// Changes returns the never-written channel that Close closes.
func (w *Watcher) Changes() <-chan Change {
	return w.ch
}

// Current returns the fixed fallback palette.
func (w *Watcher) Current() Colors {
	return Fallback()
}

// Close closes Changes. It is safe to call more than once.
func (w *Watcher) Close() {
	w.mu.Lock()
	defer w.mu.Unlock()
	if w.closed {
		return
	}
	w.closed = true
	close(w.ch)
}
