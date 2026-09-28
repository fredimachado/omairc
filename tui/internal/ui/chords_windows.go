//go:build windows

package ui

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

// windowsShortcutKeyLabels adds WT-safe aliases beside the shared chords.
var windowsShortcutKeyLabels = map[string]string{
	"Alt+Down / Alt+Up":                 "Alt+Down / Alt+Up / Ctrl+Alt+Down / Up",
	"Alt+Left / Alt+Right":              "Alt+Left / Alt+Right / Ctrl+Alt+Left / Right",
	"Alt+Shift+Left / Right":            "Alt+Shift+Left / Right / Ctrl+Shift+Left / Right",
	"Alt+Shift+Up / Down":               "Alt+Shift+Up / Down / Ctrl+Alt+Shift+Up / Down",
	"Ctrl+,":   "Ctrl+, / Ctrl+]",
	"Ctrl+Tab": "Ctrl+Tab / Ctrl+PgDn",
}

// displayShortcutKeys returns the shortcuts-sheet label for one chord row.
func displayShortcutKeys(keys string) string {
	if label, ok := windowsShortcutKeyLabels[keys]; ok {
		return label
	}
	return keys
}
