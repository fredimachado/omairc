//go:build darwin

package storage

import (
	"os"
	"path/filepath"
)

// GenericConfigRoot is ~/Library/Preferences on macOS
// (QStandardPaths::GenericConfigLocation). $XDG_CONFIG_HOME is honoured when
// set, mirroring omaircConfigRoot in src/omaircpaths.cpp.
func GenericConfigRoot() string {
	if xdg := os.Getenv("XDG_CONFIG_HOME"); xdg != "" {
		return xdg
	}
	return underHome("Library", "Preferences")
}

// GenericStateRoot is ~/Library/Preferences/State on macOS
// (QStandardPaths::GenericStateLocation). $XDG_STATE_HOME is honoured when
// set, mirroring omaircStateRoot in src/omaircpaths.cpp.
func GenericStateRoot() string {
	if xdg := os.Getenv("XDG_STATE_HOME"); xdg != "" {
		return xdg
	}
	return underHome("Library", "Preferences", "State")
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
