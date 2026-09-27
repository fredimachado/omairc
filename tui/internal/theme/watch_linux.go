//go:build linux

package theme

import (
	"os"
	"sync"
	"time"

	"github.com/fredimachado/omairc/tui/internal/session"
)

// Watcher polls the Omarchy theme tree for changes and emits a Change whenever
// the on-disk theme changes. It follows the session.Clock seam so tests drive it
// with session.FakeClock and never sleep.
//
// The watch covers both the "current" symlink and its "theme" directory:
// repointing "current" changes the symlink's own mtime, while rewriting
// colors.toml changes the theme directory's and the file's mtime. Missing the
// symlink would leave the shell on the previous theme after a swap. A file that
// appears or disappears is a change too, and a missing file yields Fallback().
type Watcher struct {
	clock    session.Clock
	interval time.Duration
	colors   string
	dirs     []string

	mu     sync.Mutex
	timer  session.Timer
	closed bool
	state  themeStamp
	found  bool
	last   Colors
	ch     chan Change
}

// NewWatcher starts watching the Omarchy theme tree. A non-positive interval
// defaults to DefaultPollInterval and a nil clock defaults to the real clock.
func NewWatcher(clock session.Clock, interval time.Duration) *Watcher {
	return newWatcher(clock, interval, colorsPath(), watchDirs())
}

// newWatcher is the test seam: it watches an explicit colors.toml and directory
// pair instead of resolving the Omarchy paths.
func newWatcher(clock session.Clock, interval time.Duration, colorsFile string, dirs []string) *Watcher {
	if clock == nil {
		clock = session.NewRealClock()
	}
	if interval <= 0 {
		interval = DefaultPollInterval
	}
	watcher := &Watcher{
		clock:    clock,
		interval: interval,
		colors:   colorsFile,
		dirs:     dirs,
		ch:       make(chan Change, 1),
	}
	watcher.state = readThemeStamp(colorsFile, dirs)
	watcher.last, watcher.found = loadColors(colorsFile)
	watcher.timer = clock.AfterFunc(interval, watcher.poll)
	return watcher
}

// Changes streams theme changes. The channel is buffered with the newest change
// only: a slow consumer may miss intermediate swaps but always sees the latest
// palette, which is all the shell needs. Close closes it.
func (w *Watcher) Changes() <-chan Change {
	return w.ch
}

// Current returns the palette loaded most recently.
func (w *Watcher) Current() Colors {
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.last
}

// Close stops the watcher and closes Changes. It is safe to call more than once.
func (w *Watcher) Close() {
	w.mu.Lock()
	defer w.mu.Unlock()
	if w.closed {
		return
	}
	w.closed = true
	if w.timer != nil {
		w.timer.Stop()
	}
	close(w.ch)
}

// poll re-stats the theme tree, reloads the palette when anything moved, and
// re-arms the timer. It holds no lock while reading the filesystem so Close can
// proceed promptly.
func (w *Watcher) poll() {
	w.mu.Lock()
	if w.closed {
		w.mu.Unlock()
		return
	}
	w.mu.Unlock()

	state := readThemeStamp(w.colors, w.dirs)
	colors, found := loadColors(w.colors)

	w.mu.Lock()
	defer w.mu.Unlock()
	if w.closed {
		return
	}
	if !state.equal(w.state) {
		w.state = state
		w.last = colors
		w.found = found
		w.emitLocked(Change{Colors: colors, Path: w.colors, Found: found})
	}
	w.timer = w.clock.AfterFunc(w.interval, w.poll)
}

// emitLocked replaces any pending change with the newest one so the buffered
// channel never blocks the poll.
func (w *Watcher) emitLocked(change Change) {
	select {
	case w.ch <- change:
		return
	default:
	}
	select {
	case <-w.ch:
	default:
	}
	select {
	case w.ch <- change:
	default:
	}
}

// fileStamp is one path's identity for change detection.
type fileStamp struct {
	mod    time.Time
	size   int64
	exists bool
}

// themeStamp is the whole watched tree's identity.
type themeStamp struct {
	current fileStamp
	theme   fileStamp
	colors  fileStamp
}

// equal reports whether two stamps describe the same on-disk state.
func (s themeStamp) equal(other themeStamp) bool {
	return s.current.equal(other.current) &&
		s.theme.equal(other.theme) &&
		s.colors.equal(other.colors)
}

func (s fileStamp) equal(other fileStamp) bool {
	return s.exists == other.exists && s.size == other.size && s.mod.Equal(other.mod)
}

// readThemeStamp stats the current symlink, the theme directory, and the colors
// file. The symlink is read with Lstat so a target swap registers; the directory
// and file are followed.
func readThemeStamp(colorsFile string, dirs []string) themeStamp {
	var stamp themeStamp
	if len(dirs) > 0 {
		stamp.current = statStamp(dirs[0], false)
	}
	if len(dirs) > 1 {
		stamp.theme = statStamp(dirs[1], true)
	}
	stamp.colors = statStamp(colorsFile, true)
	return stamp
}

// statStamp captures a path's mtime, size, and existence. A missing path is a
// zero stamp, so file-appear and file-disappear both register as changes.
func statStamp(path string, follow bool) fileStamp {
	if path == "" {
		return fileStamp{}
	}
	var (
		info os.FileInfo
		err  error
	)
	if follow {
		info, err = os.Stat(path)
	} else {
		info, err = os.Lstat(path)
	}
	if err != nil {
		return fileStamp{}
	}
	return fileStamp{mod: info.ModTime(), size: info.Size(), exists: true}
}
