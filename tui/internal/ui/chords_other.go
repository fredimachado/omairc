//go:build !windows

package ui

import tea "charm.land/bubbletea/v2"

// keyboardEnhancements is the kitty keyboard mode Omairc asks for on Unix.
//
// Bubble Tea always enables basic key disambiguation, which already reports
// Esc, ctrl+key, alt+key, shift+alt+key and ctrl+shift+key as CSI-u. Asking for
// "report all keys as escape codes" on top of that is what breaks text entry:
// the kitty protocol always sends the *unshifted* codepoint, and without
// "report alternate keys" or "report associated text" a terminal encodes
// shift+3 as "3 plus a shift modifier". ultraviolet then upper-cases the
// codepoint, so the composer types "3" instead of "#" and `/join #omarchy`
// becomes `/join 3omarchy`. The same gap loses AltGr, dead-key, and IME text,
// because report-all-keys suppresses the terminal's normal UTF-8 text. Leaving
// it off lets the terminal send the composed characters directly.
func keyboardEnhancements() tea.KeyboardEnhancements {
	return tea.KeyboardEnhancements{}
}

// normalizeChordKey leaves chord names unchanged on non-Windows platforms.
func normalizeChordKey(key string) string {
	return key
}

// displayShortcutKeys returns the shortcuts-sheet label for one chord row.
func displayShortcutKeys(keys string) string {
	return keys
}

// displayShortcutAction returns the shortcuts-sheet action text for one row.
func displayShortcutAction(action string) string {
	return action
}

// nickJumpChordKey maps a Windows Terminal alias onto ctrl+shift+k before the
// chord table runs. Non-Windows builds return the key unchanged.
func nickJumpChordKey(key string) string {
	return key
}
