//go:build windows

package openurl

import (
	"os/exec"
	"syscall"
)

// openPlatform opens rawURL with the user's default handler via cmd /c start.
// The process is detached and its stdout/stderr stay nil, so this never blocks
// the caller.
func openPlatform(rawURL string) error {
	cmd := exec.Command("cmd", "/c", "start", "", rawURL)
	cmd.SysProcAttr = &syscall.SysProcAttr{HideWindow: true}
	return cmd.Start()
}
