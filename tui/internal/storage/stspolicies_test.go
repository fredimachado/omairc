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

func TestIniSTSStoreMalformedFileIsBackedUpAndNewPolicySurvives(t *testing.T) {
	// client/src/storage/stspolicies.rs::a_malformed_file_is_backed_up_and_a_new_policy_survives
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	garbage := "this line has no equals sign\n"
	if err := os.WriteFile(path, []byte(garbage), 0o600); err != nil {
		t.Fatal(err)
	}
	store := NewIniSTSStore(path)
	if status := store.Store("irc.example", 6697, 60, 0); status != StatusWritten {
		t.Fatalf("Store status = %v, want Written", status)
	}
	backup, err := os.ReadFile(filepath.Join(dir, "sts.corrupt"))
	if err != nil {
		t.Fatal(err)
	}
	if string(backup) != garbage {
		t.Fatalf("backup = %q, want original garbage", backup)
	}
	entry, ok := store.Lookup("irc.example", 1)
	if !ok || entry.Port != 6697 || entry.DurationSeconds != 60 || entry.ExpiryEpochSeconds != 60 {
		t.Fatalf("entry = %+v ok=%v", entry, ok)
	}
	restarted := NewIniSTSStore(path)
	if _, ok := restarted.Lookup("irc.example", 1); !ok {
		t.Fatal("new policy must survive a reload")
	}
}

func TestIniSTSStoreSecondMalformedWriteDoesNotClobberFirstBackup(t *testing.T) {
	// client/src/storage/stspolicies.rs::a_second_malformed_write_does_not_clobber_the_first_backup
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	first := "first garbage without equals\n"
	if err := os.WriteFile(path, []byte(first), 0o600); err != nil {
		t.Fatal(err)
	}
	store := NewIniSTSStore(path)
	if status := store.Store("irc.example", 6697, 60, 0); status != StatusWritten {
		t.Fatal(status)
	}
	second := "second garbage without equals\n"
	if err := os.WriteFile(path, []byte(second), 0o600); err != nil {
		t.Fatal(err)
	}
	if status := store.Store("other.example", 6697, 30, 10); status != StatusWritten {
		t.Fatal(status)
	}
	raw, err := os.ReadFile(filepath.Join(dir, "sts.corrupt"))
	if err != nil {
		t.Fatal(err)
	}
	if string(raw) != first {
		t.Fatalf("first backup clobbered: %q", raw)
	}
	raw, err = os.ReadFile(filepath.Join(dir, "sts.corrupt.1"))
	if err != nil {
		t.Fatal(err)
	}
	if string(raw) != second {
		t.Fatalf("second backup = %q", raw)
	}
	if _, ok := store.Lookup("other.example", 11); !ok {
		t.Fatal("second policy must load")
	}
}

func TestIniSTSStoreValidEntriesInPartlyMalformedFileArePreserved(t *testing.T) {
	// client/src/storage/stspolicies.rs::valid_entries_in_a_partly_malformed_file_are_preserved
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	other := STSGroupKey("other.example")
	mixed := "[" + ircExampleSTSGroup + "]\nport=6697\nduration=60\nexpiry=4102444800\nbad line\n[" + other + "]\nport=6698\nduration=30\nexpiry=4102444800\n"
	if err := os.WriteFile(path, []byte(mixed), 0o600); err != nil {
		t.Fatal(err)
	}
	store := NewIniSTSStore(path)
	if status := store.Store("third.example", 6699, 10, 1_700_000_000); status != StatusWritten {
		t.Fatal(status)
	}
	raw, err := os.ReadFile(filepath.Join(dir, "sts.corrupt"))
	if err != nil {
		t.Fatal(err)
	}
	if string(raw) != mixed {
		t.Fatalf("backup mismatch: %q", raw)
	}
	if entry, ok := store.Lookup("irc.example", 1_700_000_000); !ok || entry.Port != 6697 {
		t.Fatalf("irc.example = %+v ok=%v", entry, ok)
	}
	if entry, ok := store.Lookup("other.example", 1_700_000_000); !ok || entry.Port != 6698 {
		t.Fatalf("other.example = %+v ok=%v", entry, ok)
	}
	if _, ok := store.Lookup("third.example", 1_700_000_000); !ok {
		t.Fatal("third.example missing")
	}
}

func TestIniSTSStoreWeakerNewPolicyDoesNotShortenExistingExpiry(t *testing.T) {
	// client/src/storage/stspolicies.rs::a_weaker_new_policy_does_not_shorten_an_existing_expiry
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	mixed := "[" + ircExampleSTSGroup + "]\nport=6697\nduration=60\nexpiry=5000\nbad line\n"
	if err := os.WriteFile(path, []byte(mixed), 0o600); err != nil {
		t.Fatal(err)
	}
	store := NewIniSTSStore(path)
	if status := store.Store("irc.example", 7000, 10, 100); status != StatusWritten {
		t.Fatal(status)
	}
	entry, ok := store.Lookup("irc.example", 100)
	if !ok || entry.Port != 6697 || entry.DurationSeconds != 60 || entry.ExpiryEpochSeconds != 5000 {
		t.Fatalf("must keep stronger policy: %+v ok=%v", entry, ok)
	}
}

