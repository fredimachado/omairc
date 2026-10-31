package storage

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

// The Qt/desktop group name for irc.example: SHA-256 of the host, lowercase hex.
const ircExampleSTSGroup = "a84a91b2bc2ea526cb7d244872ddffe0770a9ca3737ee9ef355e9018a76767de"

func TestSTSGroupKeyMatchesDesktop(t *testing.T) {
	// client/src/session/sts.rs::the_group_key_is_the_sha256_of_the_folded_host
	if got := STSGroupKey("irc.example"); got != ircExampleSTSGroup {
		t.Fatalf("STSGroupKey(irc.example) = %q, want %q", got, ircExampleSTSGroup)
	}
	if STSGroupKey("  IRC.Example \t") != STSGroupKey("irc.example") {
		t.Fatal("group key must fold case and trim")
	}
	if STSGroupKey("") != "" || STSGroupKey(" \t ") != "" {
		t.Fatal("an empty host has no key")
	}
}

func TestIniSTSStorePolicySurvivesRestartUnderQtGroupLayout(t *testing.T) {
	// client/src/storage/stspolicies.rs::a_policy_survives_a_restart_under_the_qt_group_layout
	dir := t.TempDir()
	path := filepath.Join(dir, "nested", "sts")
	NewIniSTSStore(path).Store("IRC.example ", 6697, 60, 1000)

	restarted := NewIniSTSStore(path)
	entry, ok := restarted.Lookup("irc.example", 1030)
	if !ok {
		t.Fatal("policy must survive a restart")
	}
	if entry.Port != 6697 || entry.DurationSeconds != 60 || entry.ExpiryEpochSeconds != 1060 {
		t.Fatalf("entry = %+v", entry)
	}

	text, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	body := string(text)
	for _, want := range []string{
		"[" + ircExampleSTSGroup + "]",
		"port=6697",
		"duration=60",
		"expiry=1060",
	} {
		if !strings.Contains(body, want) {
			t.Fatalf("file missing %q:\n%s", want, body)
		}
	}
}

func TestIniSTSStoreReadsFileWrittenByQt(t *testing.T) {
	// client/src/storage/stspolicies.rs::a_file_written_by_qt_is_read
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	qt := "[" + ircExampleSTSGroup + "]\nport=6697\nduration=60\nexpiry=4102444800\n"
	if err := os.WriteFile(path, []byte(qt), 0o600); err != nil {
		t.Fatal(err)
	}
	entry, ok := NewIniSTSStore(path).Lookup("irc.example", 1_700_000_000)
	if !ok {
		t.Fatal("qt fixture must load")
	}
	if entry.Port != 6697 || entry.DurationSeconds != 60 || entry.ExpiryEpochSeconds != 4_102_444_800 {
		t.Fatalf("entry = %+v", entry)
	}
}

func TestIniSTSStoreExpiredEntryIsDroppedFromFile(t *testing.T) {
	// client/src/storage/stspolicies.rs::an_expired_entry_is_dropped_from_the_file
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	store := NewIniSTSStore(path)
	store.Store("irc.example", 6697, 60, 0)

	if _, ok := store.Lookup("irc.example", 60); ok {
		t.Fatal("expired entry must not apply")
	}
	text, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(text), ircExampleSTSGroup) {
		t.Fatalf("expired group must be removed: %s", text)
	}
}

func TestIniSTSStoreBadPortIsDroppedFromFile(t *testing.T) {
	// client/src/storage/stspolicies.rs::a_bad_port_is_dropped_from_the_file
	for _, port := range []string{"0", "70000", "junk"} {
		dir := t.TempDir()
		path := filepath.Join(dir, "sts")
		qt := "[" + ircExampleSTSGroup + "]\nport=" + port + "\nduration=60\nexpiry=4102444800\n"
		if err := os.WriteFile(path, []byte(qt), 0o600); err != nil {
			t.Fatal(err)
		}
		if _, ok := NewIniSTSStore(path).Lookup("irc.example", 1); ok {
			t.Fatalf("port %s must not apply", port)
		}
		text, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		if strings.Contains(string(text), ircExampleSTSGroup) {
			t.Fatalf("port %s must be removed: %s", port, text)
		}
	}
}

func TestIniSTSStoreZeroDurationClearsHost(t *testing.T) {
	// client/src/storage/stspolicies.rs::a_zero_duration_clears_the_host
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	store := NewIniSTSStore(path)
	store.Store("irc.example", 6697, 60, 0)
	store.Store("irc.example", 6697, 0, 0)
	if _, ok := store.Lookup("irc.example", 1); ok {
		t.Fatal("zero duration must clear")
	}
}

func TestIniSTSStoreLookupOrClearWithNothingStoredWritesNoFile(t *testing.T) {
	// client/src/storage/stspolicies.rs::a_lookup_or_clear_with_nothing_stored_writes_no_file
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	store := NewIniSTSStore(path)
	if _, ok := store.Lookup("irc.example", 1); ok {
		t.Fatal("empty store must miss")
	}
	store.Clear("irc.example")
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Fatalf("clear of nothing must not create the file: %v", err)
	}
}

func TestIniSTSStoreMalformedFileIsLeftAlone(t *testing.T) {
	// client/src/storage/stspolicies.rs::a_malformed_file_is_left_alone
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	garbage := "this line has no equals sign\n"
	if err := os.WriteFile(path, []byte(garbage), 0o600); err != nil {
		t.Fatal(err)
	}
	store := NewIniSTSStore(path)
	if status := store.Store("irc.example", 6697, 60, 0); status != StatusFormatError {
		t.Fatalf("Store status = %v, want FormatError", status)
	}
	if _, ok := store.Lookup("irc.example", 1); ok {
		t.Fatal("malformed file must not yield a policy")
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(raw) != garbage {
		t.Fatalf("malformed file was rewritten: %q", raw)
	}
}

func TestIniSTSStoreFileIsOwnerOnlyAfterWrite(t *testing.T) {
	// client/src/storage/stspolicies.rs::the_file_is_owner_only_after_a_write
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	NewIniSTSStore(path).Store("irc.example", 6697, 60, 0)
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if mode := info.Mode().Perm(); mode != 0o600 {
		t.Fatalf("mode = %o, want 0600", mode)
	}
}

func TestSTSPathBesideSettings(t *testing.T) {
	got := STSPathBeside(filepath.Join("cfg", "omairc", "omairc.conf"))
	want := filepath.Join("cfg", "omairc", "sts")
	if got != want {
		t.Fatalf("STSPathBeside = %q, want %q", got, want)
	}
}
