package storage

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

func scanRecord(n int, author, body string) string {
	raw, _ := json.Marshal(map[string]string{
		"timestamp": fmt.Sprintf("2026-09-04T00:%02d:%02d.000Z", n/60, n%60),
		"author":    author,
		"kind":      "message",
		"body":      body,
	})
	return string(raw)
}

func writeScanLog(t *testing.T, lines []string, trailingNewline bool) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "net", "#room")
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatal(err)
	}
	text := strings.Join(lines, "\n")
	if trailingNewline {
		text += "\n"
	}
	if err := os.WriteFile(path, []byte(text), 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

func scanBodies(path string, chunk int64) []string {
	var seen []string
	scanLogChunked(context.Background(), path, chunk, func(line irc.TranscriptLine) bool {
		seen = append(seen, line.Body)
		return true
	})
	return seen
}

func numberedRecords(count int, suffix string) ([]string, []string) {
	var records, newestFirst []string
	for n := 0; n < count; n++ {
		body := fmt.Sprintf("line %d%s", n, suffix)
		records = append(records, scanRecord(n, "anna", body))
		newestFirst = append([]string{body}, newestFirst...)
	}
	return records, newestFirst
}

func TestScanLogNewestFirst(t *testing.T) {
	records, want := numberedRecords(5, "")
	got := scanBodies(writeScanLog(t, records, true), 4096)
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %v", got)
	}
}

func TestScanLogSeesEveryLineWhateverTheChunkSize(t *testing.T) {
	records, want := numberedRecords(40, " é")
	path := writeScanLog(t, records, true)
	for _, chunk := range []int64{1, 2, 3, 7, 64, 101, 4096} {
		if got := scanBodies(path, chunk); !reflect.DeepEqual(got, want) {
			t.Fatalf("chunk %d: got %v", chunk, got)
		}
	}
}

func TestScanLogWithoutFinalNewline(t *testing.T) {
	path := writeScanLog(t, []string{scanRecord(0, "anna", "first"), scanRecord(1, "anna", "last")}, false)
	if got := scanBodies(path, 16); !reflect.DeepEqual(got, []string{"last", "first"}) {
		t.Fatalf("got %v", got)
	}
}

func TestScanLogSkipsALineCutShortByACrash(t *testing.T) {
	cut := scanRecord(2, "anna", "cut off mid-write")
	path := writeScanLog(t, []string{scanRecord(0, "anna", "old"), scanRecord(1, "anna", "whole"), cut[:len(cut)-9]}, false)
	if got := scanBodies(path, 32); !reflect.DeepEqual(got, []string{"whole", "old"}) {
		t.Fatalf("got %v", got)
	}
}

func TestScanLogSkipsBlankAndGarbageLines(t *testing.T) {
	path := writeScanLog(t, []string{scanRecord(0, "anna", "one"), "", "not json", "   ", scanRecord(1, "anna", "two")}, true)
	if got := scanBodies(path, 4096); !reflect.DeepEqual(got, []string{"two", "one"}) {
		t.Fatalf("got %v", got)
	}
}

func TestScanLogMissingAndEmptyFilesHaveNoLines(t *testing.T) {
	if got := scanBodies(filepath.Join(t.TempDir(), "nothing"), 4096); len(got) != 0 {
		t.Fatalf("missing: %v", got)
	}
	if got := scanBodies(writeScanLog(t, nil, false), 4096); len(got) != 0 {
		t.Fatalf("empty: %v", got)
	}
}

func TestScanLogNeverHandsOverACredential(t *testing.T) {
	path := writeScanLog(t, []string{
		scanRecord(0, "anna", "nothing secret"),
		scanRecord(1, "fred", "IDENTIFY hunter2"),
		scanRecord(2, "fred", "PRIVMSG NickServ :IDENTIFY hunter2"),
		scanRecord(4, "anna", "still nothing secret"),
	}, true)
	got := scanBodies(path, 4096)
	for _, body := range got {
		if strings.Contains(body, "IDENTIFY") {
			t.Fatalf("a credential came back: %v", got)
		}
	}
	if len(got) < 2 {
		t.Fatalf("lost the ordinary lines: %v", got)
	}
}

func TestScanLogStopsWhenTheCallerStops(t *testing.T) {
	records, _ := numberedRecords(10, "")
	count := 0
	scanLogChunked(context.Background(), writeScanLog(t, records, true), 16, func(irc.TranscriptLine) bool {
		count++
		return count < 3
	})
	if count != 3 {
		t.Fatalf("count=%d", count)
	}
}

func TestScanLogCancelStopsAtOnce(t *testing.T) {
	records, _ := numberedRecords(50, "")
	path := writeScanLog(t, records, true)
	ctx, cancel := context.WithCancel(context.Background())
	count := 0
	scanLogChunked(ctx, path, 64, func(irc.TranscriptLine) bool {
		count++
		if count == 4 {
			cancel()
		}
		return true
	})
	if count != 4 {
		t.Fatalf("a line was visited after the cancel: %d", count)
	}
	done, stop := context.WithCancel(context.Background())
	stop()
	none := 0
	scanLogChunked(done, path, 64, func(irc.TranscriptLine) bool { none++; return true })
	if none != 0 {
		t.Fatalf("an already-cancelled scan visited %d lines", none)
	}
}
