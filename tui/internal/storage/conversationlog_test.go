package storage

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

func conversationLogTestLine(body string) irc.TranscriptLine {
	return irc.TranscriptLine{
		Timestamp: time.Date(2026, 9, 4, 0, 0, 0, 0, time.UTC),
		Author:    "alice",
		Kind:      "chat",
		Body:      body,
	}
}

func conversationLogTestWrite(t *testing.T, path, contents string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatalf("mkdir %s: %v", filepath.Dir(path), err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o600); err != nil {
		t.Fatalf("write %s: %v", path, err)
	}
}

func conversationLogTestExists(path string) bool {
	info, err := os.Stat(path)
	return err == nil && !info.IsDir()
}

func conversationLogTestDirExists(path string) bool {
	info, err := os.Stat(path)
	return err == nil && info.IsDir()
}

func TestConversationLogPrependPreservesOrderOnDisk(t *testing.T) {
	log := NewConversationLog(t.TempDir())
	mapping := irc.CaseMapping{}
	log.Append("net", "#room", mapping, irc.TranscriptLine{Kind: "message", Body: "live", Author: "a"})
	log.Prepend("net", "#room", mapping, []irc.TranscriptLine{
		{Kind: "message", Body: "older", Author: "b"},
	})
	lines := log.ReadTail("net", "#room", mapping, 10)
	if len(lines) != 2 {
		t.Fatalf("ReadTail returned %d lines, want 2", len(lines))
	}
	if lines[0].Body != "older" || lines[1].Body != "live" {
		t.Fatalf("ReadTail = %+v, want older then live", lines)
	}
}

func TestConversationLogAppendReadTailRoundTrip(t *testing.T) {
	root := t.TempDir()
	log := NewConversationLog(root)
	mapping := irc.CaseMapping{}
	base := time.Date(2026, 9, 4, 0, 0, 0, 0, time.UTC)

	bodies := []string{"one", "two", "three"}
	for index, body := range bodies {
		line := conversationLogTestLine(body)
		line.Timestamp = base.Add(time.Duration(index) * time.Minute)
		if !log.Append("net", "#room", mapping, line) {
			t.Fatalf("Append(%q) failed", body)
		}
	}

	all := log.ReadTail("net", "#room", mapping, 10)
	if len(all) != 3 {
		t.Fatalf("ReadTail returned %d lines, want 3", len(all))
	}
	for index, body := range bodies {
		if all[index].Body != body {
			t.Errorf("line %d body = %q, want %q (oldest first)", index, all[index].Body, body)
		}
	}
	if !all[0].Timestamp.Equal(base) {
		t.Errorf("timestamp = %v, want %v", all[0].Timestamp, base)
	}
	if all[0].Author != "alice" || all[0].Kind != "chat" {
		t.Errorf("author/kind = %q/%q, want alice/chat", all[0].Author, all[0].Kind)
	}

	tail := log.ReadTail("net", "#room", mapping, 2)
	if len(tail) != 2 || tail[0].Body != "two" || tail[1].Body != "three" {
		t.Fatalf("ReadTail(maxLines=2) = %+v, want [two three]", tail)
	}

	if log.ReadTail("net", "#room", mapping, 0) != nil {
		t.Error("ReadTail(0) must return nil")
	}
	if log.ReadTail("net", "#room", mapping, -1) != nil {
		t.Error("ReadTail(-1) must return nil")
	}
	if log.ReadTail("net", "missing", mapping, 5) != nil {
		t.Error("ReadTail on a missing conversation must return nil")
	}
}

func TestConversationLogReadTailSpansChunkBoundary(t *testing.T) {
	log := NewConversationLog(t.TempDir())
	mapping := irc.CaseMapping{}
	big := strings.Repeat("x", 5000)

	if !log.Append("net", "#room", mapping, conversationLogTestLine("older")) {
		t.Fatal("Append(older) failed")
	}
	if !log.Append("net", "#room", mapping, conversationLogTestLine(big)) {
		t.Fatal("Append(big) failed")
	}

	lines := log.ReadTail("net", "#room", mapping, 2)
	if len(lines) != 2 {
		t.Fatalf("ReadTail returned %d lines, want 2", len(lines))
	}
	if lines[0].Body != "older" || lines[1].Body != big {
		t.Fatalf("ReadTail across the 4096-byte boundary = %q/%d bytes, want older/long", lines[0].Body, len(lines[1].Body))
	}
}

