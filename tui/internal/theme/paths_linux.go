//go:build linux

package theme

import (
	"os"
	"path/filepath"
)

// omarchyStateDir is ~/.local/state/omarchy, the Omarchy state root.
func omarchyStateDir() string {
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		return ""
	}
	return filepath.Join(home, ".local", "state", "omarchy")
}

// colorsPath is the current theme's colors.toml, mirroring the path in
// Backend::loadOmarchyTheme. It is "" when no home directory is known.
func colorsPath() string {
	root := omarchyStateDir()
	if root == "" {
		return ""
	}
	return filepath.Join(root, "current", "theme", "colors.toml")
}

// watchDirs returns the directories the watcher polls: the "current" symlink
// (whose own mtime changes when Omarchy repoints it) and its "theme" child
// (whose mtime changes when colors.toml is written). Watching both is what
// catches a theme swap, not only an in-place edit.
func watchDirs() []string {
	root := omarchyStateDir()
	if root == "" {
		return nil
	}
	current := filepath.Join(root, "current")
	return []string{current, filepath.Join(current, "theme")}
}