func TestIniSTSStoreFailedBackupLeavesFileAlone(t *testing.T) {
	// client/src/storage/stspolicies.rs::a_failed_backup_keeps_the_policy_in_memory
	// IniSTSStore itself has no memory; the session STSStore keeps the policy.
	if runtime.GOOS == "windows" {
		t.Skip("unix directory modes")
	}
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	garbage := "this line has no equals sign\n"
	if err := os.WriteFile(path, []byte(garbage), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(dir, 0o555); err != nil {
		t.Fatal(err)
	}
	defer os.Chmod(dir, 0o755)

	store := NewIniSTSStore(path)
	if status := store.Store("irc.example", 6697, 60, 0); status != StatusFormatError {
		t.Fatalf("Store status = %v, want FormatError", status)
	}
	if err := os.Chmod(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(raw) != garbage {
		t.Fatalf("malformed file was rewritten: %q", raw)
	}
	if _, ok := store.Lookup("irc.example", 1); ok {
		t.Fatal("failed backup must not invent a policy on disk")
	}
}

func TestIniSTSStoreFailedWriteAfterBackupLeavesSTSIntact(t *testing.T) {
	// client/src/storage/stspolicies.rs::a_failed_write_after_backup_leaves_sts_intact
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	garbage := "this line has no equals sign\n"
	if err := os.WriteFile(path, []byte(garbage), 0o600); err != nil {
		t.Fatal(err)
	}
	testFailSTSInstall = true
	defer func() { testFailSTSInstall = false }()

	store := NewIniSTSStore(path)
	if status := store.Store("irc.example", 6697, 60, 0); status != StatusFormatError {
		t.Fatalf("Store status = %v, want FormatError", status)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(raw) != garbage {
		t.Fatalf("sts must stay intact: %q", raw)
	}
	backup, err := os.ReadFile(filepath.Join(dir, "sts.corrupt"))
	if err != nil {
		t.Fatal(err)
	}
	if string(backup) != garbage {
		t.Fatalf("backup = %q", backup)
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	for _, entry := range entries {
		if strings.HasSuffix(entry.Name(), ".tmp") {
			t.Fatalf("leftover temp file: %s", entry.Name())
		}
	}
}

func TestIniSTSStoreExistingBackupNameIsNeverClobbered(t *testing.T) {
	// client/src/storage/stspolicies.rs::an_existing_backup_name_is_never_clobbered
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	preexisting := "preexisting backup bytes\n"
	if err := os.WriteFile(filepath.Join(dir, "sts.corrupt"), []byte(preexisting), 0o600); err != nil {
		t.Fatal(err)
	}
	garbage := "fresh garbage without equals\n"
	if err := os.WriteFile(path, []byte(garbage), 0o600); err != nil {
		t.Fatal(err)
	}
	store := NewIniSTSStore(path)
	if status := store.Store("irc.example", 6697, 60, 0); status != StatusWritten {
		t.Fatal(status)
	}
	raw, err := os.ReadFile(filepath.Join(dir, "sts.corrupt"))
	if err != nil {
		t.Fatal(err)
	}
	if string(raw) != preexisting {
		t.Fatalf("existing backup clobbered: %q", raw)
	}
	raw, err = os.ReadFile(filepath.Join(dir, "sts.corrupt.1"))
	if err != nil {
		t.Fatal(err)
	}
	if string(raw) != garbage {
		t.Fatalf("second backup = %q", raw)
	}
}

func TestIniSTSStoreInvalidPortEntryDoesNotBeatValidPolicy(t *testing.T) {
	// client/src/storage/stspolicies.rs::an_invalid_port_entry_does_not_beat_a_valid_policy
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	junk := "[" + ircExampleSTSGroup + "]\nport=junk\nduration=60\nexpiry=4102444800\nbad line\n"
	if err := os.WriteFile(path, []byte(junk), 0o600); err != nil {
		t.Fatal(err)
	}
	store := NewIniSTSStore(path)
	if status := store.Store("irc.example", 6697, 60, 1_700_000_000); status != StatusWritten {
		t.Fatal(status)
	}
	entry, ok := store.Lookup("irc.example", 1_700_000_000)
	if !ok || entry.Port != 6697 || entry.ExpiryEpochSeconds != 1_700_000_060 {
		t.Fatalf("must keep valid policy: %+v ok=%v", entry, ok)
	}
}

func TestIniSTSStoreKeysUnderGarbageHeaderAreNotSaved(t *testing.T) {
	// client/src/storage/stspolicies.rs::keys_under_a_garbage_header_are_not_saved
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	other := STSGroupKey("other.example")
	mixed := "[" + ircExampleSTSGroup + "]\nport=6697\nduration=60\nexpiry=4102444800\ngarbage header line\nport=7000\nduration=100\nexpiry=100\n[" + other + "]\nport=6698\nduration=30\nexpiry=4102444800\n"
	if err := os.WriteFile(path, []byte(mixed), 0o600); err != nil {
		t.Fatal(err)
	}
	store := NewIniSTSStore(path)
	if status := store.Store("third.example", 6699, 10, 1_700_000_000); status != StatusWritten {
		t.Fatal(status)
	}
	entry, ok := store.Lookup("irc.example", 1_700_000_000)
	if !ok || entry.Port != 6697 || entry.ExpiryEpochSeconds != 4_102_444_800 {
		t.Fatalf("leaked keys must not weaken A: %+v ok=%v", entry, ok)
	}
	if entry, ok := store.Lookup("other.example", 1_700_000_000); !ok || entry.Port != 6698 {
		t.Fatalf("other.example = %+v ok=%v", entry, ok)
	}
	text, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	body := string(text)
	if strings.Contains(body, "port=7000") {
		t.Fatalf("leaked port must not be saved: %s", body)
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
