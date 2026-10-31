package storage

import (
	"bytes"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

func tempSettingsPath(t *testing.T) string {
	t.Helper()
	return filepath.Join(t.TempDir(), "omairc", "omairc.conf")
}

func TestSettingsStringRoundTrip(t *testing.T) {
	t.Parallel()

	path := tempSettingsPath(t)
	s := OpenSettings(path)
	tricky := map[string]string{
		"spaces":    "  leading and trailing  ",
		"semi":      "a;b",
		"equals":    "a=b",
		"comma":     "a,b",
		"hash":      "a#b",
		"quote":     `a"b`,
		"backslash": `a\b`,
		"newline":   "a\nb",
		"tab":       "a\tb",
		"unicode":   "café-ünï-日本",
		"json":      `[{"target":"a,b","ms":"1"}]`,
		"empty":     "",
	}
	for key, value := range tricky {
		s.SetValue("strings", key, value)
	}
	if status := s.Sync(); status != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", status)
	}

	reopened := OpenSettings(path)
	for key, want := range tricky {
		if got := reopened.Value("strings", key); got != want {
			t.Errorf("Value(strings, %q) = %q, want %q", key, got, want)
		}
	}

	// Missing keys read as empty and are distinguishable from empty values.
	if got := reopened.Value("strings", "absent"); got != "" {
		t.Errorf("Value(strings, absent) = %q, want empty", got)
	}
	if got := reopened.Value("", "strings"); got != "" {
		t.Errorf("Value with a bad group = %q, want empty", got)
	}
}

func TestSettingsScalarRoundTrip(t *testing.T) {
	t.Parallel()

	path := tempSettingsPath(t)
	s := OpenSettings(path)
	s.SetBool("scalars", "on", true)
	s.SetBool("scalars", "off", false)
	s.SetUint("scalars", "port", 6697)
	s.SetInt("scalars", "icon", 3)
	if status := s.Sync(); status != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", status)
	}

	reopened := OpenSettings(path)
	if !reopened.Bool("scalars", "on", false) {
		t.Error("Bool(on) = false, want true")
	}
	if reopened.Bool("scalars", "off", true) {
		t.Error("Bool(off) = true, want false")
	}
	if got := reopened.Uint("scalars", "port", 0); got != 6697 {
		t.Errorf("Uint(port) = %d, want 6697", got)
	}
	if got := reopened.Int("scalars", "icon", 0); got != 3 {
		t.Errorf("Int(icon) = %d, want 3", got)
	}

	// Fallbacks apply when the key is absent or unparseable.
	if got := reopened.Bool("scalars", "missing", true); !got {
		t.Error("Bool(missing, true) = false, want fallback true")
	}
	if got := reopened.Uint("scalars", "on", 5); got != 5 {
		t.Errorf("Uint(on, 5) = %d, want fallback 5", got)
	}
	if got := reopened.Int("scalars", "missing", 9); got != 9 {
		t.Errorf("Int(missing, 9) = %d, want fallback 9", got)
	}
}

func TestSettingsBoolCaseInsensitive(t *testing.T) {
	t.Parallel()

	path := tempSettingsPath(t)
	s := OpenSettings(path)
	s.SetValue("b", "upper", "TRUE")
	s.SetValue("b", "digit", "1")
	s.SetValue("b", "nope", "yes")
	if status := s.Sync(); status != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", status)
	}

	reopened := OpenSettings(path)
	if !reopened.Bool("b", "upper", false) {
		t.Error(`Bool("TRUE") = false, want true`)
	}
	if !reopened.Bool("b", "digit", false) {
		t.Error(`Bool("1") = false, want true`)
	}
	if reopened.Bool("b", "nope", true) != true {
		t.Error(`Bool("yes", true) = false, want fallback true`)
	}
}

func TestSettingsStringListRoundTrip(t *testing.T) {
	t.Parallel()

	path := tempSettingsPath(t)
	s := OpenSettings(path)
	s.SetStringList("lists", "autojoin", []string{"#a", "#b"})
	s.SetStringList("lists", "commas", []string{"a,b", `c"d`})
	s.SetStringList("lists", "single", []string{"freenode"})
	s.SetStringList("lists", "empty", nil)
	s.SetValue("lists", "emptyString", "")
	if status := s.Sync(); status != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", status)
	}

	reopened := OpenSettings(path)
	cases := []struct {
		key  string
		want []string
	}{
		{"autojoin", []string{"#a", "#b"}},
		{"commas", []string{"a,b", `c"d`}},
		{"single", []string{"freenode"}},
		{"empty", []string{}},
		{"emptyString", []string{}},
	}
	for _, tc := range cases {
		got := reopened.StringList("lists", tc.key)
		if !equalStrings(got, tc.want) {
			t.Errorf("StringList(lists, %q) = %#v, want %#v", tc.key, got, tc.want)
		}
	}
	if got := reopened.StringList("lists", "absent"); got != nil {
		t.Errorf("StringList(absent) = %#v, want nil", got)
	}
	if got := reopened.Value("lists", "autojoin"); got != "" {
		t.Errorf("Value on a list = %q, want empty (QVariant::toString of a list)", got)
	}
}

