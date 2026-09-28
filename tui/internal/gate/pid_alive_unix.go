//go:build !windows

package gate

import (
	"fmt"
	"os"
	"strings"
	"syscall"
)

// pidAlive reports whether pid names a live process the caller may signal. It
// returns false for a non-positive pid. On Linux it reads /proc/<pid>/stat and
// treats a zombie as dead: a child this process spawned and released lingers as
// a zombie, and a signal-0 probe reports that zombie as alive, which would make
// a teardown wait spin until the parent exits. Elsewhere it falls back to the
// signal-0 existence probe.
func pidAlive(pid int) bool {
	if pid <= 0 {
		return false
	}
	if _, err := os.Stat("/proc"); err == nil {
		data, err := os.ReadFile(fmt.Sprintf("/proc/%d/stat", pid))
		if err != nil {
			return false
		}
		// "pid (comm) state ...": comm may contain spaces and parens, so read
		// the state field after the last ')'.
		if i := strings.LastIndexByte(string(data), ')'); i >= 0 && i+2 < len(data) {
			return data[i+2] != 'Z'
		}
		return false
	}
	p, err := os.FindProcess(pid)
	if err != nil {
		return false
	}
	return p.Signal(syscall.Signal(0)) == nil
}