func TestConversationLogPathForPreservesTargetCase(t *testing.T) {
	root := t.TempDir()
	log := NewConversationLog(root)
	mapping := irc.CaseMapping{}

	upper := log.PathFor("freenode", "#Omarchy", mapping)
	want := filepath.Join(root, irc.OmaircStorageSegment("freenode"), irc.OmaircWireStorageSegment("#Omarchy"))
	if upper != want {
		t.Fatalf("PathFor = %q, want %q", upper, want)
	}
	if filepath.Base(filepath.Dir(upper)) != irc.OmaircStorageSegment("freenode") {
		t.Errorf("network dir = %q, want OmaircStorageSegment(freenode)", filepath.Base(filepath.Dir(upper)))
	}
	if filepath.Base(upper) != irc.OmaircWireStorageSegment("#Omarchy") {
		t.Errorf("file = %q, want OmaircWireStorageSegment(#Omarchy)", filepath.Base(upper))
	}

	lower := log.PathFor("freenode", "#omarchy", mapping)
	if upper == lower {
		t.Error("PathFor case-mapped the target; it must preserve the wire case")
	}
	mapped := filepath.Join(root, irc.OmaircStorageSegment("freenode"), irc.OmaircTargetSegment("#Omarchy", mapping))
	if upper == mapped {
		t.Error("PathFor used the case-mapped target segment")
	}
}

func TestConversationLogMigratesLegacyTranscriptPaths(t *testing.T) {
	root := t.TempDir()
	legacyDir := filepath.Join(root, irc.LegacyStorageSegment("Freenode"))
	legacyFile := filepath.Join(legacyDir, irc.LegacyStorageSegment("#a/b"))
	conversationLogTestWrite(t, legacyFile, "legacy\n")

	log := NewConversationLog(root)
	mapping := irc.CaseMapping{}
	path := log.PathFor("Freenode", "#a/b", mapping)
	want := filepath.Join(root, irc.OmaircStorageSegment("Freenode"), irc.OmaircWireStorageSegment("#a/b"))
	if path != want {
		t.Fatalf("PathFor = %q, want %q", path, want)
	}
	if !conversationLogTestExists(path) {
		t.Errorf("migrated file %q does not exist", path)
	}
	if conversationLogTestExists(legacyFile) {
		t.Errorf("legacy file %q still exists", legacyFile)
	}
	if conversationLogTestDirExists(legacyDir) {
		t.Errorf("legacy network dir %q still exists", legacyDir)
	}

	if again := log.PathFor("Freenode", "#a/b", mapping); again != want {
		t.Errorf("second PathFor = %q, want %q (migration must be idempotent)", again, want)
	}
}

func TestConversationLogDoesNotOverwriteExistingTranscriptDuringMigration(t *testing.T) {
	root := t.TempDir()
	legacyDir := filepath.Join(root, irc.LegacyStorageSegment("Freenode"))
	legacyFile := filepath.Join(legacyDir, irc.LegacyStorageSegment("#a/b"))
	conversationLogTestWrite(t, legacyFile, "legacy\n")

	newPath := filepath.Join(root, irc.OmaircStorageSegment("Freenode"), irc.OmaircWireStorageSegment("#a/b"))
	conversationLogTestWrite(t, newPath, "newer\n")

	log := NewConversationLog(root)
	mapping := irc.CaseMapping{}
	if path := log.PathFor("Freenode", "#a/b", mapping); path != newPath {
		t.Fatalf("PathFor = %q, want %q", path, newPath)
	}
	if !conversationLogTestExists(legacyFile) {
		t.Errorf("legacy file %q must be preserved when the new path exists", legacyFile)
	}
	contents, err := os.ReadFile(newPath)
	if err != nil {
		t.Fatalf("read %s: %v", newPath, err)
	}
	if string(contents) != "newer\n" {
		t.Fatalf("new file = %q, want %q (must not be overwritten)", contents, "newer\n")
	}
	if again := log.PathFor("Freenode", "#a/b", mapping); again != newPath {
		t.Errorf("second PathFor = %q, want %q", again, newPath)
	}
}

