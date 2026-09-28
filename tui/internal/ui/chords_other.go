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
