//go:build aix || android || linux || solaris || zos

package avatar

import "golang.org/x/sys/unix"

func enableNewlineMap(fd uintptr) error {
	termios, err := unix.IoctlGetTermios(int(fd), unix.TCGETS)
	if err != nil {
		return err
	}
	termios.Oflag |= unix.OPOST | unix.ONLCR
	return unix.IoctlSetTermios(int(fd), unix.TCSETS, termios)
}
