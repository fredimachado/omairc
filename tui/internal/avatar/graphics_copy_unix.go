//go:build unix

package avatar

import (
	"os"
	"sync"

	"golang.org/x/sys/unix"
)

// copyProbeInput reads the terminal, drops a late probe reply, and writes the
// kept bytes to the pipe Bubble Tea reads. wake is the read end of a pipe
// closed by Close; poll returns before any src.Read once that happens, so a
// post-Close keystroke stays on src.
func copyProbeInput(src, dst, wake *os.File, late *lateFilter) {
	defer dst.Close()
	defer wake.Close()
	tmp := make([]byte, 256)
	for {
		srcReady, err := waitCopyReady(src, wake)
		if err != nil || !srcReady {
			return
		}
		n, err := src.Read(tmp)
		if n > 0 {
			if err := filterCopyChunk(dst, late, tmp[:n]); err != nil {
				return
			}
		}
		if err != nil {
			return
		}
	}
}

func waitCopyStop(wg *sync.WaitGroup) {
	wg.Wait()
}

func waitCopyReady(src, wake *os.File) (srcReady bool, err error) {
	fds := []unix.PollFd{
		{Fd: int32(src.Fd()), Events: unix.POLLIN},
		{Fd: int32(wake.Fd()), Events: unix.POLLIN},
	}
	for {
		n, err := unix.Poll(fds, -1)
		if err == unix.EINTR {
			continue
		}
		if err != nil {
			return false, err
		}
		if n == 0 {
			continue
		}
		if fds[0].Revents&unix.POLLNVAL != 0 || fds[1].Revents&unix.POLLNVAL != 0 {
			return false, unix.EINVAL
		}
		if fds[1].Revents&(unix.POLLIN|unix.POLLHUP|unix.POLLERR) != 0 {
			return false, nil
		}
		if fds[0].Revents&(unix.POLLIN|unix.POLLHUP|unix.POLLERR) != 0 {
			return true, nil
		}
	}
}
