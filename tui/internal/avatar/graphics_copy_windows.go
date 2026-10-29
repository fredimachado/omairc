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

func waitCopyStop(wg *sync.WaitGroup) {
	wg.Wait()
}

// copyProbeInput reads the terminal, drops a late probe reply, and writes the
// kept bytes to the pipe Bubble Tea reads. Windows anonymous pipes reject
// SetReadDeadline and do not wake WaitForMultipleObjects once Go owns them, so
// this path peeks for input and watches wake with a normal Read. Closing the
// wake write end returns without a src.Read, so a post-Close keystroke stays
// on src. Production stdin after term.IsTerminal is a console handle; that
// path uses wait-for-ready instead of a naked blocking Read.
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

// copyProbeInputConsole is the production Windows stdin path. Console handles
// are waitable, so Close signals a stop event and WaitForMultipleObjects
// returns before any ReadFile. CancelIoEx is not required.
func copyProbeInputConsole(src, dst, wake *os.File, late *lateFilter) {
	stopEvent, err := windows.CreateEvent(nil, 1, 0, nil)
	if err != nil {
		return
	}
	defer windows.CloseHandle(stopEvent)

	go func() {
		var b [1]byte
		_, _ = wake.Read(b[:])
		_ = windows.SetEvent(stopEvent)
	}()

	srcH := windows.Handle(src.Fd())
	tmp := make([]byte, 256)
	handles := []windows.Handle{srcH, stopEvent}
	for {
		ev, err := windows.WaitForMultipleObjects(handles, false, windows.INFINITE)
		if err != nil {
			return
		}
		if ev != windows.WAIT_OBJECT_0 {
			return
		}
		// Close wins over a racing key so the post-Close stroke stays on src.
		if st, err := windows.WaitForSingleObject(stopEvent, 0); err == nil && st == windows.WAIT_OBJECT_0 {
			return
		}
		var n uint32
		err = windows.ReadFile(srcH, tmp, &n, nil)
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
