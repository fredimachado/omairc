package storage

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
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
	s.SetStringList("lists", "single", []string{"libera"})
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
		{"single", []string{"libera"}},
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
	s.SetValue("networks/abc", "host", "irc.libera.chat")
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
	if got := s.Value("networks/abc", "host"); got != "irc.libera.chat" {
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
			`abc\host=irc.libera.chat` + "\n" +
			`abc\port=6697` + "\n" +
			`abc\tls=true` + "\n" +
			"abc\\autojoin=#a, #b\n" +
			`abc\playback="[{\"target\":\"a,b\",\"ms\":\"1\"}]"` + "\n" +
			"\n[preferences]\ndirects=true\nnetworkOrder=libera\n",
		// A hand-written nested section QSettings also accepts.
		"nested-section": "[networks/abc]\n" +
			"host=irc.libera.chat\nport=6697\ntls=true\nautojoin=#a, #b\n" +
			"playback=\"[{\\\"target\\\":\\\"a,b\\\",\\\"ms\\\":\\\"1\\\"}]\"\n" +
			"\n[preferences]\ndirects=true\nnetworkOrder=libera\n",
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
			if got := s.Value("networks/abc", "host"); got != "irc.libera.chat" {
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
			if got := s.StringList("preferences", "networkOrder"); !equalStrings(got, []string{"libera"}) {
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
	original := []byte("[networks]\nabc\\host=irc.libera.chat\n")
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
