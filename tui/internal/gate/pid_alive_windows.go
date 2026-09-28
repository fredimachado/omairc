//go:build windows

package gate

import (
	"golang.org/x/sys/windows"
)

// pidAlive reports whether pid names a live process the caller may query.
func pidAlive(pid int) bool {
	if pid <= 0 {
		return false
	}
	handle, err := windows.OpenProcess(windows.PROCESS_QUERY_LIMITED_INFORMATION, false, uint32(pid))
	if err != nil {
		return false
	}
	_ = windows.CloseHandle(handle)
	return true
}
