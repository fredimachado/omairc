//go:build windows

package avatar

import (
	"os"

	"golang.org/x/sys/windows"
)

// copyProbeInput reads the terminal, drops a late probe reply, and writes the
// kept bytes to the pipe Bubble Tea reads. Windows pipes reject SetReadDeadline,
// so Close cancels the blocked Read via CancelIoEx when the wake pipe closes.
// That returns before another src byte is consumed.
func copyProbeInput(src, dst, wake *os.File, late *lateFilter) {
	defer dst.Close()
	defer wake.Close()
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