func TestSettingsNestedGroups(t *testing.T) {
	t.Parallel()

	path := tempSettingsPath(t)
	s := OpenSettings(path)
	s.SetValue("networks/abc", "host", "chat.freenode.net")
	s.SetValue("networks/abc", "port", "6697")
	s.SetValue("networks/abc/deep", "host", "irc.deep.net")
	s.SetValue("networks/def", "host", "irc.oftc.net")
	s.SetValue("preferences", "directs", "true")
	s.SetValue("", "topLevel", "yes")

	if got := s.ChildGroups(""); !equalStrings(got, []string{"networks", "preferences"}) {
		t.Errorf("ChildGroups(\"\") = %#v, want [networks preferences]", got)
	}
	if got := s.ChildGroups("networks"); !equalStrings(got, []string{"abc", "def"}) {
		t.Errorf("ChildGroups(networks) = %#v, want [abc def]", got)
	}
	if got := s.ChildGroups("networks/abc"); !equalStrings(got, []string{"deep"}) {
		t.Errorf("ChildGroups(networks/abc) = %#v, want [deep]", got)
	}
	if got := s.Value("", "topLevel"); got != "yes" {
		t.Errorf("top-level Value = %q, want yes", got)
	}

	s.RemoveKey("networks/abc", "port")
	if got := s.Value("networks/abc", "port"); got != "" {
		t.Errorf("Value after RemoveKey = %q, want empty", got)
	}
	if got := s.Value("networks/abc", "host"); got != "chat.freenode.net" {
		t.Errorf("sibling key after RemoveKey = %q, want preserved", got)
	}

	s.RemoveKey("networks/abc", "absent") // no-op, must not panic

	s.RemoveGroup("networks/abc")
	if got := s.Value("networks/abc", "host"); got != "" {
		t.Errorf("Value after RemoveGroup = %q, want empty", got)
	}
	if got := s.Value("networks/abc/deep", "host"); got != "" {
		t.Errorf("child after RemoveGroup = %q, want empty", got)
	}
	if got := s.Value("networks/def", "host"); got != "irc.oftc.net" {
		t.Errorf("sibling group after RemoveGroup = %q, want preserved", got)
	}

	if status := s.Sync(); status != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", status)
	}
	reopened := OpenSettings(path)
	if got := reopened.Value("networks/def", "host"); got != "irc.oftc.net" {
		t.Errorf("persisted sibling = %q, want irc.oftc.net", got)
	}
}

func TestSyncEscapingMatchesQSettings(t *testing.T) {
	t.Parallel()

	path := tempSettingsPath(t)
	s := OpenSettings(path)
	s.SetValue("tricky", "backslash", `a\b`)
	s.SetValue("tricky", "comma", "a,b")
	s.SetValue("tricky", "empty_str", "")
	s.SetValue("tricky", "equals", "a=b")
	s.SetValue("tricky", "hash", "a#b")
	s.SetValue("tricky", "lead", " x")
	s.SetValue("tricky", "newline", "a\nb")
	s.SetValue("tricky", "quote", `a"b`)
	s.SetValue("tricky", "semi", "a;b")
	s.SetValue("tricky", "trail", "x ")
	s.SetValue("tricky", "unicode", "café日本")
	s.SetStringList("tricky", "list", []string{"a,b", `c"d`})
	s.SetStringList("tricky", "empty_list", nil)

	if status := s.Sync(); status != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", status)
	}

	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	text := string(raw)

	// Expectations captured from Qt 6.8 QSettings(NativeFormat) output.
	wantLines := []string{
		"[tricky]",
		`backslash=a\\b`,
		`comma="a,b"`,
		`empty_list=@Invalid()`,
		`empty_str=`,
		`equals="a=b"`,
		`hash=a#b`,
		`lead=" x"`,
		`list="a,b", c\"d`,
		`newline=a\nb`,
		`quote=a\"b`,
		`semi="a;b"`,
		`trail="x "`,
		`unicode=café日本`,
	}
	for _, line := range wantLines {
		if !strings.Contains(text, line+"\n") {
			t.Errorf("file does not contain QSettings line %q\nfile:\n%s", line, text)
		}
	}

	reopened := OpenSettings(path)
	if got := reopened.Value("tricky", "empty_list"); got != "" {
		t.Errorf("empty list Value = %q, want empty", got)
	}
	if got := reopened.StringList("tricky", "list"); !equalStrings(got, []string{"a,b", `c"d`}) {
		t.Errorf("list after reopen = %#v", got)
	}
	if got := reopened.Value("tricky", "unicode"); got != "café日本" {
		t.Errorf("unicode after reopen = %q", got)
	}
}

