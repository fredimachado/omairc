package storage

import (
	"bytes"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// ConversationLog persists and reads back conversation transcripts, one JSONL
// file per conversation keyed by the network and target. It mirrors
// IrcConversationLog in src/irc/ircconversationlog.cpp and satisfies
// irc.ConversationLog.
//
// The on-disk layout is <root>/<OmaircStorageSegment(network)>/
// <OmaircWireStorageSegment(target)>. PathFor migrates a pre-Omairc
// (LegacyStorageSegment) file in place when it finds one, matching
// migrateLegacyTranscriptPath in the C++.
type ConversationLog struct {
	root string
}

// ConversationLog is the on-disk irc.ConversationLog implementation; the
// assertion catches signature drift at compile time.
var _ irc.ConversationLog = (*ConversationLog)(nil)

// NewConversationLog returns a log rooted at root, or TranscriptRoot() when
// root is "". TranscriptRoot honours OMAIRC_TRANSCRIPT_ROOT and otherwise
// falls back to <GenericStateRoot>/omairc/logs, mirroring defaultRoot().
func NewConversationLog(root string) *ConversationLog {
	if root == "" {
		root = TranscriptRoot()
	}
	return &ConversationLog{root: root}
}

// SetRoot repoints the log at a new root.
func (l *ConversationLog) SetRoot(root string) {
	l.root = root
}

// Root returns the current on-disk root.
func (l *ConversationLog) Root() string {
	return l.root
}

// PathFor returns the on-disk path for one conversation, migrating a legacy
// file or network directory in place when needed. mapping is ignored: the file
// name preserves the target's case, exactly as pathFor does in the C++.
func (l *ConversationLog) PathFor(networkID, target string, mapping irc.CaseMapping) string {
	_ = mapping
	newNetworkDir := irc.OmaircStorageSegment(networkID)
	newTarget := irc.OmaircWireStorageSegment(target)
	newPath := filepath.Join(l.root, newNetworkDir, newTarget)
	return conversationLogMigrateLegacyPath(l.root, networkID, target, newPath)
}

// Append stores one line, reporting whether it was written. An empty kind or
// body is a successful no-op, as is a line that the secret policy rejects.
func (l *ConversationLog) Append(networkID, target string, mapping irc.CaseMapping, line irc.TranscriptLine) bool {
	if line.Kind == "" || line.Body == "" {
		return true
	}
	channelTypes := conversationLogChannelTypes()
	if !irc.AllowsTranscript(line.Body, channelTypes) {
		return true
	}
	if !irc.AllowsTranscript(line.Author, channelTypes) {
		return true
	}
	return conversationLogAppendBytes(l.PathFor(networkID, target, mapping), conversationLogFormatLine(line))
}

// ReadTail returns up to maxLines most recent lines, oldest first. It returns
// nil when maxLines is not positive, when the file is missing, or when no line
// survives the secret-policy filter.
func (l *ConversationLog) ReadTail(networkID, target string, mapping irc.CaseMapping, maxLines int) []irc.TranscriptLine {
	if maxLines <= 0 {
		return nil
	}
	file, err := os.Open(l.PathFor(networkID, target, mapping))
	if err != nil {
		return nil
	}
	defer file.Close()
	return conversationLogReadTail(file, maxLines)
}

// conversationLogChannelTypes returns the default ISUPPORT CHANTYPES, matching
// the C++ core's single-argument IrcSecretPolicy::allowsTranscript default.
func conversationLogChannelTypes() string {
	features := irc.NewServerFeatures()
	return features.ChannelTypes()
}

// conversationLogRecord is the JSONL shape written for one transcript line.
// msgid is omitted when empty, matching formatLine.
type conversationLogRecord struct {
	Timestamp string `json:"timestamp"`
	Author    string `json:"author"`
	Kind      string `json:"kind"`
	Body      string `json:"body"`
	MsgID     string `json:"msgid,omitempty"`
}

// conversationLogFlatten replaces newlines and carriage returns with spaces,
// mirroring flattenText.
func conversationLogFlatten(text string) string {
	replaced := make([]byte, 0, len(text))
	for index := 0; index < len(text); index++ {
		switch text[index] {
		case '\n', '\r':
			replaced = append(replaced, ' ')
		default:
			replaced = append(replaced, text[index])
		}
	}
	return string(replaced)
}

// conversationLogISODateWithMs renders one instant as Qt's ISODateWithMs in
// UTC, e.g. 2011-10-19T16:40:00.000Z.
func conversationLogISODateWithMs(when time.Time) string {
	return when.UTC().Format("2006-01-02T15:04:05.000Z")
}

// conversationLogFormatLine renders one line as a compact JSON object plus a
// newline. A zero timestamp becomes the current UTC time.
func conversationLogFormatLine(line irc.TranscriptLine) []byte {
	when := line.Timestamp
	if when.IsZero() {
		when = time.Now().UTC()
	}
	record := conversationLogRecord{
		Timestamp: conversationLogISODateWithMs(when),
		Author:    conversationLogFlatten(line.Author),
		Kind:      line.Kind,
		Body:      conversationLogFlatten(line.Body),
		MsgID:     line.MsgID,
	}
	var buffer bytes.Buffer
	encoder := json.NewEncoder(&buffer)
	// Qt's compact writer does not escape '<', '>' or '&'; disable Go's HTML
	// escaping so the bytes match.
	encoder.SetEscapeHTML(false)
	// bytes.Buffer writes never fail, and Encode appends the trailing newline.
	_ = encoder.Encode(record)
	return buffer.Bytes()
}

// conversationLogParseLine parses one JSONL record. ok is false when the bytes
// are not a JSON object or the kind is empty.
func conversationLogParseLine(raw []byte) (irc.TranscriptLine, bool) {
	var record conversationLogRecord
	if err := json.Unmarshal(raw, &record); err != nil {
		return irc.TranscriptLine{}, false
	}
	if record.Kind == "" {
		return irc.TranscriptLine{}, false
	}
	line := irc.TranscriptLine{
		Author: record.Author,
		Kind:   record.Kind,
		Body:   record.Body,
		MsgID:  record.MsgID,
	}
	if record.Timestamp != "" {
		if when, err := time.Parse("2006-01-02T15:04:05.000Z", record.Timestamp); err == nil {
			line.Timestamp = when
		} else if when, err := time.Parse(time.RFC3339Nano, record.Timestamp); err == nil {
			line.Timestamp = when
		}
	}
	return line, true
}

// conversationLogPrepareTree creates the file's parent directories with
// owner-only permissions and tightens the directory plus up to two parents to
// 0700, mirroring prepareTree. Tightening failures are ignored, as in the C++.
func conversationLogPrepareTree(filePath string) bool {
	dir := filepath.Dir(filePath)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return false
	}
	for depth := 0; depth < 3; depth++ {
		if dir == "" || dir == "." || dir == string(filepath.Separator) {
			break
		}
		_ = os.Chmod(dir, 0o700)
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
		dir = parent
	}
	return true
}

