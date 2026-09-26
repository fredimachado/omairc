package gate

import (
	"fmt"
	"os"
	"os/exec"
	"sync"
	"time"

	"github.com/creack/pty"
)

// Options describes one PTY child.
type Options struct {
	Binary string
	Args   []string
	Cols   int
	Rows   int
	Dir    string
	Env    []string
}

// Process is a running child attached to a PTY master.
//
// The master is the only handle on the child's terminal: reading it yields
// everything the child wrote (including OSC titles), writing it delivers
// keystrokes. Start spins one goroutine that reads the master and hands each
// chunk to the output callback, so the daemon can feed a Screen while the
// client is not connected.
type Process struct {
	master *os.File
	cmd    *exec.Cmd

	mu       sync.Mutex
	onOutput func([]byte)

	exited  chan struct{}
	waitErr error
}

// Start launches opts.Binary on a fresh PTY of the requested size.
func Start(opts Options) (*Process, error) {
	if opts.Binary == "" {
		return nil, fmt.Errorf("no binary given")
	}
	cols, rows := opts.Cols, opts.Rows
	if cols <= 0 {
		cols = 118
	}
	if rows <= 0 {
		rows = 30
	}

	cmd := exec.Command(opts.Binary, opts.Args...)
	cmd.Dir = opts.Dir
	if opts.Env != nil {
		cmd.Env = opts.Env
	}

	// StartWithSize starts the child in a new session with the PTY as its
	// controlling terminal, so the child sees a real TTY and a SIGWINCH-less
	// fixed size. It exists on Windows too (returning ErrUnsupported), which
	// keeps this file cross-compiling.
	master, err := pty.StartWithSize(cmd, &pty.Winsize{Rows: uint16(rows), Cols: uint16(cols)})
	if err != nil {
		return nil, fmt.Errorf("start %s on a pty: %w", opts.Binary, err)
	}

	p := &Process{master: master, cmd: cmd, exited: make(chan struct{})}
	go func() {
		p.waitErr = cmd.Wait()
		close(p.exited)
	}()
	go p.readLoop()
	return p, nil
}

func (p *Process) readLoop() {
	buf := make([]byte, 32*1024)
	for {
		n, err := p.master.Read(buf)
		if n > 0 {
			chunk := make([]byte, n)
			copy(chunk, buf[:n])
			p.mu.Lock()
			cb := p.onOutput
			p.mu.Unlock()
			if cb != nil {
				cb(chunk)
			}
		}
		if err != nil {
			return
		}
	}
}

// OnOutput registers the sink for child output. It may be set before or after
// output starts; chunks already read are not replayed.
func (p *Process) OnOutput(cb func([]byte)) {
	p.mu.Lock()
	p.onOutput = cb
	p.mu.Unlock()
}

// Write delivers key bytes to the child.
func (p *Process) Write(b []byte) (int, error) {
	return p.master.Write(b)
}

// Pid is the child pid, or 0 before start.
func (p *Process) Pid() int {
	if p.cmd.Process == nil {
		return 0
	}
	return p.cmd.Process.Pid
}

// Alive reports whether the child is still running.
func (p *Process) Alive() bool {
	select {
	case <-p.exited:
		return false
	default:
		return true
	}
}

// Wait blocks until the child exits and returns its wait error.
func (p *Process) Wait() error {
	<-p.exited
	return p.waitErr
}

// Terminate asks the child to stop (SIGTERM on Unix, Kill on Windows).
func (p *Process) Terminate() error {
	if p.cmd.Process == nil {
		return nil
	}
	return terminateProcess(p.cmd.Process)
}

// Kill forcibly stops the child.
func (p *Process) Kill() error {
	if p.cmd.Process == nil {
		return nil
	}
	return p.cmd.Process.Kill()
}

// Close kills the child, closes the master, and waits briefly for the exit so
// no goroutine is left behind.
func (p *Process) Close() error {
	var firstErr error
	if p.cmd.Process != nil {
		if err := p.cmd.Process.Kill(); err != nil && firstErr == nil {
			firstErr = err
		}
	}
	if err := p.master.Close(); err != nil && firstErr == nil {
		firstErr = err
	}
	select {
	case <-p.exited:
	case <-time.After(3 * time.Second):
	}
	return firstErr
}

// terminalEnv returns the process environment plus the terminal variables a
// full-screen TUI needs. exec.Cmd deduplicates Env with the last value
// winning, so appending here overrides an inherited value.
func terminalEnv() []string {
	env := os.Environ()
	return append(env,
		"TERM=xterm-256color",
		"COLORTERM=truecolor",
		"LANG=C.UTF-8",
	)
}
