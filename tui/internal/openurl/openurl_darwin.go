//go:build darwin

package openurl

import "os/exec"

// openPlatform opens rawURL with the user's default handler via open. It does
// not wait for the handler: the process is detached and its stdout and stderr
// stay nil, so this never blocks the caller. A missing open binary is
// returned as an error rather than panicking.
func openPlatform(rawURL string) error {
	return exec.Command("open", rawURL).Start()
}