// conversationLogAppendBytes appends line to path, creating the tree and
// tightening permissions first. It reports false only on a write failure.
func conversationLogAppendBytes(path string, line []byte) bool {
	if !conversationLogPrepareTree(path) {
		return false
	}
	file, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o600)
	if err != nil {
		return false
	}
	defer file.Close()
	if err := file.Chmod(0o600); err != nil {
		return false
	}
	written, err := file.Write(line)
	return err == nil && written == len(line)
}

// conversationLogReadTail ports IrcConversationLog::readTail(QIODevice*,int):
// it scans the file backwards in 4096-byte chunks, parses the newest lines
// first, filters them through the secret policy, stops at maxLines, and
// reverses the result so it is oldest first.
func conversationLogReadTail(file *os.File, maxLines int) []irc.TranscriptLine {
	if maxLines <= 0 {
		return nil
	}
	info, err := file.Stat()
	if err != nil {
		return nil
	}
	size := info.Size()
	position := size
	var pending []byte
	var lines []irc.TranscriptLine

	channelTypes := conversationLogChannelTypes()
	accept := func(raw []byte) bool {
		raw = bytes.TrimSpace(raw)
		if len(raw) == 0 {
			return false
		}
		parsed, ok := conversationLogParseLine(raw)
		if !ok {
			return false
		}
		if !irc.AllowsTranscript(parsed.Body, channelTypes) || !irc.AllowsTranscript(parsed.Author, channelTypes) {
			return false
		}
		lines = append(lines, parsed)
		return len(lines) == maxLines
	}

	const chunkSize = 4096
	for position > 0 && len(lines) < maxLines {
		bytesToRead := position
		if bytesToRead > chunkSize {
			bytesToRead = chunkSize
		}
		position -= bytesToRead
		if _, err := file.Seek(position, io.SeekStart); err != nil {
			break
		}
		chunk := make([]byte, bytesToRead)
		if _, err := io.ReadFull(file, chunk); err != nil {
			break
		}
		pending = append(chunk, pending...)

		newline := bytes.LastIndexByte(pending, '\n')
		for newline >= 0 {
			raw := pending[newline+1:]
			pending = pending[:newline]
			if accept(raw) {
				break
			}
			newline = bytes.LastIndexByte(pending, '\n')
		}
	}

	if position == 0 && len(lines) < maxLines {
		accept(pending)
	}

	for left, right := 0, len(lines)-1; left < right; left, right = left+1, right-1 {
		lines[left], lines[right] = lines[right], lines[left]
	}
	return lines
}

