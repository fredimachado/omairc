//go:build windows

package gate

import (
	"os"
	"os/exec"
)

// detachProcess is a no-op on Windows: there is no session to leave, and the
// gate only fully supports Unix PTYs (creack/pty returns ErrUnsupported).
func detachProcess(cmd *exec.Cmd) {}

// terminateProcess falls back to Kill on Windows, which has no SIGTERM.
func terminateProcess(p *os.Process) error {
	return p.Kill()
}
