//go:build darwin || ios

package avatar

import (
	"syscall"
	"unsafe"

	"golang.org/x/sys/unix"
)

// installSIGTSTPDefault replaces the Go signal handler with SIG_DFL so
// the following SIGTSTP stops the process. The zero handler word is
// SIG_DFL and matches the Darwin sigaction layout. Other BSD kernels
// use a different struct, so they take the stub and reapply raw mode
// without stopping.
func installSIGTSTPDefault() error {
	var sa struct {
		handler [8]byte
		tramp   uintptr
		mask    uint32
		flags   int32
	}
	_, _, errno := unix.RawSyscall(unix.SYS_SIGACTION, uintptr(unix.SIGTSTP), uintptr(unsafe.Pointer(&sa)), 0)
	if errno != 0 {
		return errno
	}
	return nil
}

func unblockSIGTSTP() error {
	return nil
}

func stopProcess() error {
	return syscall.Kill(syscall.Getpid(), syscall.SIGTSTP)
}
