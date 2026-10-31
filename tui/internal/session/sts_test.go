package session

import (
	"math"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/storage"
)

func stsTokens(text string) []string {
	return strings.Fields(text)
}

// client/src/session/sts.rs::a_bare_sts_token_is_an_empty_advertisement
func TestParseSTSBareTokenIsEmptyAdvertisement(t *testing.T) {
	ad, ok := ParseSTSAdvertisement(stsTokens("multi-prefix sts"))
	if !ok {
		t.Fatal("bare sts must be found")
	}
	if ad.Port != nil || ad.DurationSeconds != nil {
		t.Fatalf("bare sts must be empty, got %+v", ad)
	}
}

// client/src/session/sts.rs::no_sts_token_gives_no_advertisement
func TestParseSTSNoTokenGivesNoAdvertisement(t *testing.T) {
	if _, ok := ParseSTSAdvertisement(stsTokens("multi-prefix sasl")); ok {
		t.Fatal("no sts token must yield ok false")
	}
}

// client/src/session/sts.rs::the_sts_name_matches_any_case_and_the_last_token_wins
func TestParseSTSNameMatchesAnyCaseAndLastTokenWins(t *testing.T) {
	ad, ok := ParseSTSAdvertisement(stsTokens("STS=port=6697 sts=port=6698"))
	if !ok || ad.Port == nil || *ad.Port != 6698 {
		t.Fatalf("last token must win, got %+v ok=%v", ad, ok)
	}
}

// client/src/session/sts.rs::port_and_duration_parse_from_the_comma_list
func TestParseSTSPortAndDurationFromCommaList(t *testing.T) {
	ad, ok := ParseSTSAdvertisement(stsTokens("sts=port=6697,duration=86400"))
	if !ok || ad.Port == nil || *ad.Port != 6697 || ad.DurationSeconds == nil || *ad.DurationSeconds != 86400 {
		t.Fatalf("got %+v ok=%v", ad, ok)
	}
}

// client/src/session/sts.rs::the_first_valid_value_of_each_key_wins
func TestParseSTSFirstValidValueOfEachKeyWins(t *testing.T) {
	ad, ok := ParseSTSAdvertisement(stsTokens("sts=port=0,port=6697,port=6698,duration=5,duration=9"))
	if !ok || ad.Port == nil || *ad.Port != 6697 || ad.DurationSeconds == nil || *ad.DurationSeconds != 5 {
		t.Fatalf("got %+v ok=%v", ad, ok)
	}
}

// client/src/session/sts.rs::malformed_and_out_of_range_values_are_ignored
func TestParseSTSMalformedAndOutOfRangeValuesAreIgnored(t *testing.T) {
	for _, value := range []string{
		"port=0", "port=65536", "port=-1", "port=+6697", "port=abc",
		"duration=-1", "duration=soon",
	} {
		ad, ok := ParseSTSAdvertisement(stsTokens("sts=" + value))
		if !ok || ad.Port != nil || ad.DurationSeconds != nil {
			t.Fatalf("%s must not set a field, got %+v ok=%v", value, ad, ok)
		}
	}
}

// client/src/session/sts.rs::a_zero_duration_is_kept_so_the_session_can_clear
func TestParseSTSZeroDurationIsKept(t *testing.T) {
	ad, ok := ParseSTSAdvertisement(stsTokens("sts=duration=0"))
	if !ok || ad.DurationSeconds == nil || *ad.DurationSeconds != 0 {
		t.Fatalf("got %+v ok=%v", ad, ok)
	}
}

// client/src/session/sts.rs::an_empty_list_item_is_skipped
func TestParseSTSEmptyListItemIsSkipped(t *testing.T) {
	ad, ok := ParseSTSAdvertisement(stsTokens("sts=,,port=6697,"))
	if !ok || ad.Port == nil || *ad.Port != 6697 {
		t.Fatalf("got %+v ok=%v", ad, ok)
	}
}

// client/src/session/sts.rs::a_stored_policy_is_found_until_it_expires
func TestSTSStorePolicyFoundUntilExpiry(t *testing.T) {
	store := NewSTSStore()
	now := time.Unix(1000, 0)
	_ = store.Store("irc.example", 6697, 60, now)
	cached, ok := store.Lookup("irc.example", time.Unix(1059, 0))
	if !ok || cached.Port != 6697 || cached.DurationSeconds != 60 || cached.ExpiryEpochSeconds != 1060 {
		t.Fatalf("still valid: %+v ok=%v", cached, ok)
	}
	if _, ok := store.Lookup("irc.example", time.Unix(1060, 0)); ok {
		t.Fatal("the expiry instant has passed")
	}
}

