//go:build windows

package avatar

import (
	"encoding/binary"
	"io"
	"os"
	"syscall"
	"testing"
	"time"
	"unsafe"

	"golang.org/x/sys/windows"
)

var procWriteConsoleInputW = windows.NewLazySystemDLL("kernel32.dll").NewProc("WriteConsoleInputW")

type consoleInputRecord struct {
	EventType uint16
	_         [2]byte
	Event     [16]byte
}

func openConsoleInput(t *testing.T) (*os.File, windows.Handle, uint32) {
	t.Helper()
	h, err := windows.CreateFile(
		windows.StringToUTF16Ptr("CONIN$"),
		windows.GENERIC_READ|windows.GENERIC_WRITE,
		windows.FILE_SHARE_READ|windows.FILE_SHARE_WRITE,
		nil,
		windows.OPEN_EXISTING,
		0,
		0,
	)
	if err != nil {
		t.Fatalf("console input required to prove Windows stop path: %v", err)
	}
	var mode uint32
	if err := windows.GetConsoleMode(h, &mode); err != nil {
		_ = windows.CloseHandle(h)
		t.Fatalf("GetConsoleMode: %v", err)
	}
	raw := mode &^ (windows.ENABLE_LINE_INPUT | windows.ENABLE_ECHO_INPUT | windows.ENABLE_PROCESSED_INPUT)
	if err := windows.SetConsoleMode(h, raw); err != nil {
		_ = windows.CloseHandle(h)
		t.Fatalf("SetConsoleMode: %v", err)
	}
	if err := windows.FlushConsoleInputBuffer(h); err != nil {
		_ = windows.SetConsoleMode(h, mode)
		_ = windows.CloseHandle(h)
		t.Fatalf("FlushConsoleInputBuffer: %v", err)
	}
	// Production selects copyProbeInputConsole when PeekNamedPipe fails.
	if _, err := peekNamedPipeAvail(h); err == nil {
		_ = windows.SetConsoleMode(h, mode)
		_ = windows.CloseHandle(h)
		t.Fatal("CONIN$ peeked as a pipe; console stop path would not run")
	}
	return os.NewFile(uintptr(h), "CONIN$"), h, mode
}

func writeConsoleKey(h windows.Handle, ch byte) error {
	var rec consoleInputRecord
	rec.EventType = windows.KEY_EVENT
	binary.LittleEndian.PutUint32(rec.Event[0:4], 1) // key down
	binary.LittleEndian.PutUint16(rec.Event[4:6], 1)
	binary.LittleEndian.PutUint16(rec.Event[6:8], uint16(ch))
	binary.LittleEndian.PutUint16(rec.Event[10:12], uint16(ch))
	var written uint32
	r1, _, e := syscall.SyscallN(procWriteConsoleInputW.Addr(),
		uintptr(h),
		uintptr(unsafe.Pointer(&rec)),
		1,
		uintptr(unsafe.Pointer(&written)),
	)
	if r1 == 0 {
		if e != 0 {
			return e
		}
		return syscall.EINVAL
	}
	return nil
}

// Close must stop the production Windows console copier without a naked
// blocking Read. The pipe-only TestCloseDoesNotEatPostCloseKeystroke does not
// exercise this path: ProbeGraphics wraps term.IsTerminal stdin, which is a
// console handle, so peekNamedPipeAvail fails and copyProbeInputConsole runs.
func TestCloseDoesNotEatPostCloseConsoleKeystroke(t *testing.T) {
	src, srcH, origMode := openConsoleInput(t)
	defer func() {
		_ = windows.SetConsoleMode(srcH, origMode)
		_ = src.Close()
	}()

	input := probeResult{replay: []byte("hi"), dropLate: true}.wrap(src)
	if input == nil {
		t.Fatal("wrap returned nil")
	}
	closer, ok := input.(io.Closer)
	if !ok {
		t.Fatalf("input = %T, want Closer", input)
	}
	if got := readSoon(t, input); got != "hi" {
		t.Fatalf("replay = %q, want hi", got)
	}

	closed := make(chan error, 1)
	go func() { closed <- closer.Close() }()
	select {
	case err := <-closed:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(time.Second):
		t.Fatal("Close did not return; console copier still blocked on Read")
	}

	if err := writeConsoleKey(srcH, 'K'); err != nil {
		t.Fatalf("WriteConsoleInput: %v", err)
	}
	time.Sleep(50 * time.Millisecond)

	type readResult struct {
		n   int
		err error
		b   byte
	}
	done := make(chan readResult, 1)
	go func() {
		buf := make([]byte, 1)
		var n uint32
		err := windows.ReadFile(srcH, buf, &n, nil)
		var b byte
		if n > 0 {
			b = buf[0]
		}
		done <- readResult{n: int(n), err: err, b: b}
	}()
	select {
	case got := <-done:
		if got.err != nil || got.n != 1 || got.b != 'K' {
			t.Fatalf("post-close key = %q n=%d err=%v, want K still on console", got.b, got.n, got.err)
		}
	case <-time.After(200 * time.Millisecond):
		t.Fatal("post-close key was eaten by the console copier")
	}
}
