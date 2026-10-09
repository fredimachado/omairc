//go:build windows

package ui

import tea "charm.land/bubbletea/v2"

// keyboardEnhancements is the kitty keyboard mode Omairc asks for on Windows.
//
// ConPTY collapses ctrl+shift+letter to ctrl+letter and Ctrl+Enter to ctrl+j,
// so request escape-coded keys to keep the shift chords (nick jump, server
// list, inbox, …) and Connect Apply distinct on Windows Terminal. Report
// alternate keys as well, so a terminal that supports it delivers the shifted
// codepoint for shift+<symbol> instead of the unshifted key plus a modifier.
// The ctrl+j aliases in this file cover the collapse where it still happens.
func keyboardEnhancements() tea.KeyboardEnhancements {
	return tea.KeyboardEnhancements{
		ReportAllKeysAsEscapeCodes: true,
		ReportAlternateKeys:        true,
	}
}

// nickJumpChordKey maps ConPTY's collapsed Ctrl+Shift+K encoding onto ctrl+shift+k.
// Connect still owns bare ctrl+j for Apply; handleKey never calls this while
// the sheet is open.
func nickJumpChordKey(key string) string {
	if key == "ctrl+j" {
		return "ctrl+shift+k"
	}
	return key
}

// normalizeChordKey maps Windows Terminal aliases and ConPTY legacy control-byte
// names onto the shared chord names from keyboard.md. The originals stay in the
// table for terminals that deliver them; these aliases reach Omairc when WT
// keeps the default bindings or ConPTY encodes punctuation chords as FS/US bytes.
func normalizeChordKey(key string) string {
	switch key {
	case "ctrl+_":
		// ConPTY sends Ctrl+/ as 0x1f, which ultraviolet names ctrl+_.
		return "ctrl+/"
	case "ctrl+]":
		return "ctrl+,"
	case "ctrl+alt+left":
		return "alt+left"
	case "ctrl+alt+right":
		return "alt+right"
	case "ctrl+alt+up":
		return "alt+up"
	case "ctrl+alt+down":
		return "alt+down"
	case "ctrl+shift+left":
		return "alt+shift+left"
	case "ctrl+shift+right":
		return "alt+shift+right"
	case "ctrl+alt+shift+up":
		return "alt+shift+up"
	case "ctrl+alt+shift+down":
		return "alt+shift+down"
	case "ctrl+pgdown":
		return "ctrl+tab"
	}
	return key
}

// displayShortcutKeys returns the shortcuts-sheet label for one chord row.
func displayShortcutKeys(keys string) string {
	return ExpandShortcutKeyLabels(keys)
}

// displayShortcutAction returns the shortcuts-sheet action text for one row.
func displayShortcutAction(action string) string {
	return action
}
