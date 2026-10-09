//go:build !windows

package ui

import (
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// TestUnixDoesNotReportAllKeys guards the shift+3 fix. Requesting report-all
// keys without alternate/associated text makes a kitty terminal (foot, kitty,
// ghostty, alacritty) encode shift+3 as CSI 51;2u, the *unshifted* codepoint
// plus a shift modifier, which ultraviolet upper-cases back to "3". The
// composer then types "3" instead of "#", so `/join #omarchy` becomes
// `/join 3omarchy`. Basic disambiguation is always on and already covers every
// Omairc chord, so Unix must leave report-all-keys off and let the terminal
// send the characters the user typed.
func TestUnixDoesNotReportAllKeys(t *testing.T) {
	if got := keyboardEnhancements(); got != (tea.KeyboardEnhancements{}) {
		t.Fatalf("keyboardEnhancements() = %+v, want zero; report-all-keys turns shift+3 into \"3\" on Unix", got)
	}
}

// TestViewUsesPlatformKeyboardEnhancements ties the rendered View to the
// platform helper so a stray View edit cannot request report-all-keys again.
func TestViewUsesPlatformKeyboardEnhancements(t *testing.T) {
	m := New(controller.New(), nil)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	m = updated.(*Model)
	if got, want := m.View().KeyboardEnhancements, keyboardEnhancements(); got != want {
		t.Fatalf("View().KeyboardEnhancements = %+v, want %+v", got, want)
	}
}
