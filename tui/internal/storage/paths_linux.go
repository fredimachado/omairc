//go:build linux

package storage

import (
	"os"
	"path/filepath"
)

// GenericConfigRoot is the XDG base config directory: $XDG_CONFIG_HOME, falling
// back to ~/.config. This is the Linux
// QStandardPaths::GenericConfigLocation that QSettings writes
// <organization>/<application>.conf into.
func GenericConfigRoot() string {
	if xdg := os.Getenv("XDG_CONFIG_HOME"); xdg != "" {
		return xdg
	}
	return underHome(".config")
}

// GenericStateRoot is the XDG base state directory: $XDG_STATE_HOME, falling
// back to ~/.local/state. This is the Linux
// QStandardPaths::GenericStateLocation that owns the transcript root.
func GenericStateRoot() string {
	if xdg := os.Getenv("XDG_STATE_HOME"); xdg != "" {
		return xdg
	}
	return underHome(".local", "state")
}

// underHome joins parts onto the user's home directory, returning "" when no
// home directory is known.
func underHome(parts ...string) string {
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		return ""
	}
	return filepath.Join(append([]string{home}, parts...)...)
}