func TestConversationLogSkipsRejectedTranscriptLines(t *testing.T) {
	features := irc.NewServerFeatures()
	channelTypes := features.ChannelTypes()
	rejected := "PRIVMSG NickServ :IDENTIFY swordfish"
	if irc.AllowsTranscript(rejected, channelTypes) {
		t.Fatalf("test sample %q is unexpectedly allowed by the secret policy", rejected)
	}

	log := NewConversationLog(t.TempDir())
	mapping := irc.CaseMapping{}
	line := conversationLogTestLine(rejected)
	if !log.Append("net", "#room", mapping, line) {
		t.Error("Append must report success for a filtered line")
	}
	if lines := log.ReadTail("net", "#room", mapping, 10); len(lines) != 0 {
		t.Errorf("ReadTail returned %d lines for a filtered append, want 0", len(lines))
	}
	if conversationLogTestExists(log.PathFor("net", "#room", mapping)) {
		t.Error("a filtered append must not create the transcript file")
	}

	if !log.Append("net", "#room", mapping, irc.TranscriptLine{Body: "no kind"}) {
		t.Error("Append with an empty kind must be a successful no-op")
	}
	if !log.Append("net", "#room", mapping, irc.TranscriptLine{Kind: "chat"}) {
		t.Error("Append with an empty body must be a successful no-op")
	}
}

func TestConversationLogAppendTightensPermissions(t *testing.T) {
	root := t.TempDir()
	log := NewConversationLog(root)
	mapping := irc.CaseMapping{}
	if !log.Append("net", "#room", mapping, conversationLogTestLine("hello")) {
		t.Fatal("Append failed")
	}
	path := log.PathFor("net", "#room", mapping)

	fileInfo, err := os.Stat(path)
	if err != nil {
		t.Fatalf("stat %s: %v", path, err)
	}
	if runtime.GOOS != "windows" {
		if got := fileInfo.Mode().Perm(); got != 0o600 {
			t.Errorf("file mode = %o, want 600", got)
		}
	}
	dirInfo, err := os.Stat(filepath.Dir(path))
	if err != nil {
		t.Fatalf("stat %s: %v", filepath.Dir(path), err)
	}
	if runtime.GOOS != "windows" {
		if got := dirInfo.Mode().Perm(); got != 0o700 {
			t.Errorf("directory mode = %o, want 700", got)
		}
	}
}

func TestConversationLogWritesFlattenedJSONL(t *testing.T) {
	log := NewConversationLog(t.TempDir())
	mapping := irc.CaseMapping{}
	line := conversationLogTestLine("hello\nworld")
	line.Timestamp = time.Date(2026, 9, 4, 12, 30, 0, 0, time.UTC)
	line.Author = "alice\r\nbob"
	line.MsgID = "m1"
	if !log.Append("net", "#room", mapping, line) {
		t.Fatal("Append failed")
	}
	contents, err := os.ReadFile(log.PathFor("net", "#room", mapping))
	if err != nil {
		t.Fatalf("read transcript: %v", err)
	}
	text := string(contents)
	if strings.Count(text, "\n") != 1 || !strings.HasSuffix(text, "\n") {
		t.Fatalf("transcript must be one newline-terminated line, got %q", text)
	}
	for _, want := range []string{
		`"timestamp":"2026-09-04T12:30:00.000Z"`,
		`"author":"alice  bob"`,
		`"kind":"chat"`,
		`"body":"hello world"`,
		`"msgid":"m1"`,
	} {
		if !strings.Contains(text, want) {
			t.Errorf("transcript %q missing %s", text, want)
		}
	}

	if !log.Append("net", "#other", mapping, conversationLogTestLine("hi")) {
		t.Fatal("Append failed")
	}
	other, err := os.ReadFile(log.PathFor("net", "#other", mapping))
	if err != nil {
		t.Fatalf("read transcript: %v", err)
	}
	if strings.Contains(string(other), "msgid") {
		t.Errorf("msgid must be omitted when empty, got %q", other)
	}
}

func TestConversationLogUsesTranscriptRootOverride(t *testing.T) {
	root := t.TempDir()
	t.Setenv("OMAIRC_TRANSCRIPT_ROOT", root)

	log := NewConversationLog("")
	if log.Root() != root {
		t.Fatalf("NewConversationLog(\"\").Root() = %q, want %q", log.Root(), root)
	}
	log.SetRoot("/somewhere/else")
	if log.Root() != "/somewhere/else" {
		t.Fatalf("SetRoot did not update Root: %q", log.Root())
	}
}