func TestSettingsReadsQSettingsFixture(t *testing.T) {
	t.Parallel()

	fixtures := map[string]string{
		// QSettings' canonical layout: top-level section + escaped key path.
		"canonical": "[networks]\n" +
			`abc\host=chat.freenode.net` + "\n" +
			`abc\port=6697` + "\n" +
			`abc\tls=true` + "\n" +
			"abc\\autojoin=#a, #b\n" +
			`abc\playback="[{\"target\":\"a,b\",\"ms\":\"1\"}]"` + "\n" +
			"\n[preferences]\ndirects=true\nnetworkOrder=freenode\n",
		// A hand-written nested section QSettings also accepts.
		"nested-section": "[networks/abc]\n" +
			"host=chat.freenode.net\nport=6697\ntls=true\nautojoin=#a, #b\n" +
			"playback=\"[{\\\"target\\\":\\\"a,b\\\",\\\"ms\\\":\\\"1\\\"}]\"\n" +
			"\n[preferences]\ndirects=true\nnetworkOrder=freenode\n",
	}

	for name, fixture := range fixtures {
		t.Run(name, func(t *testing.T) {
			path := tempSettingsPath(t)
			if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
				t.Fatalf("MkdirAll: %v", err)
			}
			if err := os.WriteFile(path, []byte(fixture), 0o600); err != nil {
				t.Fatalf("WriteFile: %v", err)
			}
			if probe := ProbeSettingsIni(path); probe != IniProbeOK {
				t.Fatalf("ProbeSettingsIni = %v, want IniProbeOK", probe)
			}

			s := OpenSettings(path)
			if got := s.Value("networks/abc", "host"); got != "chat.freenode.net" {
				t.Errorf("host = %q", got)
			}
			if got := s.Uint("networks/abc", "port", 0); got != 6697 {
				t.Errorf("port = %d", got)
			}
			if !s.Bool("networks/abc", "tls", false) {
				t.Error("tls = false, want true")
			}
			if got := s.StringList("networks/abc", "autojoin"); !equalStrings(got, []string{"#a", "#b"}) {
				t.Errorf("autojoin = %#v", got)
			}
			if got := s.Value("networks/abc", "playback"); got != `[{"target":"a,b","ms":"1"}]` {
				t.Errorf("playback = %q", got)
			}
			if !s.Bool("preferences", "directs", false) {
				t.Error("preferences/directs = false, want true")
			}
			if got := s.StringList("preferences", "networkOrder"); !equalStrings(got, []string{"freenode"}) {
				t.Errorf("networkOrder = %#v", got)
			}
			if got := s.ChildGroups(""); !equalStrings(got, []string{"networks", "preferences"}) {
				t.Errorf("ChildGroups(\"\") = %#v", got)
			}
			if got := s.ChildGroups("networks"); !equalStrings(got, []string{"abc"}) {
				t.Errorf("ChildGroups(networks) = %#v", got)
			}
		})
	}
}

