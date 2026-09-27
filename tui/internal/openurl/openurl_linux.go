//go:build linux

package openurl

import "os/exec"

// openPlatform opens rawURL with the user's desktop handler via xdg-open. It
// does not wait for the handler: the process is detached and its stdout and
// stderr stay nil, so this never blocks the caller. A missing xdg-open is
// returned as an error rather than panicking.
func openPlatform(rawURL string) error {
	return exec.Command("xdg-open", rawURL).Start()
}
