package ui

// WindowsShortcutKeyLabels adds WT-safe aliases beside the shared chords on
// Windows. Non-Windows builds use it only in tests so narrow layouts stay
// checked against the widest shipped labels.
var WindowsShortcutKeyLabels = map[string]string{
	"Alt+Down / Alt+Up":      "Alt+Down / Alt+Up / Ctrl+Alt+Down / Up",
	"Alt+Left / Alt+Right":   "Alt+Left / Alt+Right / Ctrl+Alt+Left / Right",
	"Alt+Shift+Left / Right": "Alt+Shift+Left / Right / Ctrl+Shift+Left / Right",
	"Alt+Shift+Up / Down":    "Alt+Shift+Up / Down / Ctrl+Alt+Shift+Up / Down",
	"Ctrl+,":                 "Ctrl+, / Ctrl+]",
	"Ctrl+Tab":               "Ctrl+Tab / Ctrl+PgDn",
}

// ExpandShortcutKeyLabels returns the shortcuts-sheet label for keys, including
// Windows Terminal aliases when present in WindowsShortcutKeyLabels.
func ExpandShortcutKeyLabels(keys string) string {
	if label, ok := WindowsShortcutKeyLabels[keys]; ok {
		return label
	}
	return keys
}