// client/src/session/sts.rs::lookup_folds_case_and_trims_the_host
func TestSTSStoreLookupFoldsCaseAndTrims(t *testing.T) {
	store := NewSTSStore()
	_ = store.Store("IRC.example", 6697, 60, time.Unix(0, 0))
	if _, ok := store.Lookup("  irc.EXAMPLE ", time.Unix(1, 0)); !ok {
		t.Fatal("lookup must fold and trim")
	}
}

// client/src/session/sts.rs::an_expired_entry_is_dropped_by_lookup
func TestSTSStoreExpiredEntryIsDropped(t *testing.T) {
	store := NewSTSStore()
	_ = store.Store("irc.example", 6697, 60, time.Unix(0, 0))
	if _, ok := store.Lookup("irc.example", time.Unix(61, 0)); ok {
		t.Fatal("expired must miss")
	}
	if len(store.entries) != 0 {
		t.Fatal("an expired entry is removed")
	}
}

// client/src/session/sts.rs::a_zero_duration_clears_the_host
func TestSTSStoreZeroDurationClearsHost(t *testing.T) {
	store := NewSTSStore()
	_ = store.Store("irc.example", 6697, 60, time.Unix(0, 0))
	_ = store.Store("irc.example", 6697, 0, time.Unix(0, 0))
	if _, ok := store.Lookup("irc.example", time.Unix(1, 0)); ok {
		t.Fatal("zero duration must clear")
	}
}

// client/src/session/sts.rs::a_zero_port_or_an_empty_host_is_not_stored
func TestSTSStoreZeroPortOrEmptyHostNotStored(t *testing.T) {
	store := NewSTSStore()
	_ = store.Store("irc.example", 0, 60, time.Unix(0, 0))
	_ = store.Store("  ", 6697, 60, time.Unix(0, 0))
	if len(store.entries) != 0 {
		t.Fatal("nothing should be stored")
	}
}

// client/src/session/sts.rs::clear_drops_only_the_named_host
func TestSTSStoreClearDropsOnlyNamedHost(t *testing.T) {
	store := NewSTSStore()
	_ = store.Store("a.example", 6697, 60, time.Unix(0, 0))
	_ = store.Store("b.example", 6697, 60, time.Unix(0, 0))
	_ = store.Clear("a.example")
	if _, ok := store.Lookup("a.example", time.Unix(1, 0)); ok {
		t.Fatal("a must be gone")
	}
	if _, ok := store.Lookup("b.example", time.Unix(1, 0)); !ok {
		t.Fatal("b must remain")
	}
}

// client/src/session/sts.rs::a_huge_duration_saturates_instead_of_wrapping
func TestSTSStoreHugeDurationSaturates(t *testing.T) {
	store := NewSTSStore()
	_ = store.Store("irc.example", 6697, math.MaxInt64, time.Unix(1, 0))
	cached, ok := store.Lookup("irc.example", time.Unix(2, 0))
	if !ok || cached.ExpiryEpochSeconds != math.MaxInt64 {
		t.Fatalf("got %+v ok=%v", cached, ok)
	}
}

// A malformed sts file is recovered; memory and disk both keep the policy.
func TestSTSStoreMalformedDiskRecoversWithoutWeakeningMemory(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	garbage := "this line has no equals sign\n"
	if err := os.WriteFile(path, []byte(garbage), 0o600); err != nil {
		t.Fatal(err)
	}
	store := NewSTSStore()
	_ = store.Store("irc.example", 6697, 60, time.Unix(0, 0))
	store.AttachDisk(storage.NewIniSTSStore(path))
	if err := store.Store("irc.example", 6697, 120, time.Unix(10, 0)); err != nil {
		t.Fatalf("recovery must succeed: %v", err)
	}
	cached, ok := store.Lookup("irc.example", time.Unix(11, 0))
	if !ok || cached.DurationSeconds != 120 || cached.Port != 6697 {
		t.Fatalf("memory must keep the refreshed policy: %+v ok=%v", cached, ok)
	}
	backup, err := os.ReadFile(filepath.Join(dir, "sts.corrupt"))
	if err != nil {
		t.Fatal(err)
	}
	if string(backup) != garbage {
		t.Fatalf("backup = %q", backup)
	}
	entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 11)
	if !ok || entry.DurationSeconds != 120 {
		t.Fatalf("disk entry = %+v ok=%v", entry, ok)
	}
}

