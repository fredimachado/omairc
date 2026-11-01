//go:build windows

package storage

import (
	"os"
	"path/filepath"
)

// GenericConfigRoot is the Windows config directory: %LOCALAPPDATA%, falling
// back to $XDG_CONFIG_HOME when set. This mirrors QStandardPaths::
// GenericConfigLocation and omaircConfigRoot in `omaircpaths`.
func GenericConfigRoot() string {
	if xdg := os.Getenv("XDG_CONFIG_HOME"); xdg != "" {
		return xdg
	}
	return os.Getenv("LOCALAPPDATA")
}

// GenericStateRoot is the Windows state directory: %LOCALAPPDATA%, falling
// back to $XDG_STATE_HOME when set. This mirrors QStandardPaths::
// GenericStateLocation and omaircStateRoot in `omaircpaths`.
func GenericStateRoot() string {
	if xdg := os.Getenv("XDG_STATE_HOME"); xdg != "" {
		return xdg
	}
	return os.Getenv("LOCALAPPDATA")
}

// scrollPlacesPathDefault matches the desktop client's Windows state root
// (%LOCALAPPDATA%\State\omairc\scrollplaces.ini) without changing
// GenericStateRoot, which still owns transcripts and the crash log under
// bare %LOCALAPPDATA%\omairc.
func scrollPlacesPathDefault() string {
	local := os.Getenv("LOCALAPPDATA")
	if local == "" {
		return ""
	}
	return filepath.Join(local, "State", "omairc", ScrollPlacesFile)
}
