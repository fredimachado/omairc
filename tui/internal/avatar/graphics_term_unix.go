//go:build unix

package avatar

import (
	"os"
	"os/signal"
	"runtime"
	"syscall"

	"github.com/charmbracelet/x/term"
)

// holdTerminal keeps the real terminal in non-canonical mode with ONLCR.
// Bubble Tea sees the replay pipe, not this tty, and therefore maps newlines
// as if output processing were on. An external SIGTSTP is caught here so the
// cooked state is restored and the process actually stops; continue puts the
// probe mode back and asks the program to redraw.
func (b *bufferedProbeInput) holdTerminal(fd uintptr, state *term.State) {
	_ = enableNewlineMap(fd)
	b.mu.Lock()
	b.fd = fd
	b.orig = state
	b.done = make(chan struct{})
	b.watching = make(chan struct{})
	b.mu.Unlock()
	go b.watchStop()
}

func (b *bufferedProbeInput) watchStop() {
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	stop := make(chan os.Signal, 1)
	signal.Notify(stop, syscall.SIGTSTP)
	close(b.watching)
	defer signal.Stop(stop)
	for {
		select {
		case <-b.done:
			return
		case <-stop:
		}
		if b.terminalStopped() {
			return
		}
		// Cooked mode is only for the stopped process. stopProcess does
		// not return until SIGCONT. Re-arm Notify before reapply so a
		// second SIGTSTP in that gap cannot stop the process while the
		// tty is still raw.
		b.releaseForStop()
		raiseDefaultStop()
		if b.terminalStopped() {
			return
		}
		signal.Notify(stop, syscall.SIGTSTP)
		b.reapply()
		requestRedraw()
	}
}

func (b *bufferedProbeInput) terminalStopped() bool {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.stopped
}

// raiseDefaultStop restores the kernel's stop action and re-raises SIGTSTP.
// signal.Reset leaves Go's handler installed for this signal, and that
// handler swallows it, so the process would stay runnable with a cooked tty.
// Ignore clears that handler; the platform install puts SIG_DFL back.
func raiseDefaultStop() {
	signal.Ignore(syscall.SIGTSTP)
	if installSIGTSTPDefault() != nil || unblockSIGTSTP() != nil {
		return
	}
	// Locked to one OS thread above, so the unblocked mask and the
	// self-directed stop stay on the same thread. A group-directed kill
	// can be taken by the signal thread after raw mode is already back.
	_ = stopProcess()
}

func requestRedraw() {
	_ = syscall.Kill(syscall.Getpid(), syscall.SIGWINCH)
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
