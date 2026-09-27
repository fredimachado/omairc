package storage

import (
	"os"
	"path/filepath"
)

// ConfigPath is the shared QSettings INI path both clients use:
// <GenericConfigRoot>/omairc/omairc.conf. src/main.cpp sets both the
// organization and application name to "omairc", so QSettings writes
// <GenericConfigLocation>/omairc/omairc.conf on Linux. It returns "" when the
// platform config root is unknown.
func ConfigPath() string {
	root := GenericConfigRoot()
	if root == "" {
		return ""
	}
	return filepath.Join(root, "omairc", "omairc.conf")
}

// TranscriptRoot returns $OMAIRC_TRANSCRIPT_ROOT when set, else
// <GenericStateRoot>/omairc/logs. This mirrors
// IrcConversationLog::defaultRoot and xdgLogsRoot in
// src/irc/ircconversationlog.cpp, including the environment override.
func TranscriptRoot() string {
	if scoped := os.Getenv("OMAIRC_TRANSCRIPT_ROOT"); scoped != "" {
		return scoped
	}
	root := GenericStateRoot()
	if root == "" {
		return ""
	}
	return filepath.Join(root, "omairc", "logs")
}
