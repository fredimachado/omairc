//go:build linux

package avatar

import (
	"io"
	"testing"
	"time"

	"github.com/creack/pty"
	"golang.org/x/sys/unix"
)

func TestReplayInputKeepsNewlineMap(t *testing.T) {
	master, slave, err := pty.Open()
	if err != nil {
		t.Skip(err)
	}
	defer master.Close()
	defer slave.Close()
	go func() { _, _ = io.Copy(io.Discard, master) }()

	ok, input := ProbeGraphics(slave, slave, 40*time.Millisecond)
	if c, isCloser := input.(io.Closer); isCloser {
		defer c.Close()
	}
	if ok {
		t.Fatal("silent pty enabled graphics")
	}
	if input == nil {
		t.Fatal("timeout did not install a replay reader")
	}
	termios, err := unix.IoctlGetTermios(int(slave.Fd()), unix.TCGETS)
	if err != nil {
		t.Fatal(err)
	}
	if termios.Oflag&unix.OPOST == 0 || termios.Oflag&unix.ONLCR == 0 {
		t.Fatalf("oflag = %#x, want OPOST|ONLCR", termios.Oflag)
	}
	if termios.Lflag&unix.ICANON != 0 || termios.Lflag&unix.ECHO != 0 {
		t.Fatalf("lflag = %#x, want non-canonical and no echo", termios.Lflag)
	}
}