// Fail-closed: when the backup rename cannot run, memory still keeps the policy
// and the malformed file is left untouched.
func TestSTSStoreFailedBackupRenameDoesNotWeakenMemory(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix directory modes")
	}
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	garbage := "this line has no equals sign\n"
	if err := os.WriteFile(path, []byte(garbage), 0o600); err != nil {
		t.Fatal(err)
	}
	store := NewSTSStore()
	store.AttachDisk(storage.NewIniSTSStore(path))
	if err := os.Chmod(dir, 0o555); err != nil {
		t.Fatal(err)
	}
	err := store.Store("irc.example", 6697, 120, time.Unix(10, 0))
	_ = os.Chmod(dir, 0o755)
	if err == nil {
		t.Fatal("failed rename must report a persist error")
	}
	cached, ok := store.Lookup("irc.example", time.Unix(11, 0))
	if !ok || cached.DurationSeconds != 120 || cached.Port != 6697 {
		t.Fatalf("memory must keep the policy: %+v ok=%v", cached, ok)
	}
	raw, readErr := os.ReadFile(path)
	if readErr != nil {
		t.Fatal(readErr)
	}
	if string(raw) != garbage {
		t.Fatalf("malformed file was rewritten: %q", raw)
	}
}

// All in-memory hosts are written when recovering a malformed file.
func TestSTSStoreMalformedRecoveryFlushesEveryMemoryHost(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	if err := os.WriteFile(path, []byte("bad line without equals\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	store := NewSTSStore()
	now := time.Unix(100, 0)
	_ = store.Store("a.example", 6697, 60, now)
	_ = store.Store("b.example", 6697, 60, now)
	store.AttachDisk(storage.NewIniSTSStore(path))
	if err := store.Store("c.example", 6697, 60, now); err != nil {
		t.Fatal(err)
	}
	disk := storage.NewIniSTSStore(path)
	for _, host := range []string{"a.example", "b.example", "c.example"} {
		if _, ok := disk.Lookup(host, now.Unix()); !ok {
			t.Fatalf("%s missing from recovered disk", host)
		}
	}
}

// duration=0 through STSStore must clear the on-disk group, not only memory.
func TestSTSStoreZeroDurationClearsDisk(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	store := NewSTSStore()
	store.AttachDisk(storage.NewIniSTSStore(path))
	now := time.Unix(1_700_000_000, 0)
	if err := store.Store("irc.example", 6697, 60, now); err != nil {
		t.Fatal(err)
	}
	if _, ok := storage.NewIniSTSStore(path).Lookup("irc.example", now.Unix()); !ok {
		t.Fatal("policy must be on disk before clear")
	}
	if err := store.Store("irc.example", 6697, 0, now); err != nil {
		t.Fatal(err)
	}
	if _, ok := store.Lookup("irc.example", now.Add(time.Second)); ok {
		t.Fatal("memory must clear on duration=0")
	}
	if _, ok := storage.NewIniSTSStore(path).Lookup("irc.example", now.Unix()); ok {
		t.Fatal("duration=0 through STSStore must remove the host from disk")
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(raw), storage.STSGroupKey("irc.example")) {
		t.Fatalf("cleared group still in file: %s", raw)
	}
}

// Concurrent Stores from shared-manager sessions must not drop each other's
// hosts. Each round uses a fresh file and asserts ExpiryEpochSeconds ==
// round+60 for every host, so a lost update cannot hide behind an older
// unexpired entry. Without the STSStore mutex this fails; with it, it passes.
func TestSTSStoreConcurrentStoresPersistAllHosts(t *testing.T) {
	dir := t.TempDir()
	hosts := []string{"a.example", "b.example", "c.example", "d.example"}
	const rounds = 50
	var wg sync.WaitGroup
	for round := 0; round < rounds; round++ {
		path := filepath.Join(dir, "sts-"+strconv.Itoa(round))
		store := NewSTSStore()
		store.AttachDisk(storage.NewIniSTSStore(path))
		now := time.Unix(int64(round), 0)
		wantExpiry := now.Unix() + 60
		for _, host := range hosts {
			wg.Add(1)
			go func(host string, now time.Time) {
				defer wg.Done()
				if err := store.Store(host, 6697, 60, now); err != nil {
					t.Errorf("Store(%s): %v", host, err)
				}
			}(host, now)
		}
		wg.Wait()
		disk := storage.NewIniSTSStore(path)
		for _, host := range hosts {
			entry, ok := disk.Lookup(host, now.Unix())
			if !ok {
				t.Fatalf("round %d: host %s missing from disk", round, host)
			}
			if entry.ExpiryEpochSeconds != wantExpiry {
				t.Fatalf("round %d: host %s expiry = %d, want %d", round, host, entry.ExpiryEpochSeconds, wantExpiry)
			}
		}
	}
}