func TestProbeSettingsIni(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	missing := filepath.Join(dir, "missing.conf")
	if got := ProbeSettingsIni(missing); got != IniProbeOK {
		t.Errorf("missing file probe = %v, want IniProbeOK", got)
	}
	if got := ProbeSettingsIni(""); got != IniProbeOK {
		t.Errorf("empty path probe = %v, want IniProbeOK", got)
	}

	valid := filepath.Join(dir, "valid.conf")
	if err := os.WriteFile(valid, []byte("[networks]\nabc\\host=example\n"), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if got := ProbeSettingsIni(valid); got != IniProbeOK {
		t.Errorf("valid file probe = %v, want IniProbeOK", got)
	}

	commentOnly := filepath.Join(dir, "comment.conf")
	if err := os.WriteFile(commentOnly, []byte(";just a comment\n"), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if got := ProbeSettingsIni(commentOnly); got != IniProbeOK {
		t.Errorf("comment-only probe = %v, want IniProbeOK", got)
	}

	hashComment := filepath.Join(dir, "hash-comment.conf")
	if err := os.WriteFile(hashComment, []byte("# keep me\n   # note\n"), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if got := ProbeSettingsIni(hashComment); got != IniProbeOK {
		t.Errorf("hash comment probe = %v, want IniProbeOK", got)
	}

	unclosed := filepath.Join(dir, "unclosed.conf")
	if err := os.WriteFile(unclosed, []byte("[unclosed\nkey=x\n"), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if got := ProbeSettingsIni(unclosed); got != IniProbeMalformed {
		t.Errorf("unclosed section probe = %v, want IniProbeMalformed", got)
	}

	noEquals := filepath.Join(dir, "noequals.conf")
	if err := os.WriteFile(noEquals, []byte("[group]\nkey\n"), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if got := ProbeSettingsIni(noEquals); got != IniProbeMalformed {
		t.Errorf("missing '=' probe = %v, want IniProbeMalformed", got)
	}

	unreadable := filepath.Join(dir, "unreadable.conf")
	if err := os.WriteFile(unreadable, []byte("[networks]\nabc\\host=example\n"), 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := os.Chmod(unreadable, 0o000); err != nil {
		t.Fatalf("Chmod: %v", err)
	}
	if _, err := os.ReadFile(unreadable); err == nil {
		t.Skip("process can read a 0000 file (running as root?); skipping unreadable probe")
	}
	if got := ProbeSettingsIni(unreadable); got != IniProbeUnreadable {
		t.Errorf("unreadable file probe = %v, want IniProbeUnreadable", got)
	}
}

func TestHashFullLineCommentsLoadAndSave(t *testing.T) {
	t.Parallel()

	path := tempSettingsPath(t)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	original := "# keep me\n   # note\n[preferences]\ndirects=false\nchannel=#omarchy\ncolor=#ff0000\n"
	if err := os.WriteFile(path, []byte(original), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := os.Chmod(path, 0o644); err != nil {
		t.Fatalf("Chmod: %v", err)
	}

	if got := ProbeSettingsIni(path); got != IniProbeOK {
		t.Fatalf("ProbeSettingsIni = %v, want IniProbeOK", got)
	}
	s := OpenSettings(path)
	if got := s.WriteBlocked(); got != StatusWritten {
		t.Fatalf("WriteBlocked = %v, want StatusWritten", got)
	}
	if s.Bool("preferences", "directs", true) {
		t.Fatal("directs should load as false")
	}
	if got := s.Value("preferences", "channel"); got != "#omarchy" {
		t.Fatalf("channel = %q, want #omarchy", got)
	}
	if got := s.Value("preferences", "color"); got != "#ff0000" {
		t.Fatalf("color = %q, want #ff0000", got)
	}

	s.SetBool("preferences", "directs", true)
	if got := s.Sync(); got != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", got)
	}
	if !s.Bool("preferences", "directs", false) {
		t.Fatal("directs should be true after save")
	}

	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	body := string(after)
	if !strings.Contains(body, "directs=true\n") {
		t.Fatalf("missing directs=true after save:\n%s", body)
	}
	if !strings.Contains(body, "channel=#omarchy\n") {
		t.Fatalf("channel with # must round-trip:\n%s", body)
	}
	if !strings.Contains(body, "color=#ff0000\n") {
		t.Fatalf("color with # must round-trip:\n%s", body)
	}
	if strings.Contains(body, "# keep me") || strings.Contains(body, "# note") {
		t.Fatalf("full-line # comments are not preserved on write:\n%s", body)
	}
}

func TestSyncRefusesMalformedFile(t *testing.T) {
	t.Parallel()

	path := tempSettingsPath(t)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	corrupt := []byte("[networks]\nabc\\host\n")
	if err := os.WriteFile(path, corrupt, 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}

	s := OpenSettings(path)
	s.SetValue("networks/abc", "host", "irc.changed.example")
	if got := s.Sync(); got != StatusFormatError {
		t.Fatalf("Sync() = %v, want StatusFormatError", got)
	}
	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(after) != string(corrupt) {
		t.Errorf("malformed file was modified:\n%s", after)
	}
}

func TestSyncRefusesReadOnlyFile(t *testing.T) {
	t.Parallel()

	path := tempSettingsPath(t)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	original := []byte("[networks]\nabc\\host=chat.freenode.net\n")
	if err := os.WriteFile(path, original, 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := os.Chmod(path, 0o444); err != nil {
		t.Fatalf("Chmod: %v", err)
	}
	if f, err := os.OpenFile(path, os.O_WRONLY, 0); err == nil {
		f.Close()
		t.Skip("process can write a read-only file (running as root?); skipping")
	}

	s := OpenSettings(path)
	s.SetValue("networks/abc", "host", "irc.changed.example")
	if got := s.Sync(); got != StatusAccessError {
		t.Fatalf("Sync() = %v, want StatusAccessError", got)
	}
	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(after) != string(original) {
		t.Errorf("read-only file was modified:\n%s", after)
	}
}

func TestSyncRefusesReadOnlyMalformedFileWithFormatError(t *testing.T) {
	t.Parallel()

	path := tempSettingsPath(t)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	corrupt := []byte("[networks]\nabc\\host\n")
	if err := os.WriteFile(path, corrupt, 0o600); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := os.Chmod(path, 0o444); err != nil {
		t.Fatalf("Chmod: %v", err)
	}
	if f, err := os.OpenFile(path, os.O_WRONLY, 0); err == nil {
		f.Close()
		_ = os.Chmod(path, 0o644)
		t.Skip("process can write a read-only file (running as root?); skipping")
	}

	s := OpenSettings(path)
	s.SetValue("networks/abc", "host", "irc.changed.example")
	if got := s.Sync(); got != StatusFormatError {
		_ = os.Chmod(path, 0o644)
		t.Fatalf("Sync() = %v, want StatusFormatError", got)
	}
	_ = os.Chmod(path, 0o644)
	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(after) != string(corrupt) {
		t.Errorf("read-only malformed file was modified:\n%s", after)
	}
}

func TestSyncPreservesExistingFileMode(t *testing.T) {
	t.Parallel()
	if runtime.GOOS == "windows" {
		t.Skip("unix file modes")
	}

	path := tempSettingsPath(t)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, []byte("[preferences]\ndirects=false\n"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := os.Chmod(path, 0o644); err != nil {
		t.Fatalf("Chmod: %v", err)
	}

	s := OpenSettings(path)
	s.SetBool("preferences", "directs", true)
	if got := s.Sync(); got != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", got)
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("Stat: %v", err)
	}
	if perm := info.Mode().Perm(); perm != 0o644 {
		t.Errorf("permissions = %o, want 644", perm)
	}
	if !OpenSettings(path).Bool("preferences", "directs", false) {
		t.Error("directs did not round-trip")
	}
}

func TestSyncCreatesMissingFile(t *testing.T) {
	t.Parallel()

	path := tempSettingsPath(t)
	s := OpenSettings(path)
	s.SetValue("preferences", "directs", "true")
	if got := s.Sync(); got != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", got)
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("Stat: %v", err)
	}
	if runtime.GOOS != "windows" {
		if perm := info.Mode().Perm(); perm != 0o600 {
			t.Errorf("permissions = %o, want 600", perm)
		}
	}
	reopened := OpenSettings(path)
	if !reopened.Bool("preferences", "directs", false) {
		t.Error("directs did not round-trip")
	}
}

func TestSyncNoChangesDoesNotCreateFile(t *testing.T) {
	t.Parallel()

	path := tempSettingsPath(t)
	s := OpenSettings(path)
	if got := s.Sync(); got != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", got)
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Errorf("Sync with no changes created a file (stat err = %v)", err)
	}
}

func TestSyncRepeatAssignmentPreservesFileBytesAndMtime(t *testing.T) {
	t.Parallel()
	if runtime.GOOS == "windows" {
		t.Skip("unix mtime and mode")
	}

	path := tempSettingsPath(t)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	original := []byte("[preferences]\ndirects=true\n")
	if err := os.WriteFile(path, original, 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := os.Chmod(path, 0o644); err != nil {
		t.Fatalf("Chmod: %v", err)
	}

	beforeBytes, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	beforeInfo, err := os.Stat(path)
	if err != nil {
		t.Fatalf("Stat: %v", err)
	}
	time.Sleep(1100 * time.Millisecond)

	s := OpenSettings(path)
	s.SetBool("preferences", "directs", true)
	if got := s.Sync(); got != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", got)
	}
	afterBytes, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(afterBytes) != string(beforeBytes) {
		t.Errorf("no-op sync changed bytes:\n%s", afterBytes)
	}
	afterInfo, err := os.Stat(path)
	if err != nil {
		t.Fatalf("Stat: %v", err)
	}
	if !afterInfo.ModTime().Equal(beforeInfo.ModTime()) {
		t.Errorf("no-op sync changed mtime: before %v after %v", beforeInfo.ModTime(), afterInfo.ModTime())
	}

	s.SetBool("preferences", "directs", false)
	if got := s.Sync(); got != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", got)
	}
	changed, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if string(changed) == string(beforeBytes) {
		t.Fatal("real change did not update file bytes")
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("Stat: %v", err)
	}
	if perm := info.Mode().Perm(); perm != 0o644 {
		t.Errorf("permissions = %o, want 644", perm)
	}
	if OpenSettings(path).Bool("preferences", "directs", true) {
		t.Error("directs did not round-trip as false")
	}
}

func syncLeavesDiskUnchanged(t *testing.T, path string, s *Settings, mutate func()) bool {
	t.Helper()
	if runtime.GOOS == "windows" {
		t.Skip("unix mtime")
	}
	beforeBytes, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	beforeInfo, err := os.Stat(path)
	if err != nil {
		t.Fatalf("Stat: %v", err)
	}
	time.Sleep(1100 * time.Millisecond)
	mutate()
	if got := s.Sync(); got != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", got)
	}
	afterBytes, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	afterInfo, err := os.Stat(path)
	if err != nil {
		t.Fatalf("Stat: %v", err)
	}
	return bytes.Equal(beforeBytes, afterBytes) && afterInfo.ModTime().Equal(beforeInfo.ModTime())
}

func TestSyncStringListAfterReopenSkipsUnchangedBytes(t *testing.T) {
	t.Parallel()
	if runtime.GOOS == "windows" {
		t.Skip("unix mtime")
	}

	path := tempSettingsPath(t)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, []byte("[preferences]\nnetworkOrder=@Invalid()\ncollapsed=@Invalid()\n"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := os.Chmod(path, 0o644); err != nil {
		t.Fatalf("Chmod: %v", err)
	}
	s := OpenSettings(path)
	if !syncLeavesDiskUnchanged(t, path, s, func() {
		s.SetStringList("preferences", "networkOrder", nil)
		s.SetStringList("preferences", "collapsed", nil)
	}) {
		t.Fatal("empty lists after reopen rewrote the file")
	}

	if err := os.WriteFile(path, []byte("[preferences]\nnetworkOrder=freenode\n"), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	s = OpenSettings(path)
	if !syncLeavesDiskUnchanged(t, path, s, func() {
		s.SetStringList("preferences", "networkOrder", []string{"freenode"})
	}) {
		t.Fatal("one-item list after reopen rewrote the file")
	}
}

func markDirtyNoop(s *Settings) {
	s.SetValue("_sync_test", "noop", "1")
	s.RemoveKey("_sync_test", "noop")
}

func TestSyncOnEmptyFileLeavesDiskUnchanged(t *testing.T) {
	t.Parallel()
	if runtime.GOOS == "windows" {
		t.Skip("unix mtime")
	}

	path := tempSettingsPath(t)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, nil, 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := os.Chmod(path, 0o644); err != nil {
		t.Fatalf("Chmod: %v", err)
	}
	s := OpenSettings(path)
	if !syncLeavesDiskUnchanged(t, path, s, func() { markDirtyNoop(s) }) {
		t.Fatal("no-op sync on empty file rewrote the file")
	}
}

func TestSyncOnCommentOnlyFileLeavesDiskUnchanged(t *testing.T) {
	t.Parallel()
	if runtime.GOOS == "windows" {
		t.Skip("unix mtime")
	}

	path := tempSettingsPath(t)
	original := []byte("; keep this comment\n")
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if err := os.WriteFile(path, original, 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	if err := os.Chmod(path, 0o644); err != nil {
		t.Fatalf("Chmod: %v", err)
	}
	s := OpenSettings(path)
	if !syncLeavesDiskUnchanged(t, path, s, func() { markDirtyNoop(s) }) {
		t.Fatal("no-op sync on comment-only file rewrote the file")
	}
	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile: %v", err)
	}
	if !bytes.Equal(after, original) {
		t.Fatalf("comment-only bytes changed:\n%s", after)
	}
}

func TestSettingsEncodeIsDeterministic(t *testing.T) {
	t.Parallel()

	s := OpenSettings(tempSettingsPath(t))
	s.SetValue("z", "b", "2")
	s.SetValue("z", "a", "1")
	s.SetValue("a", "y", "2")
	s.SetValue("a", "x", "1")
	first := s.encode()

	other := OpenSettings(tempSettingsPath(t))
	other.SetValue("a", "x", "1")
	other.SetValue("a", "y", "2")
	other.SetValue("z", "a", "1")
	other.SetValue("z", "b", "2")
	if second := other.encode(); string(second) != string(first) {
		t.Errorf("encode order-dependent:\n%s\n---\n%s", first, second)
	}
}

func TestSettingsIniEscapingHelpers(t *testing.T) {
	t.Parallel()

	stringCases := map[string]string{
		"a;b":      `"a;b"`,
		"a,b":      `"a,b"`,
		"a=b":      `"a=b"`,
		`a\b`:      `a\\b`,
		`a"b`:      `a\"b`,
		"a\nb":     `a\nb`,
		"a\tb":     `a\tb`,
		" x":       `" x"`,
		"x ":       `"x "`,
		"a#b":      "a#b",
		"":         "",
		"café":     "café",
		"trig\x00": `trig\0`,
	}
	for in, want := range stringCases {
		if got := iniEscapedString(in); got != want {
			t.Errorf("iniEscapedString(%q) = %q, want %q", in, got, want)
		}
	}

	if got := iniEscapedStringList(nil); got != "@Invalid()" {
		t.Errorf("empty list = %q, want @Invalid()", got)
	}
	if got := iniEscapedStringList([]string{"a", "b"}); got != "a, b" {
		t.Errorf("list = %q, want %q", got, "a, b")
	}
}

func TestSyncSameValueAfterWriteSkipsUnchangedBytes(t *testing.T) {
	t.Parallel()
	if runtime.GOOS == "windows" {
		t.Skip("unix mtime")
	}

	path := tempSettingsPath(t)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatalf("MkdirAll: %v", err)
	}
	if _, err := os.Stat(path); err == nil {
		t.Fatalf("test file already exists: %s", path)
	} else if !os.IsNotExist(err) {
		t.Fatalf("Stat: %v", err)
	}

	s := OpenSettings(path)
	s.SetBool("preferences", "directs", true)
	if got := s.Sync(); got != StatusWritten {
		t.Fatalf("first Sync() = %v, want StatusWritten", got)
	}
	if !syncLeavesDiskUnchanged(t, path, s, func() {
		s.SetBool("preferences", "directs", true)
	}) {
		t.Fatal("repeat assignment after first write rewrote the file")
	}
}

func equalStrings(left, right []string) bool {
	if len(left) != len(right) {
		return false
	}
	for i := range left {
		if left[i] != right[i] {
			return false
		}
	}
	return true
}

func TestSyncThroughSymlinkUpdatesTarget3dba(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("symlinks")
	}

	root := t.TempDir()
	cfgDir := filepath.Join(root, "cfg", "omairc")
	if err := os.MkdirAll(cfgDir, 0o700); err != nil {
		t.Fatalf("MkdirAll cfg: %v", err)
	}
	target := filepath.Join(cfgDir, "real", "omairc.conf")
	if err := os.MkdirAll(filepath.Dir(target), 0o700); err != nil {
		t.Fatalf("MkdirAll target dir: %v", err)
	}
	if err := os.WriteFile(target, []byte("[preferences]\ndirects=false\n"), 0o644); err != nil {
		t.Fatalf("WriteFile target: %v", err)
	}
	link := filepath.Join(cfgDir, "omairc.conf")
	if err := os.Symlink("real/omairc.conf", link); err != nil {
		t.Fatalf("Symlink: %v", err)
	}

	s := OpenSettings(link)
	s.SetBool("preferences", "directs", true)
	if got := s.Sync(); got != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", got)
	}

	linkInfo, err := os.Lstat(link)
	if err != nil {
		t.Fatalf("Lstat link: %v", err)
	}
	if linkInfo.Mode()&os.ModeSymlink == 0 {
		t.Fatal("config path must remain a symlink")
	}
	wantTarget := "real/omairc.conf"
	if got, err := os.Readlink(link); err != nil || got != wantTarget {
		t.Fatalf("Readlink = %q, want %q (err=%v)", got, wantTarget, err)
	}
	after, err := os.ReadFile(target)
	if err != nil {
		t.Fatalf("ReadFile target: %v", err)
	}
	if string(after) != "[preferences]\ndirects=true\n" {
		t.Fatalf("target bytes = %q", after)
	}
	targetInfo, err := os.Stat(target)
	if err != nil {
		t.Fatalf("Stat target: %v", err)
	}
	if targetInfo.Mode().Perm() != 0o644 {
		t.Fatalf("target mode = %o, want 0644", targetInfo.Mode().Perm())
	}
}

func TestSyncThroughSymlinkedParentAndDotdot3dba(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("symlinks")
	}

	root := t.TempDir()
	realStore := filepath.Join(root, "real", "store")
	realCfg := filepath.Join(root, "real", "cfg", "omairc")
	mustMkdirAll(t, realStore, realCfg)
	realFile := filepath.Join(realStore, "omairc.conf")
	mustWriteFileMode(t, realFile, []byte("old\n"), 0o644)
	cfgLink := filepath.Join(root, "cfg")
	mustSymlink(t, filepath.Join(root, "real", "cfg"), cfgLink)
	link := filepath.Join(cfgLink, "omairc", "omairc.conf")
	mustSymlink(t, "../../store/omairc.conf", link)

	if err := writeSettingsAtomic(link, []byte("new\n"), true); err != nil {
		t.Fatalf("writeSettingsAtomic: %v", err)
	}
	assertSymlinkTarget(t, link, "../../store/omairc.conf")
	assertFileBytes(t, realFile, []byte("new\n"))
}

func TestAtomicWritePathHelpers3dba(t *testing.T) {
	t.Parallel()
	cases := []struct {
		path     string
		wantDir  string
		wantBase string
	}{
		{"sts", ".", "sts"},
		{"x", ".", "x"},
		{"a/b", "a", "b"},
		{"a/b/", "a", "b"},
		{"/x", "/", "x"},
		{"..", ".", ".."},
		{"a/..", "a", ".."},
	}
	for _, tc := range cases {
		if got := atomicWriteDir(tc.path); got != tc.wantDir {
			t.Errorf("atomicWriteDir(%q) = %q, want %q", tc.path, got, tc.wantDir)
		}
		if got := atomicWriteBase(tc.path); got != tc.wantBase {
			t.Errorf("atomicWriteBase(%q) = %q, want %q", tc.path, got, tc.wantBase)
		}
	}
}

func TestWriteSettingsAtomicTwoHopDotdotDecoy3dba(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("symlinks")
	}

	root := t.TempDir()
	home := filepath.Join(root, "home")
	dots := filepath.Join(root, "dots")
	mustMkdirAll(t, filepath.Join(dots, "cfg", "omairc"), filepath.Join(dots, "pkg"), filepath.Join(home, "pkg"))
	decoy := filepath.Join(home, "pkg", "real.conf")
	real := filepath.Join(dots, "pkg", "real.conf")
	mustWriteFileMode(t, decoy, []byte("DECOY"), 0o644)
	mustWriteFileMode(t, real, []byte("REAL"), 0o644)
	mustSymlink(t, "../dots/cfg", filepath.Join(home, ".config"))
	mustSymlink(t, "../../pkg/hop2", filepath.Join(dots, "cfg", "omairc", "omairc.conf"))
	mustSymlink(t, "real.conf", filepath.Join(dots, "pkg", "hop2"))

	conf := filepath.Join(home, ".config", "omairc", "omairc.conf")
	if err := writeSettingsAtomic(conf, []byte("NEW"), true); err != nil {
		t.Fatalf("writeSettingsAtomic: %v", err)
	}
	assertFileBytes(t, real, []byte("NEW"))
	assertFileBytes(t, decoy, []byte("DECOY"))
}

