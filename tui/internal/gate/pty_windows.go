//go:build windows

package gate

import (
	"fmt"
	"os/exec"

	gopty "github.com/aymanbagabas/go-pty"
)

func startPty(opts Options) (*Process, error) {
	pty, err := gopty.New()
	if err != nil {
		return nil, fmt.Errorf("create conpty: %w", err)
	}
	conpty, ok := pty.(gopty.ConPty)
	if !ok {
		_ = pty.Close()
		return nil, fmt.Errorf("create conpty: unexpected pty type %T", pty)
	}
	if err := pty.Resize(opts.Cols, opts.Rows); err != nil {
		_ = pty.Close()
		return nil, fmt.Errorf("resize conpty: %w", err)
	}

	cmd := pty.Command(opts.Binary, opts.Args...)
	cmd.Dir = opts.Dir
	cmd.Env = opts.Env
	if err := cmd.Start(); err != nil {
		_ = pty.Close()
		return nil, fmt.Errorf("start %s on a conpty: %w", opts.Binary, err)
	}

	p := &Process{
		readFd:  conpty.OutputPipe(),
		writeFd: conpty.InputPipe(),
		cmd:     &exec.Cmd{Process: cmd.Process},
		exited:  make(chan struct{}),
		cleanup: func() error {
			return pty.Close()
		},
	}
	go func() {
		p.waitErr = cmd.Wait()
		close(p.exited)
	}()
	go p.readLoop()
	return p, nil
}
