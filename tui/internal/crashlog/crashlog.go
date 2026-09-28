// Package crashlog keeps a durable record of omairc-tui crashes.
//
// The terminal holds the only copy of a crash. Bubble Tea recovers panics and
// prints the message and stack to os.Stderr before restoring the terminal, and
// the Go runtime prints unrecovered panics and fatal signals to the terminal
// itself. Once the shell scrolls or closes, that evidence is gone. Install
// tees os.Stderr into <GenericStateRoot>/omairc/omairc-tui.log and points the
// runtime's fatal crash output at the same file, so the next crash can be
// diagnosed from disk instead of a terminal that no longer exists.
//
// The log is crash-only: a clean run writes nothing, so the file stays empty
// until something actually dies, and its mtime is the crash time. It is
// created owner-only (0600) and holds raw stderr, so never pass secret
// material to panic: a panic value is written verbatim.
package crashlog

import (
	"os"
	"path/filepath"
	"runtime/debug"
	"sync"

	"github.com/fredimachado/omairc/tui/internal/storage"
)

// Path returns the crash log path, or "" when no state directory is known.
func Path() string {
	return storage.CrashLogPath()
}

// Install tees os.Stderr into the crash log and points the runtime's fatal
// crash output at it. It returns a function that stops capturing and flushes
// the log; the function is safe to call more than once.
//
// Install is best-effort. When the log cannot be reached the process keeps its
// original stderr and Install reports the error, because a missing crash log
// must never keep the shell from starting. A returned restore is always safe
// to call.
func Install() (restore func(), err error) {
	noop := func() {}

	path := Path()
	if path == "" {
		return noop, nil
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return noop, err
	}
	log, err := os.OpenFile(path, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o600)
	if err != nil {
		return noop, err
	}
	// The Qt client pins its log to owner-only; match that even when the file
	// already existed with looser permissions.
	_ = os.Chmod(path, 0o600)

	// The runtime writes unrecovered panics, fatal signals, and fatal runtime
	// errors here, in addition to the terminal. Bubble Tea's recovered panics
	// never reach the runtime, so the stderr tee below catches those.
	if err := debug.SetCrashOutput(log, debug.CrashOptions{}); err != nil {
		_ = log.Close()
		return noop, err
	}

	terminal := os.Stderr
	reader, writer, err := os.Pipe()
	if err != nil {
		_ = debug.SetCrashOutput(nil, debug.CrashOptions{})
		_ = log.Close()
		return noop, err
	}
	os.Stderr = writer

	done := make(chan struct{})
	go tee(reader, log, terminal, done)

	var once sync.Once
	restore = func() {
		once.Do(func() {
			// Stop capturing before draining, so every byte written up to
			// here is already in the pipe.
			os.Stderr = terminal
			_ = writer.Close()
			<-done
			_ = reader.Close()
			_ = debug.SetCrashOutput(nil, debug.CrashOptions{})
			_ = log.Close()
		})
	}
	return restore, nil
}

// tee copies everything written to the redirected stderr into the crash log
// and on to the real terminal. The log is written first and the terminal error
// is ignored, so a terminal that has already gone away cannot drop the crash
// record.
func tee(reader, log, terminal *os.File, done chan<- struct{}) {
	defer close(done)
	buf := make([]byte, 4096)
	for {
		n, err := reader.Read(buf)
		if n > 0 {
			_, _ = log.Write(buf[:n])
			_, _ = terminal.Write(buf[:n])
		}
		if err != nil {
			return
		}
	}
}