func TestSyncThroughSymlinkedDataDirectory3dba(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("symlinks")
	}

	root := t.TempDir()
	data := filepath.Join(root, "data")
	mustMkdirAll(t, data, filepath.Join(root, "cfg"))
	target := filepath.Join(data, "omairc.conf")
	mustWriteFileMode(t, target, []byte("[preferences]\ndirects=false\n"), 0o644)
	mustSymlink(t, data, filepath.Join(root, "dotfiles"))
	link := filepath.Join(root, "cfg", "omairc.conf")
	mustSymlink(t, "../dotfiles/omairc.conf", link)

	s := OpenSettings(link)
	s.SetBool("preferences", "directs", true)
	if got := s.Sync(); got != StatusWritten {
		t.Fatalf("Sync() = %v, want StatusWritten", got)
	}
	assertFileBytes(t, target, []byte("[preferences]\ndirects=true\n"))
}

func TestWriteSettingsAtomicSymlinkCases3dba(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("symlinks")
	}

	t.Run("relative", func(t *testing.T) {
		root := t.TempDir()
		cfgDir := filepath.Join(root, "cfg", "omairc")
		mustMkdirAll(t, cfgDir)
		target := filepath.Join(cfgDir, "real", "omairc.conf")
		mustMkdirAll(t, filepath.Dir(target))
		mustWriteFileMode(t, target, []byte("old\n"), 0o644)
		link := filepath.Join(cfgDir, "omairc.conf")
		mustSymlink(t, "real/omairc.conf", link)
		if err := writeSettingsAtomic(link, []byte("new\n"), true); err != nil {
			t.Fatalf("writeSettingsAtomic: %v", err)
		}
		assertSymlinkTarget(t, link, "real/omairc.conf")
		assertFileBytes(t, target, []byte("new\n"))
		assertFileMode(t, target, 0o644)
	})

	t.Run("two-hop chain", func(t *testing.T) {
		root := t.TempDir()
		store := filepath.Join(root, "store")
		links := filepath.Join(root, "links")
		mustMkdirAll(t, store, links)
		target := filepath.Join(store, "omairc.conf")
		mustWriteFileMode(t, target, []byte("old\n"), 0o644)
		hop1 := filepath.Join(links, "hop1.conf")
		hop2 := filepath.Join(links, "hop2.conf")
		mustSymlink(t, "../store/omairc.conf", hop1)
		mustSymlink(t, "hop1.conf", hop2)
		if err := writeSettingsAtomic(hop2, []byte("new\n"), true); err != nil {
			t.Fatalf("writeSettingsAtomic: %v", err)
		}
		assertSymlinkTarget(t, hop2, "hop1.conf")
		assertSymlinkTarget(t, hop1, "../store/omairc.conf")
		assertFileBytes(t, target, []byte("new\n"))
	})

	t.Run("dangling with existing parent", func(t *testing.T) {
		root := t.TempDir()
		cfgDir := filepath.Join(root, "cfg", "omairc")
		realDir := filepath.Join(cfgDir, "real")
		mustMkdirAll(t, cfgDir, realDir)
		link := filepath.Join(cfgDir, "omairc.conf")
		mustSymlink(t, "real/omairc.conf", link)
		if err := writeSettingsAtomic(link, []byte("created\n"), false); err != nil {
			t.Fatalf("writeSettingsAtomic: %v", err)
		}
		target := filepath.Join(cfgDir, "real", "omairc.conf")
		assertSymlinkTarget(t, link, "real/omairc.conf")
		assertFileBytes(t, target, []byte("created\n"))
		assertFileMode(t, target, 0o600)
	})

	t.Run("dangling missing target directory", func(t *testing.T) {
		root := t.TempDir()
		cfgDir := filepath.Join(root, "cfg", "omairc")
		mustMkdirAll(t, cfgDir)
		link := filepath.Join(cfgDir, "omairc.conf")
		mustSymlink(t, "missing/deep/omairc.conf", link)
		missingDir := filepath.Join(cfgDir, "missing")
		if err := writeSettingsAtomic(link, []byte("nope\n"), false); err == nil {
			t.Fatal("expected error for missing target directory")
		}
		if _, err := os.Stat(missingDir); err == nil {
			t.Fatal("missing directory must not be created")
		}
	})

	t.Run("read-only target", func(t *testing.T) {
		root := t.TempDir()
		cfgDir := filepath.Join(root, "cfg", "omairc")
		mustMkdirAll(t, cfgDir)
		target := filepath.Join(cfgDir, "real", "omairc.conf")
		mustMkdirAll(t, filepath.Dir(target))
		original := []byte("[preferences]\ndirects=false\n")
		mustWriteFileMode(t, target, original, 0o444)
		link := filepath.Join(cfgDir, "omairc.conf")
		mustSymlink(t, "real/omairc.conf", link)
		if writable, err := os.OpenFile(link, os.O_WRONLY, 0); err == nil {
			writable.Close()
			_ = os.Chmod(target, 0o644)
			t.Skip("running as root")
		}
		s := OpenSettings(link)
		s.SetBool("preferences", "directs", true)
		if got := s.Sync(); got != StatusAccessError {
			t.Fatalf("Sync() = %v, want StatusAccessError", got)
		}
		assertFileBytes(t, target, original)
		assertSymlinkTarget(t, link, "real/omairc.conf")
		_ = os.Chmod(target, 0o644)
	})
}

