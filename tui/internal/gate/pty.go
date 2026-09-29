package gate

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"
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

// Process is a running child attached to a pseudo-terminal.
//
// readFd carries child output; writeFd delivers keystrokes. On Unix they are
// the same PTY master handle. On Windows they are separate ConPTY pipe ends.
type Process struct {
	readFd  *os.File
	writeFd *os.File
	cmd     *exec.Cmd

	mu       sync.Mutex
	onOutput func([]byte)

	exited  chan struct{}
	waitErr error

	cleanup func() error
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
	opts.Cols, opts.Rows = cols, rows
	return startPty(opts)
}

func (p *Process) readLoop() {
	buf := make([]byte, 32*1024)
	for {
		n, err := p.readFd.Read(buf)
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
	return p.writeFd.Write(b)
}

// Pid is the child pid, or 0 before start.
func (p *Process) Pid() int {
	if p.cmd == nil || p.cmd.Process == nil {
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
	if p.cmd == nil || p.cmd.Process == nil {
		return nil
	}
	return terminateProcess(p.cmd.Process)
}

// Kill forcibly stops the child.
func (p *Process) Kill() error {
	if p.cmd == nil || p.cmd.Process == nil {
		return nil
	}
	return p.cmd.Process.Kill()
}

// Close kills the child, closes the PTY handles, and waits briefly for the
// exit so no goroutine is left behind.
func (p *Process) Close() error {
	var firstErr error
	if p.cmd != nil && p.cmd.Process != nil {
		if err := p.cmd.Process.Kill(); err != nil && firstErr == nil {
			firstErr = err
		}
	}
	if err := p.closeFds(); err != nil && firstErr == nil {
		firstErr = err
	}
	if p.cleanup != nil {
		if err := p.cleanup(); err != nil && firstErr == nil {
			firstErr = err
		}
	}
	select {
	case <-p.exited:
	case <-time.After(3 * time.Second):
	}
	return firstErr
}

func (p *Process) closeFds() error {
	if p.readFd == nil && p.writeFd == nil {
		return nil
	}
	if p.readFd == p.writeFd {
		err := p.readFd.Close()
		p.readFd, p.writeFd = nil, nil
		return err
	}
	var firstErr error
	if p.readFd != nil {
		if err := p.readFd.Close(); err != nil {
			firstErr = err
		}
		p.readFd = nil
	}
	if p.writeFd != nil {
		if err := p.writeFd.Close(); err != nil && firstErr == nil {
			firstErr = err
		}
		p.writeFd = nil
	}
	return firstErr
}

// terminalEnv returns the process environment plus the terminal variables a
// full-screen TUI needs, with the app's XDG state rooted under stateDir so a
// launched shell never touches the developer's state directory (the crash log
// in particular). exec.Cmd deduplicates Env with the last value winning, so
// appending here overrides an inherited value.
//
// The inherited no-color hints are dropped rather than overridden. NO_COLOR and
// CLICOLOR are presence checks in the color libraries the shell renders
// through, not values, so a developer's or CI runner's hint cannot be beaten by
// appending a replacement. Letting one through stripped every SGR from the
// child and made the gate's PNG evidence mono, which cannot show the composer's
// surface fill, the nick colors, or the presence dots the screenshots exist to
// prove. TERM and COLORTERM are pinned below, so the child always drives a
// color-capable terminal.
func terminalEnv(stateDir string) []string {
	inherited := os.Environ()
	env := make([]string, 0, len(inherited)+4)
	for _, entry := range inherited {
		if envNames(entry, "NO_COLOR") || envNames(entry, "CLICOLOR") {
			continue
		}
		env = append(env, entry)
	}
	return append(env,
		"TERM=xterm-256color",
		"COLORTERM=truecolor",
		"LANG=C.UTF-8",
		"XDG_STATE_HOME="+filepath.Join(stateDir, "app-state"),
	)
}

// envNames reports whether an "NAME=value" environment entry names name.
// Windows environment names are case-insensitive, so the comparison folds case.
// CLICOLOR_FORCE is a different name and is deliberately left alone.
func envNames(entry, name string) bool {
	if len(entry) <= len(name) {
		return false
	}
	return strings.EqualFold(entry[:len(name)], name) && entry[len(name)] == '='
}
