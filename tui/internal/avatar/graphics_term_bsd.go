//go:build darwin || dragonfly || freebsd || netbsd || openbsd

package avatar

import "golang.org/x/sys/unix"

func enableNewlineMap(fd uintptr) error {
	termios, err := unix.IoctlGetTermios(int(fd), unix.TIOCGETA)
	if err != nil {
		return err
	}
	termios.Oflag |= unix.OPOST | unix.ONLCR
	return unix.IoctlSetTermios(int(fd), unix.TIOCSETA, termios)
}