func mustMkdirAll(t *testing.T, dirs ...string) {
	t.Helper()
	for _, dir := range dirs {
		if err := os.MkdirAll(dir, 0o700); err != nil {
			t.Fatalf("MkdirAll %s: %v", dir, err)
		}
	}
}

func mustWriteFileMode(t *testing.T, path string, data []byte, mode os.FileMode) {
	t.Helper()
	if err := os.WriteFile(path, data, mode); err != nil {
		t.Fatalf("WriteFile %s: %v", path, err)
	}
}

func mustSymlink(t *testing.T, target, link string) {
	t.Helper()
	if err := os.Symlink(target, link); err != nil {
		t.Fatalf("Symlink %s -> %s: %v", link, target, err)
	}
}

func assertSymlinkTarget(t *testing.T, link, wantTarget string) {
	t.Helper()
	info, err := os.Lstat(link)
	if err != nil {
		t.Fatalf("Lstat %s: %v", link, err)
	}
	if info.Mode()&os.ModeSymlink == 0 {
		t.Fatalf("%s is not a symlink", link)
	}
	got, err := os.Readlink(link)
	if err != nil || got != wantTarget {
		t.Fatalf("Readlink(%s) = %q, want %q (err=%v)", link, got, wantTarget, err)
	}
}

func assertFileBytes(t *testing.T, path string, want []byte) {
	t.Helper()
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile %s: %v", path, err)
	}
	if !bytes.Equal(got, want) {
		t.Fatalf("%s bytes = %q, want %q", path, got, want)
	}
}

func assertFileMode(t *testing.T, path string, want os.FileMode) {
	t.Helper()
	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("Stat %s: %v", path, err)
	}
	if info.Mode().Perm() != want {
		t.Fatalf("%s mode = %o, want %o", path, info.Mode().Perm(), want)
	}
}
