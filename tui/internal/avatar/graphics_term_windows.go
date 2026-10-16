//go:build windows

package avatar

import "github.com/charmbracelet/x/term"

// holdTerminal keeps the console mode the probe set. Windows does not map
// newlines from the input type, and cancelreader only polls a console handle,
// so the replay pipe is read directly and stays Windows-safe.
func (b *bufferedProbeInput) holdTerminal(fd uintptr, state *term.State) {
	b.mu.Lock()
	b.fd = fd
	b.orig = state
	b.mu.Unlock()
}
