//go:build unix

package avatar

import (
	"io"
	"os"
	"time"

	"golang.org/x/sys/unix"
)

// readProbeReply writes the graphics query and waits for DA1 or the timeout.
// A PTY rejects SetReadDeadline, so the wait is a poll on the slave
// descriptor. The read runs only after the descriptor is readable, which
// keeps a missing reply from blocking the rest of startup.
func readProbeReply(w io.Writer, in *os.File, timeout time.Duration) (bool, io.Reader) {
	if w == nil || in == nil || timeout <= 0 {
		return false, nil
	}
	deadline := time.Now().Add(timeout)
	if _, err := io.WriteString(w, probeQueries()); err != nil {
		return false, nil
	}
	var scan probeScan
	tmp := make([]byte, 256)
	fd := int(in.Fd())
	for {
		ready, err := pollReadable(fd, deadline)
		if err != nil || !ready {
			res := scan.finish(true)
			return res.supported, res.wrap(in)
		}
		n, err := in.Read(tmp)
		if n > 0 && scan.feed(tmp[:n]) {
			res := scan.finish(false)
			return res.supported, res.wrap(in)
		}
		if err != nil || !time.Now().Before(deadline) {
			res := scan.finish(true)
			return res.supported, res.wrap(in)
		}
	}
}

// pollReadable waits until fd can be read or deadline passes. EINTR retries
// with the time that is left.
func pollReadable(fd int, deadline time.Time) (bool, error) {
	for {
		remain := time.Until(deadline)
		if remain <= 0 {
			return false, nil
		}
		ms := int(remain / time.Millisecond)
		if ms < 1 {
			ms = 1
		}
		fds := []unix.PollFd{{
			Fd:     int32(fd),
			Events: unix.POLLIN,
		}}
		n, err := unix.Poll(fds, ms)
		if err == unix.EINTR {
			continue
		}
		if err != nil {
			return false, err
		}
		if n == 0 {
			return false, nil
		}
		if fds[0].Revents&unix.POLLNVAL != 0 {
			return false, unix.EINVAL
		}
		return true, nil
	}
}
