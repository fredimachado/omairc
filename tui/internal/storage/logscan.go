package storage

import (
	"context"
	"os"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// scanChunkSize is the bytes read per chunk of a full scan. Larger than the
// tail reader's: a scan reads far more of the file.
const scanChunkSize = 64 * 1024

// ScanLog hands each line of the log at path to visit, newest first, until
// visit returns false, the file runs out or ctx is cancelled. It mirrors
// scan_log in `client/src/storage/logscan.rs`. A missing or unreadable file has
// no lines. A line the transcript policy would not have stored (a credential)
// is skipped here, so no caller can forget the filter and a search never shows
// what the transcript would drop. A line that does not parse, such as one cut
// short by a crash, is skipped too.
func ScanLog(ctx context.Context, path string, visit func(irc.TranscriptLine) bool) {
	scanLogChunked(ctx, path, scanChunkSize, visit)
}

func scanLogChunked(ctx context.Context, path string, chunkSize int64, visit func(irc.TranscriptLine) bool) {
	file, err := os.Open(path)
	if err != nil {
		return
	}
	defer file.Close()
	channelTypes := conversationLogChannelTypes()
	conversationLogReverseLines(ctx, file, chunkSize, func(raw []byte) bool {
		line, ok := conversationLogAdmittedLine(raw, channelTypes)
		if !ok {
			return true
		}
		return visit(line)
	})
}
