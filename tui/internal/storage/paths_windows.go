//go:build windows

package storage

import "os"

// GenericConfigRoot is the Windows config directory: %LOCALAPPDATA%, falling
// back to $XDG_CONFIG_HOME when set. This mirrors QStandardPaths::
// GenericConfigLocation and omaircConfigRoot() in src/omaircpaths.cpp.
func GenericConfigRoot() string {
	if xdg := os.Getenv("XDG_CONFIG_HOME"); xdg != "" {
		return xdg
	}
	return os.Getenv("LOCALAPPDATA")
}

// GenericStateRoot is the Windows state directory: %LOCALAPPDATA%, falling
// back to $XDG_STATE_HOME when set. This mirrors QStandardPaths::
// GenericStateLocation and omaircStateRoot() in src/omaircpaths.cpp.
func GenericStateRoot() string {
	if xdg := os.Getenv("XDG_STATE_HOME"); xdg != "" {
		return xdg
	}
	return os.Getenv("LOCALAPPDATA")
}
