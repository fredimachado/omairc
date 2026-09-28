//go:build !windows

package ui

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