// conversationLogFileExists reports whether path is an existing regular file.
// QFile::exists is false for directories.
func conversationLogFileExists(path string) bool {
	info, err := os.Stat(path)
	return err == nil && !info.IsDir()
}

// conversationLogDirExists reports whether path is an existing directory.
func conversationLogDirExists(path string) bool {
	info, err := os.Stat(path)
	return err == nil && info.IsDir()
}

// conversationLogCanonicalPath mirrors storageCanonicalPath: the resolved
// canonical path, or filepath.Clean when it cannot be canonicalized (for
// example when the path does not exist).
func conversationLogCanonicalPath(path string) string {
	if path == "" {
		return ""
	}
	canonical, err := filepath.EvalSymlinks(path)
	if err != nil || canonical == "" {
		return filepath.Clean(path)
	}
	return canonical
}

// conversationLogPathsSameFile mirrors storagePathsSameFile: os.SameFile when
// both paths exist, otherwise a canonical-path comparison.
func conversationLogPathsSameFile(left, right string) bool {
	leftInfo, leftErr := os.Stat(left)
	rightInfo, rightErr := os.Stat(right)
	if leftErr == nil && rightErr == nil {
		return os.SameFile(leftInfo, rightInfo)
	}
	canonicalLeft := conversationLogCanonicalPath(left)
	return canonicalLeft != "" && canonicalLeft == conversationLogCanonicalPath(right)
}

// conversationLogDirSegmentsShareLocation mirrors
// storageDirSegmentsShareLocation: two directory names under root point at the
// same location when they are equal or resolve to the same file.
func conversationLogDirSegmentsShareLocation(root, leftSegment, rightSegment string) bool {
	if leftSegment == rightSegment {
		return true
	}
	return conversationLogPathsSameFile(filepath.Join(root, leftSegment), filepath.Join(root, rightSegment))
}

// conversationLogLegacyPathExists mirrors legacyStoragePathExists. The C++
// rejects Win32 device names on Windows; the Go core owns the device-name
// encoder, so this storage helper only checks existence.
func conversationLogLegacyPathExists(path string) bool {
	return conversationLogFileExists(path)
}

// conversationLogLegacyDirExists mirrors legacyStorageDirExists.
func conversationLogLegacyDirExists(root, segment string) bool {
	return conversationLogDirExists(filepath.Join(root, segment))
}

// conversationLogMigrateLegacyPath ports migrateLegacyTranscriptPath: it
// renames the legacy network directory to the Omairc name when that is safe,
// then moves the legacy transcript file from either the resolved or the
// legacy directory. It never overwrites an existing Omairc file, and it keeps
// the legacy path when a rename fails but the legacy file survives.
func conversationLogMigrateLegacyPath(root, networkID, target, newPath string) string {
	legacyNetworkDir := irc.LegacyStorageSegment(networkID)
	newNetworkDir := irc.OmaircStorageSegment(networkID)
	networkDirPath := filepath.Join(root, legacyNetworkDir)
	legacyNetworkDirExists := conversationLogLegacyDirExists(root, legacyNetworkDir)
	newNetworkDirExists := conversationLogDirExists(filepath.Join(root, newNetworkDir))

	if legacyNetworkDir != newNetworkDir {
		sharedNetworkDir := conversationLogDirSegmentsShareLocation(root, legacyNetworkDir, newNetworkDir)
		if !sharedNetworkDir && legacyNetworkDirExists && !newNetworkDirExists {
			if err := os.Rename(filepath.Join(root, legacyNetworkDir), filepath.Join(root, newNetworkDir)); err == nil {
				networkDirPath = filepath.Join(root, newNetworkDir)
			}
		} else if newNetworkDirExists {
			networkDirPath = filepath.Join(root, newNetworkDir)
		}
	} else if newNetworkDirExists {
		networkDirPath = filepath.Join(root, newNetworkDir)
	}

	resolvedPath := newPath
	tryMigrateFile := func(legacyPath string) {
		if !conversationLogLegacyPathExists(legacyPath) {
			return
		}
		if conversationLogFileExists(newPath) || conversationLogPathsSameFile(legacyPath, newPath) {
			return
		}
		conversationLogPrepareTree(newPath)
		if err := os.Rename(legacyPath, newPath); err == nil {
			return
		}
		if conversationLogFileExists(legacyPath) {
			resolvedPath = legacyPath
		}
	}

	legacyPath := filepath.Join(networkDirPath, irc.LegacyStorageSegment(target))
	tryMigrateFile(legacyPath)

	if legacyNetworkDir != newNetworkDir && legacyNetworkDirExists {
		legacyPathInLegacyDir := filepath.Join(root, legacyNetworkDir, irc.LegacyStorageSegment(target))
		tryMigrateFile(legacyPathInLegacyDir)
	}

	return resolvedPath
}
