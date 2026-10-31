package storage

import (
	"os"
	"path/filepath"
)

// STSFile is the STS policy cache file name, beside the settings file. It
// mirrors client/src/storage/paths.rs STS_FILE and Qt's IrcStsStore.
const STSFile = "sts"

// ConfigPath is the shared QSettings INI path both clients use:
// <GenericConfigRoot>/omairc/omairc.conf. Both clients use organization and
// application name "omairc", so QSettings writes
// <GenericConfigLocation>/omairc/omairc.conf on Linux. It returns "" when the
// platform config root is unknown.
func ConfigPath() string {
	root := GenericConfigRoot()
	if root == "" {
		return ""
	}
	return filepath.Join(root, "omairc", "omairc.conf")
}

// STSPath is the shared STS policy cache beside ConfigPath
// (<GenericConfigRoot>/omairc/sts). Desktop and TUI read and write the same
// file. It returns "" when the platform config root is unknown.
func STSPath() string {
	conf := ConfigPath()
	if conf == "" {
		return ""
	}
	return STSPathBeside(conf)
}

// STSPathBeside returns the STS cache path in the folder that holds
// settingsFile, mirroring sts_path_beside.
func STSPathBeside(settingsFile string) string {
	if settingsFile == "" {
		return ""
	}
	return filepath.Join(filepath.Dir(settingsFile), STSFile)
}

// TranscriptRoot returns $OMAIRC_TRANSCRIPT_ROOT when set, else
// <GenericStateRoot>/omairc/logs. This mirrors
// IrcConversationLog::defaultRoot and xdgLogsRoot in
// `client/src/storage/conversationlog.rs`, including the environment override.
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

// CrashLogPath is the TUI crash log: <GenericStateRoot>/omairc/omairc-tui.log.
// It sits beside the Qt client's omairc.log (OmaircFileLog::defaultPath) so
// either client's crash record is in the same state directory. It returns ""
// when the platform state root is unknown.
func CrashLogPath() string {
	root := GenericStateRoot()
	if root == "" {
		return ""
	}
	return filepath.Join(root, "omairc", "omairc-tui.log")
}
