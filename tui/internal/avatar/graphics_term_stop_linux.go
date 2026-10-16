//go:build linux || android

package avatar

import (
	"os"
	"runtime"
	"unsafe"

	"golang.org/x/sys/unix"
)

// installSIGTSTPDefault replaces Go's handler with the kernel default so the
// process stops. The sigaction layout follows the runtime's per-arch struct.
func installSIGTSTPDefault() error {
	switch runtime.GOARCH {
	case "386", "arm":
		var sa struct {
			handler  uintptr
			flags    uint32
			restorer uintptr
			mask     uint64
		}
		return rtSigaction(unsafe.Pointer(&sa), 8)
	case "riscv64":
		var sa struct {
			handler  uintptr
			flags    uint64
			mask     uint64
			restorer uintptr
		}
		return rtSigaction(unsafe.Pointer(&sa), 8)
	case "mips", "mipsle", "mips64", "mips64le":
		var sa struct {
			flags    uint32
			handler  uintptr
			mask     [2]uint64
			restorer uintptr
		}
		return rtSigaction(unsafe.Pointer(&sa), 16)
	default:
		var sa struct {
			handler  uintptr
			flags    uint64
			restorer uintptr
			mask     uint64
		}
		return rtSigaction(unsafe.Pointer(&sa), 8)
	}
}

func rtSigaction(sa unsafe.Pointer, maskBytes uintptr) error {
	_, _, errno := unix.RawSyscall6(
		unix.SYS_RT_SIGACTION,
		uintptr(unix.SIGTSTP),
		uintptr(sa),
		0,
		maskBytes,
		0, 0,
	)
	if errno != 0 {
		return errno
	}
	return nil
}

// stopProcess sends SIGTSTP to this thread. With SIG_DFL and the signal
// unblocked, the call does not return until the process is continued.
func stopProcess() error {
	return unix.Tgkill(os.Getpid(), unix.Gettid(), unix.SIGTSTP)
}

func unblockSIGTSTP() error {
	var set unix.Sigset_t
	bit := uint(unix.SIGTSTP) - 1
	set.Val[bit/64] |= 1 << (bit % 64)
	return unix.PthreadSigmask(unix.SIG_UNBLOCK, &set, nil)
}
