package crashlog

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
)

// quietStderr points os.Stderr at a scratch file for the duration of the test,
// so the bytes the tee forwards to the terminal do not land in the test log.
func quietStderr(t *testing.T) {
	t.Helper()
	original := os.Stderr
	f, err := os.CreateTemp(t.TempDir(), "stderr")
	if err != nil {
		t.Fatalf("create scratch stderr: %v", err)
	}
	os.Stderr = f
	t.Cleanup(func() {
		os.Stderr = original
		_ = f.Close()
	})
}

func TestPathSitsBesideTheQtLog(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_STATE_HOME", dir)
	want := filepath.Join(dir, "omairc", "omairc-tui.log")
	if got := Path(); got != want {
		t.Fatalf("Path() = %q, want %q", got, want)
	}
}

func TestInstallTeesStderrIntoTheLog(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_STATE_HOME", dir)
	quietStderr(t)

	restore, err := Install()
	if err != nil {
		t.Fatalf("Install() error = %v", err)
	}
	if _, err := fmt.Fprint(os.Stderr, "Caught panic: boom\n"); err != nil {
		t.Fatalf("write to stderr: %v", err)
	}
	restore()

	data, err := os.ReadFile(Path())
	if err != nil {
		t.Fatalf("read crash log: %v", err)
	}
	if !strings.Contains(string(data), "Caught panic: boom") {
		t.Fatalf("crash log = %q, want the stderr bytes", data)
	}
}

func TestRestoreIsIdempotentAndStopsCapturing(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_STATE_HOME", dir)
	quietStderr(t)

	restore, err := Install()
	if err != nil {
		t.Fatalf("Install() error = %v", err)
	}
	if _, err := fmt.Fprint(os.Stderr, "before restore\n"); err != nil {
		t.Fatalf("write to stderr: %v", err)
	}
	restore()
	restore() // must not panic, block, or write a second time

	if _, err := fmt.Fprint(os.Stderr, "after restore\n"); err != nil {
		t.Fatalf("write to stderr: %v", err)
	}
	data, err := os.ReadFile(Path())
	if err != nil {
		t.Fatalf("read crash log: %v", err)
	}
	if !strings.Contains(string(data), "before restore") {
		t.Fatalf("crash log = %q, want the pre-restore line", data)
	}
	if strings.Contains(string(data), "after restore") {
		t.Fatalf("crash log = %q, must not capture after restore", data)
	}
}

func TestInstallAppendsToAnExistingLog(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_STATE_HOME", dir)
	quietStderr(t)

	path := Path()
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatalf("mkdir state dir: %v", err)
	}
	if err := os.WriteFile(path, []byte("earlier crash\n"), 0o600); err != nil {
		t.Fatalf("seed crash log: %v", err)
	}

	restore, err := Install()
	if err != nil {
		t.Fatalf("Install() error = %v", err)
	}
	if _, err := fmt.Fprint(os.Stderr, "later crash\n"); err != nil {
		t.Fatalf("write to stderr: %v", err)
	}
	restore()

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read crash log: %v", err)
	}
	if !strings.Contains(string(data), "earlier crash") {
		t.Fatalf("crash log = %q, must keep the earlier entry", data)
	}
	if !strings.Contains(string(data), "later crash") {
		t.Fatalf("crash log = %q, want the new entry appended", data)
	}
}

func TestLogFileIsOwnerOnly(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Windows does not expose POSIX permission bits")
	}
	dir := t.TempDir()
	t.Setenv("XDG_STATE_HOME", dir)
	quietStderr(t)

	restore, err := Install()
	if err != nil {
		t.Fatalf("Install() error = %v", err)
	}
	restore()

	info, err := os.Stat(Path())
	if err != nil {
		t.Fatalf("stat crash log: %v", err)
	}
	if got := info.Mode().Perm(); got != 0o600 {
		t.Fatalf("crash log mode = %o, want 600", got)
	}
}

func TestInstallWithoutAStateRootIsANoOp(t *testing.T) {
	// An empty XDG_STATE_HOME falls back to $HOME; the only reliable way to
	// have no root is to clear HOME too, which t.Setenv allows.
	t.Setenv("XDG_STATE_HOME", "")
	t.Setenv("HOME", "")
	if Path() != "" {
		t.Skip("this platform still resolves a state root without HOME")
	}
	restore, err := Install()
	if err != nil {
		t.Fatalf("Install() error = %v", err)
	}
	if restore == nil {
		t.Fatal("Install() must return a callable restore")
	}
	restore()
}

// panicModel panics the first time Bubble Tea calls Update, which is what a
// bug in the shell looks like from the runtime's point of view.
type panicModel struct{}

func (panicModel) Init() tea.Cmd                       { return nil }
func (panicModel) Update(tea.Msg) (tea.Model, tea.Cmd) { panic("boom from update") }
func (panicModel) View() tea.View                      { return tea.View{} }

// TestRecoveredPanicReachesTheLog is the end-to-end proof: Bubble Tea catches
// an Update panic and reports it on os.Stderr, which is exactly the path the
// crash log has to intercept. Without the tee this stack is lost with the
// terminal.
func TestRecoveredPanicReachesTheLog(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_STATE_HOME", dir)
	quietStderr(t)

	restore, err := Install()
	if err != nil {
		t.Fatalf("Install() error = %v", err)
	}

	p := tea.NewProgram(
		panicModel{},
		tea.WithInput(nil),
		tea.WithOutput(io.Discard),
		tea.WithoutRenderer(),
		tea.WithoutSignalHandler(),
		tea.WithoutSignals(),
		tea.WithWindowSize(80, 24),
	)
	if _, err := p.Run(); err == nil {
		restore()
		t.Fatal("a panicking Update must fail the program")
	}
	restore()

	data, err := os.ReadFile(Path())
	if err != nil {
		t.Fatalf("read crash log: %v", err)
	}
	if !strings.Contains(string(data), "Caught panic:") {
		t.Fatalf("crash log = %q, want Bubble Tea's panic report", data)
	}
	if !strings.Contains(string(data), "boom from update") {
		t.Fatalf("crash log = %q, want the panic message", data)
	}
}
