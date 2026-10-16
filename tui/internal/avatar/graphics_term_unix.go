//go:build unix

package avatar

import (
	"os"
	"os/signal"
	"syscall"

	"github.com/charmbracelet/x/term"
)

// holdTerminal keeps the real terminal in non-canonical mode with ONLCR.
// Bubble Tea sees the replay pipe, not this tty, and therefore maps newlines
// as if output processing were on. Ctrl+Z arrives as a key, and Bubble Tea
// stops the process without restoring a tty it does not own; the watcher
// restores the cooked state for the stop and puts this mode back on continue.
func (b *bufferedProbeInput) holdTerminal(fd uintptr, state *term.State) {
	_ = enableNewlineMap(fd)
	b.mu.Lock()
	b.fd = fd
	b.orig = state
	b.done = make(chan struct{})
	b.mu.Unlock()
	go b.watchStop()
}

func (b *bufferedProbeInput) watchStop() {
	stop := make(chan os.Signal, 1)
	signal.Notify(stop, syscall.SIGTSTP)
	defer signal.Stop(stop)
	for {
		select {
		case <-b.done:
			return
		case <-stop:
		}
		b.mu.Lock()
		stopped := b.stopped
		b.mu.Unlock()
		if stopped {
			return
		}
		b.releaseForStop()
		cont := make(chan os.Signal, 1)
		signal.Notify(cont, syscall.SIGCONT)
		signal.Stop(stop)
		_ = syscall.Kill(0, syscall.SIGTSTP)
		select {
		case <-b.done:
			signal.Stop(cont)
			return
		case <-cont:
		}
		signal.Stop(cont)
		b.reapply()
		signal.Notify(stop, syscall.SIGTSTP)
	}
}

func (b *bufferedProbeInput) releaseForStop() {
	b.mu.Lock()
	defer b.mu.Unlock()
	if b.orig != nil {
		_ = term.Restore(b.fd, b.orig)
	}
}

func (b *bufferedProbeInput) reapply() {
	b.mu.Lock()
	defer b.mu.Unlock()
	if b.stopped || b.orig == nil {
		return
	}
	_, _ = term.MakeRaw(b.fd)
	_ = enableNewlineMap(b.fd)
}
