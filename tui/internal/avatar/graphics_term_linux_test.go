//go:build linux

package avatar

import (
	"bytes"
	"fmt"
	"io"
	"os"
	"os/exec"
	"runtime"
	"strings"
	"syscall"
	"testing"
	"time"

	"github.com/charmbracelet/x/term"
	"github.com/creack/pty"
	"golang.org/x/sys/unix"
)

func TestMain(m *testing.M) {
	if os.Getenv("OMAIRC_TUI_STOP_HELPER") == "1" {
		runStopHelper()
		os.Exit(0)
	}
	os.Exit(m.Run())
}

// runStopHelper is a child process. It holds a pty the parent kept open,
// signals readiness only after SIGTSTP is being watched, then waits to be
// stopped.
func runStopHelper() {
	slave := os.NewFile(3, "slave")
	defer runtime.KeepAlive(slave)
	state, err := term.MakeRaw(slave.Fd())
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	held := &bufferedProbeInput{}
	held.holdTerminal(slave.Fd(), state)
	<-held.watching
	fmt.Println("ready")
	select {}
}

func TestExternalSIGTSTPStopsAndRestores(t *testing.T) {
	master, slave, err := pty.Open()
	if err != nil {
		t.Skip(err)
	}
	defer master.Close()
	defer slave.Close()
	go func() { _, _ = io.Copy(io.Discard, master) }()

	before, err := unix.IoctlGetTermios(int(slave.Fd()), unix.TCGETS)
	if err != nil {
		t.Fatal(err)
	}
	if before.Lflag&unix.ICANON == 0 || before.Lflag&unix.ECHO == 0 {
		t.Fatalf("pty lflag = %#x, want a cooked terminal", before.Lflag)
	}

	cmd := exec.Command(os.Args[0])
	cmd.Env = append(os.Environ(), "OMAIRC_TUI_STOP_HELPER=1")
	cmd.ExtraFiles = []*os.File{slave}
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	defer func() {
		_ = syscall.Kill(cmd.Process.Pid, syscall.SIGCONT)
		_ = cmd.Process.Kill()
		_, _ = cmd.Process.Wait()
	}()

	ready := make(chan string, 1)
	go func() {
		buf := make([]byte, 32)
		n, readErr := stdout.Read(buf)
		if readErr != nil && n == 0 {
			ready <- readErr.Error()
			return
		}
		ready <- string(buf[:n])
	}()
	select {
	case got := <-ready:
		if !strings.Contains(got, "ready") {
			t.Fatalf("helper said %q stderr %s", got, stderr.String())
		}
	case <-time.After(3 * time.Second):
		t.Fatalf("helper did not arm SIGTSTP, stderr %s", stderr.String())
	}

	if err := syscall.Kill(cmd.Process.Pid, syscall.SIGTSTP); err != nil {
		t.Fatal(err)
	}
	stopped := make(chan syscall.WaitStatus, 1)
	waitErr := make(chan error, 1)
	go func() {
		var ws syscall.WaitStatus
		_, err := syscall.Wait4(cmd.Process.Pid, &ws, syscall.WUNTRACED, nil)
		if err != nil {
			waitErr <- err
			return
		}
		stopped <- ws
	}()
	select {
	case err := <-waitErr:
		t.Fatal(err)
	case ws := <-stopped:
		if !ws.Stopped() || ws.StopSignal() != syscall.SIGTSTP {
			t.Fatalf("wait status %#x, want SIGTSTP stop", uint32(ws))
		}
	case <-time.After(3 * time.Second):
		t.Fatal("process stayed running after SIGTSTP")
	}

	cooked, err := unix.IoctlGetTermios(int(slave.Fd()), unix.TCGETS)
	if err != nil {
		t.Fatal(err)
	}
	if cooked.Lflag&unix.ICANON == 0 || cooked.Lflag&unix.ECHO == 0 {
		t.Fatalf("stopped lflag = %#x, want cooked mode restored", cooked.Lflag)
	}

	if err := syscall.Kill(cmd.Process.Pid, syscall.SIGCONT); err != nil {
		t.Fatal(err)
	}
	deadline := time.Now().Add(3 * time.Second)
	var restored *unix.Termios
	for {
		restored, err = unix.IoctlGetTermios(int(slave.Fd()), unix.TCGETS)
		if err != nil {
			t.Fatal(err)
		}
		raw := restored.Lflag&unix.ICANON == 0 && restored.Lflag&unix.ECHO == 0
		mapped := restored.Oflag&unix.OPOST != 0 && restored.Oflag&unix.ONLCR != 0
		if raw && mapped {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("continued lflag %#x oflag %#x, want non-canonical no-echo with OPOST|ONLCR", restored.Lflag, restored.Oflag)
		}
		time.Sleep(20 * time.Millisecond)
	}
}

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
