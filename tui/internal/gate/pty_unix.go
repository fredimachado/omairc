//go:build !windows

package gate

import (
	"fmt"
	"os/exec"

	"github.com/creack/pty"
)

func startPty(opts Options) (*Process, error) {
	cmd := exec.Command(opts.Binary, opts.Args...)
	cmd.Dir = opts.Dir
	if opts.Env != nil {
		cmd.Env = opts.Env
	}

	master, err := pty.StartWithSize(cmd, &pty.Winsize{
		Rows: uint16(opts.Rows),
		Cols: uint16(opts.Cols),
	})
	if err != nil {
		return nil, fmt.Errorf("start %s on a pty: %w", opts.Binary, err)
	}

	p := &Process{
		readFd:  master,
		writeFd: master,
		cmd:     cmd,
		exited:  make(chan struct{}),
	}
	go func() {
		p.waitErr = cmd.Wait()
		close(p.exited)
	}()
	go p.readLoop()
	return p, nil
}
