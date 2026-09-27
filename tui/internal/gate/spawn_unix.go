//go:build !windows

package gate

import (
	"os"
	"os/exec"
	"syscall"
)

// detachProcess puts a spawned helper in its own session so it survives the
// CLI invocation that started it. This is how `launch` starts the daemon.
func detachProcess(cmd *exec.Cmd) {
	if cmd.SysProcAttr == nil {
		cmd.SysProcAttr = &syscall.SysProcAttr{}
	}
	cmd.SysProcAttr.Setsid = true
}

// terminateProcess asks a process to stop gracefully.
func terminateProcess(p *os.Process) error {
	return p.Signal(syscall.SIGTERM)
}
