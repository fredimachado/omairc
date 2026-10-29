//go:build windows

package avatar

import (
	"os"
	"sync"
	"syscall"
	"time"
	"unsafe"

	"golang.org/x/sys/windows"
)

var (
	modkernel32       = windows.NewLazySystemDLL("kernel32.dll")
	procPeekNamedPipe = modkernel32.NewProc("PeekNamedPipe")
)

func peekNamedPipeAvail(h windows.Handle) (uint32, error) {
	var avail uint32
	r1, _, e := syscall.SyscallN(procPeekNamedPipe.Addr(),
		uintptr(h),
		0,
		0,
		0,
		uintptr(unsafe.Pointer(&avail)),
		0,
	)
	if r1 == 0 {
		if e != 0 {
			return 0, e
		}
		return 0, syscall.EINVAL
	}
	return avail, nil
}

// waitCopyStop waits for the copier, but bounds the wait. Console CancelIoEx is
// best-effort; a hung Read must not block process teardown.
func waitCopyStop(wg *sync.WaitGroup) {
	done := make(chan struct{})
	go func() {
		wg.Wait()
		close(done)
	}()
	select {
	case <-done:
	case <-time.After(250 * time.Millisecond):
	}
}

// copyProbeInput reads the terminal, drops a late probe reply, and writes the
// kept bytes to the pipe Bubble Tea reads. Windows anonymous pipes reject
// SetReadDeadline and do not wake WaitForMultipleObjects once Go owns them, so
// this path peeks for input and watches wake with a normal Read. Closing the
// wake write end returns without a src.Read, so a post-Close keystroke stays
// on src.
func copyProbeInput(src, dst, wake *os.File, late *lateFilter) {
	defer dst.Close()
	defer wake.Close()

	srcH := windows.Handle(src.Fd())
	if _, err := peekNamedPipeAvail(srcH); err != nil {
		copyProbeInputConsole(src, dst, wake, late)
		return
	}

	stop := make(chan struct{})
	go func() {
		var b [1]byte
		_, _ = wake.Read(b[:])
		close(stop)
	}()

	tmp := make([]byte, 256)
	for {
		select {
		case <-stop:
			return
		default:
		}
		avail, err := peekNamedPipeAvail(srcH)
		if err != nil {
			return
		}
		if avail > 0 {
			select {
			case <-stop:
				return
			default:
			}
			var n uint32
			err := windows.ReadFile(srcH, tmp, &n, nil)
			if n > 0 {
				if werr := filterCopyChunk(dst, late, tmp[:n]); werr != nil {
					return
				}
			}
			if err != nil {
				return
			}
			continue
		}
		select {
		case <-stop:
			return
		case <-time.After(20 * time.Millisecond):
		}
	}
}

// copyProbeInputConsole is the non-pipe stdin path. CancelIoEx is best-effort;
// Close still closes wake so a later Read can observe stop.
func copyProbeInputConsole(src, dst, wake *os.File, late *lateFilter) {
	stop := make(chan struct{})
	go func() {
		var b [1]byte
		_, _ = wake.Read(b[:])
		_ = windows.CancelIoEx(windows.Handle(src.Fd()), nil)
		close(stop)
	}()
	tmp := make([]byte, 256)
	for {
		select {
		case <-stop:
			return
		default:
		}
		n, err := src.Read(tmp)
		select {
		case <-stop:
			return
		default:
		}
		if n > 0 {
			if werr := filterCopyChunk(dst, late, tmp[:n]); werr != nil {
				return
			}
		}
		if err != nil {
			return
		}
	}
}
