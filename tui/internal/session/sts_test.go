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

// A Lookup that removes a stale group from a malformed file must recover with
// flushRecoveryLocked so another in-memory host is kept on disk and on lookup.
func TestSTSStoreLookupRemoveOnMalformedKeepsOtherMemoryHost(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	store := NewSTSStore()
	now := time.Unix(1_700_000_000, 0)
	if err := store.Store("keep.example", 6697, 60, now); err != nil {
		t.Fatal(err)
	}
	dropKey := storage.STSGroupKey("drop.example")
	malformed := "[" + dropKey + "]\nport=junk\nduration=60\nexpiry=4102444800\nbad line\n"
	if err := os.WriteFile(path, []byte(malformed), 0o600); err != nil {
		t.Fatal(err)
	}
	store.AttachDisk(storage.NewIniSTSStore(path))
	if _, ok := store.Lookup("drop.example", now); ok {
		t.Fatal("stale drop.example must not apply")
	}
	cached, ok := store.Lookup("keep.example", now.Add(time.Second))
	if !ok || cached.Port != 6697 || cached.DurationSeconds != 60 {
		t.Fatalf("keep.example must remain in memory: %+v ok=%v", cached, ok)
	}
	disk, ok := storage.NewIniSTSStore(path).Lookup("keep.example", now.Unix())
	if !ok || disk.Port != 6697 || disk.DurationSeconds != 60 {
		t.Fatalf("keep.example must be on disk after recovery: %+v ok=%v", disk, ok)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(raw), dropKey) {
		t.Fatalf("stale drop group must be gone after recovery: %s", raw)
	}
	if !strings.Contains(string(raw), storage.STSGroupKey("keep.example")) {
		t.Fatalf("keep.example missing from recovered file: %s", raw)
	}
}

// Fail-closed: when the backup step cannot run, memory still keeps the policy
// and the malformed file is left untouched.
func TestSTSStoreFailedBackupDoesNotWeakenMemory(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	garbage := "this line has no equals sign\n"
	if err := os.WriteFile(path, []byte(garbage), 0o600); err != nil {
		t.Fatal(err)
	}
	store := NewSTSStore()
	store.AttachDisk(storage.NewIniSTSStore(path))
	restore := storage.TestingSetSTSHook("fail-backup", 0)
	defer restore()
	err := store.Store("irc.example", 6697, 120, time.Unix(10, 0))
	if err == nil {
		t.Fatal("failed backup must report a persist error")
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

// A host cleared on disk must not return when a normal (well-formed) write
// only persists the new host. Memory still holds the old host, but flush is
// recovery-only.
func TestSTSStoreClearedHostNotBroughtBackByNormalWrite(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	store := NewSTSStore()
	store.AttachDisk(storage.NewIniSTSStore(path))
	now := time.Unix(100, 0)
	if err := store.Store("keep.example", 6697, 60, now); err != nil {
		t.Fatal(err)
	}
	if err := store.Store("drop.example", 6697, 60, now); err != nil {
		t.Fatal(err)
	}
	// Simulate another client clearing drop.example on disk with duration=0.
	keepKey := storage.STSGroupKey("keep.example")
	onlyKeep := "[" + keepKey + "]\nduration=60\nexpiry=160\nport=6697\n"
	if err := os.WriteFile(path, []byte(onlyKeep), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := store.Store("other.example", 6697, 30, time.Unix(110, 0)); err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	body := string(raw)
	dropKey := storage.STSGroupKey("drop.example")
	if strings.Contains(body, dropKey) {
		t.Fatalf("cleared host must not return on a normal write: %s", body)
	}
	if !strings.Contains(body, keepKey) {
		t.Fatalf("keep.example missing: %s", body)
	}
	if !strings.Contains(body, storage.STSGroupKey("other.example")) {
		t.Fatalf("other.example missing: %s", body)
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

// stsStoreOnFile is one client's STS cache, attached to the shared file at path.
// Two of them stand for two running clients.
func stsStoreOnFile(path string) *STSStore {
	store := NewSTSStore()
	store.AttachDisk(storage.NewIniSTSStore(path))
	return store
}

func TestSTSStoreRefreshKeepsNewerPolicyAnotherClientSaved(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sts")
	older := stsStoreOnFile(path)
	newer := stsStoreOnFile(path)
	if err := older.Store("irc.example", 6697, 3600, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	if err := newer.Store("irc.example", 6697, 60, time.Unix(10, 0)); err != nil {
		t.Fatal(err)
	}

	if err := older.Refresh("irc.example", 3600, time.Unix(20, 0)); err != nil {
		t.Fatal(err)
	}

	entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 20)
	if !ok || entry.DurationSeconds != 60 || entry.ExpiryEpochSeconds != 70 {
		t.Fatalf("file must keep the newer 60s policy: %+v ok=%v", entry, ok)
	}
}

func TestSTSStoreRefreshExtendsSameDurationPolicyOnDisk(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sts")
	store := stsStoreOnFile(path)
	if err := store.Store("irc.example", 6697, 60, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}

	if err := store.Refresh("irc.example", 60, time.Unix(30, 0)); err != nil {
		t.Fatal(err)
	}

	entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 31)
	if !ok || entry.ExpiryEpochSeconds != 90 {
		t.Fatalf("refresh must extend the expiry to 30+60: %+v ok=%v", entry, ok)
	}
}

func TestSTSStoreRefreshDoesNotRecreateClearedPolicy(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sts")
	older := stsStoreOnFile(path)
	newer := stsStoreOnFile(path)
	if err := older.Store("irc.example", 6697, 60, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	if err := newer.Store("irc.example", 6697, 0, time.Unix(10, 0)); err != nil {
		t.Fatal(err)
	}

	if err := older.Refresh("irc.example", 60, time.Unix(20, 0)); err != nil {
		t.Fatal(err)
	}

	if _, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 20); ok {
		t.Fatal("a cleared host must not come back on refresh")
	}
	raw, err := os.ReadFile(path)
	if err == nil && strings.Contains(string(raw), storage.STSGroupKey("irc.example")) {
		t.Fatalf("cleared group must stay out of the file: %s", raw)
	}
}

func TestSTSStoreRefreshRevivesLapsedPolicyOfSameDuration(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sts")
	store := stsStoreOnFile(path)
	if err := store.Store("irc.example", 6697, 60, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}

	if err := store.Refresh("irc.example", 60, time.Unix(100, 0)); err != nil {
		t.Fatal(err)
	}

	entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 100)
	if !ok || entry.ExpiryEpochSeconds != 160 {
		t.Fatalf("a session that outlived its duration re-arms its own policy: %+v ok=%v", entry, ok)
	}
}

// TestSessionQuitKeepsNewerPolicyAnotherClientSaved is the two-writer case through
// Stop: the older client quits holding 3600s after the newer one saved 60s.
func TestSessionQuitKeepsNewerPolicyAnotherClientSaved(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sts")
	clock := NewFakeClock(time.Unix(0, 0))
	older, olderTransport := stsSessionOnFile(path, clock)
	older.Start()
	olderTransport.CompleteConnect()
	olderTransport.InjectBytes([]byte(":server CAP omairc LS :sts=duration=3600\r\n"))

	newer, newerTransport := stsSessionOnFile(path, clock)
	newer.Start()
	newerTransport.CompleteConnect()
	newerTransport.InjectBytes([]byte(":server CAP omairc LS :sts=duration=60\r\n"))

	older.Stop()

	entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", clock.Now().Unix())
	if !ok || entry.DurationSeconds != 60 {
		t.Fatalf("quitting the older client must keep the newer 60s policy: %+v ok=%v", entry, ok)
	}
}

// stsSessionOnFile is a TLS session whose cache is the shared file at path.
func stsSessionOnFile(path string, clock *FakeClock) (*Session, *LoopbackTransport) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.AutojoinChannels = nil
	transport := NewLoopbackTransport()
	return NewSessionWithSTS(config, transport, clock, stsStoreOnFile(path)), transport
}

// TestSTSStoreLookupPrefersNewerDiskPolicyOverMemory: this client's memory holds
// 3600s, and another client saved 60s after it, so the file's policy wins.
func TestSTSStoreLookupPrefersNewerDiskPolicyOverMemory(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sts")
	ours := stsStoreOnFile(path)
	theirs := stsStoreOnFile(path)
	if err := ours.Store("irc.example", 6697, 3600, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	if err := theirs.Store("irc.example", 6697, 60, time.Unix(10, 0)); err != nil {
		t.Fatal(err)
	}

	cached, ok := ours.Lookup("irc.example", time.Unix(20, 0))
	if !ok || cached.DurationSeconds != 60 || cached.ExpiryEpochSeconds != 70 {
		t.Fatalf("the policy another client saved must beat memory: %+v ok=%v", cached, ok)
	}
}

// TestSTSStoreRecoveryKeepsNewerDiskPolicyOverMemory: the file goes malformed
// after another client saved 60s, and recovery must not write the stale 3600s
// memory entry over it.
func TestSTSStoreRecoveryKeepsNewerDiskPolicyOverMemory(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sts")
	ours := stsStoreOnFile(path)
	theirs := stsStoreOnFile(path)
	if err := ours.Store("irc.example", 6697, 3600, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	if err := theirs.Store("irc.example", 6697, 60, time.Unix(10, 0)); err != nil {
		t.Fatal(err)
	}
	key := storage.STSGroupKey("irc.example")
	mixed := "this line has no equals sign\n[" + key + "]\nport=6697\nduration=60\nexpiry=70\n"
	if err := os.WriteFile(path, []byte(mixed), 0o600); err != nil {
		t.Fatal(err)
	}

	if err := ours.Store("third.example", 6699, 10, time.Unix(20, 0)); err != nil {
		t.Fatal(err)
	}

	entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 20)
	if !ok || entry.DurationSeconds != 60 || entry.ExpiryEpochSeconds != 70 {
		t.Fatalf("recovery must keep the 60s policy on disk: %+v ok=%v", entry, ok)
	}
}

// TestSessionCachedPolicyBesideJunkGroupsUpgradesPlaintext: a policy in a file
// that also holds junk groups still makes a plaintext profile connect with TLS
// on the cached port.
func TestSessionCachedPolicyBesideJunkGroupsUpgradesPlaintext(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sts")
	now := time.Unix(1_700_000_000, 0)
	if status := storage.NewIniSTSStore(path).Store("irc.example", 6697, 60, now.Unix(), nil); status != storage.StatusWritten {
		t.Fatalf("seed write = %v", status)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	junk := "[deadbeef]\nport=1\nduration=60\nexpiry=4102444800\n"
	if err := os.WriteFile(path, append(raw, junk...), 0o600); err != nil {
		t.Fatal(err)
	}
	store := NewSTSStore()
	store.AttachDisk(storage.NewIniSTSStore(path))
	transport := NewLoopbackTransport()
	session := NewSessionWithSTS(sessionTestPlaintextConfig(), transport, NewFakeClock(now), store)

	session.Start()

	if !transport.TLSRequested() {
		t.Fatal("the cached policy must force TLS beside junk groups")
	}
	if got := transport.ConnectedPort(); got != 6697 {
		t.Fatalf("port = %d, want 6697", got)
	}
}

// TestSTSStoreMalformedRecoveryWritesTheStoredPolicyOverTheFile: the file goes
// malformed holding a live 3600s policy for the host this client then stores at
// 60s. The new policy must land, so the file and a lookup both show 60s.
func TestSTSStoreMalformedRecoveryWritesTheStoredPolicyOverTheFile(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sts")
	key := storage.STSGroupKey("irc.example")
	live := "this line has no equals sign\n[" + key + "]\nport=6697\nduration=3600\nexpiry=3600\n"
	if err := os.WriteFile(path, []byte(live), 0o600); err != nil {
		t.Fatal(err)
	}
	store := stsStoreOnFile(path)

	if err := store.Store("irc.example", 6697, 60, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}

	entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 1)
	if !ok || entry.DurationSeconds != 60 || entry.ExpiryEpochSeconds != 60 {
		t.Fatalf("the stored 60s policy must replace the file's 3600s one: %+v ok=%v", entry, ok)
	}
	cached, ok := store.Lookup("irc.example", time.Unix(1, 0))
	if !ok || cached.DurationSeconds != 60 {
		t.Fatalf("lookup must show the stored 60s policy: %+v ok=%v", cached, ok)
	}
}

// TestSTSStoreRefreshOnMalformedFileExtendsThePolicy: a refresh of a 3600s
// policy on a file that has gone malformed must still extend its expiry on disk.
func TestSTSStoreRefreshOnMalformedFileExtendsThePolicy(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sts")
	store := stsStoreOnFile(path)
	if err := store.Store("irc.example", 6697, 3600, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, append([]byte("this line has no equals sign\n"), raw...), 0o600); err != nil {
		t.Fatal(err)
	}

	if err := store.Refresh("irc.example", 3600, time.Unix(100, 0)); err != nil {
		t.Fatal(err)
	}

	entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 101)
	if !ok || entry.ExpiryEpochSeconds != 3700 {
		t.Fatalf("refresh on a malformed file must extend to 100+3600: %+v ok=%v", entry, ok)
	}
}

// TestSTSStoreUnwritableStorePrefersItsLongerPolicyOverFile: the file holds a
// 60s policy and is read-only, so storing 86400s for the host fails and keeps
// the new policy in memory. After the file's 60s lapses, lookup still returns
// the 86400s policy instead of dropping to none.
func TestSTSStoreUnwritableStorePrefersItsLongerPolicyOverFile(t *testing.T) {
	// client/src/storage/stspolicies.rs::an_unwritable_store_keeps_its_longer_policy_over_the_file
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	path := filepath.Join(t.TempDir(), "sts")
	if err := stsStoreOnFile(path).Store("irc.example", 6697, 60, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	makeReadOnly(t, path)
	store := stsStoreOnFile(path)

	if err := store.Store("irc.example", 6697, 86400, time.Unix(0, 0)); err == nil {
		t.Fatal("a read-only file must refuse the write")
	}

	for _, seconds := range []int64{31, 61} {
		cached, ok := store.Lookup("irc.example", time.Unix(seconds, 0))
		if !ok || cached.ExpiryEpochSeconds != 86400 {
			t.Fatalf("at %ds the unwritten 86400s policy must hold: %+v ok=%v", seconds, cached, ok)
		}
	}
}

// TestSTSStoreLaterWriteClearsTheUnsavedMark: once a write of the host lands,
// the policy is in the file, so a newer file policy from another client wins again.
func TestSTSStoreLaterWriteClearsTheUnsavedMark(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	path := filepath.Join(t.TempDir(), "sts")
	if err := stsStoreOnFile(path).Store("irc.example", 6697, 60, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	makeReadOnly(t, path)
	store := stsStoreOnFile(path)
	if err := store.Store("irc.example", 6697, 86400, time.Unix(0, 0)); err == nil {
		t.Fatal("a read-only file must refuse the write")
	}
	if err := os.Chmod(path, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := store.Store("irc.example", 6697, 30, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	if err := stsStoreOnFile(path).Store("irc.example", 6697, 45, time.Unix(10, 0)); err != nil {
		t.Fatal(err)
	}

	cached, ok := store.Lookup("irc.example", time.Unix(11, 0))
	if !ok || cached.DurationSeconds != 45 {
		t.Fatalf("after a landed write the newer file policy must win: %+v ok=%v", cached, ok)
	}
}

// TestSTSStoreAnotherClientReplacementAfterFailedWriteWins: another client saves a
// different port for the host after this store's write failed, and the disk
// recovers. Lookup follows the file, and a refresh does not write 6697 back.
func TestSTSStoreAnotherClientReplacementAfterFailedWriteWins(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	path := filepath.Join(t.TempDir(), "sts")
	if err := stsStoreOnFile(path).Store("irc.example", 6697, 3600, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	makeReadOnly(t, path)
	ours := stsStoreOnFile(path)
	if err := ours.Store("irc.example", 6697, 86400, time.Unix(10, 0)); err == nil {
		t.Fatal("a read-only file must refuse the write")
	}
	if err := os.Chmod(path, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := stsStoreOnFile(path).Store("irc.example", 6698, 86400, time.Unix(20, 0)); err != nil {
		t.Fatal(err)
	}

	cached, ok := ours.Lookup("irc.example", time.Unix(21, 0))
	if !ok || cached.Port != 6698 || cached.ExpiryEpochSeconds != 86420 {
		t.Fatalf("the newer port on disk must beat this store's unwritten 6697 policy: %+v ok=%v", cached, ok)
	}

	if err := ours.Refresh("irc.example", 86400, time.Unix(30, 0)); err != nil {
		t.Fatal(err)
	}
	entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 31)
	if !ok || entry.Port != 6698 || entry.ExpiryEpochSeconds != 86430 {
		t.Fatalf("a refresh must not write the unwritten 6697 policy back: %+v ok=%v", entry, ok)
	}
}

// TestSTSStoreAnotherClientClearAfterFailedWriteStaysCleared: another client clears
// the host after this store's write failed, and the disk recovers. Lookup answers
// none, and a refresh does not recreate the cleared group.
func TestSTSStoreAnotherClientClearAfterFailedWriteStaysCleared(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	path := filepath.Join(t.TempDir(), "sts")
	if err := stsStoreOnFile(path).Store("irc.example", 6697, 60, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	makeReadOnly(t, path)
	ours := stsStoreOnFile(path)
	if err := ours.Store("irc.example", 6697, 86400, time.Unix(0, 0)); err == nil {
		t.Fatal("a read-only file must refuse the write")
	}
	if err := os.Chmod(path, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := stsStoreOnFile(path).Store("irc.example", 6697, 0, time.Unix(10, 0)); err != nil {
		t.Fatal(err)
	}

	if cached, ok := ours.Lookup("irc.example", time.Unix(11, 0)); ok {
		t.Fatalf("the clear on disk must win over this store's unwritten policy: %+v", cached)
	}
	if err := ours.Refresh("irc.example", 86400, time.Unix(20, 0)); err != nil {
		t.Fatal(err)
	}

	if _, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 20); ok {
		t.Fatal("a refresh must not recreate the cleared host")
	}
	raw, err := os.ReadFile(path)
	if err == nil && strings.Contains(string(raw), storage.STSGroupKey("irc.example")) {
		t.Fatalf("cleared group must stay out of the file: %s", raw)
	}
}

// TestSTSStoreLapsedGroupDroppedByAnotherClientIsNotAClear: the 60s policy lapses,
// and another client's lookup then drops that group from the file. A lapsed group
// removed this way is not a clear, so the unwritten 86400 policy still holds.
func TestSTSStoreLapsedGroupDroppedByAnotherClientIsNotAClear(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	path := filepath.Join(t.TempDir(), "sts")
	if err := stsStoreOnFile(path).Store("irc.example", 6697, 60, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	makeReadOnly(t, path)
	ours := stsStoreOnFile(path)
	if err := ours.Store("irc.example", 6697, 86400, time.Unix(0, 0)); err == nil {
		t.Fatal("a read-only file must refuse the write")
	}
	if err := os.Chmod(path, 0o600); err != nil {
		t.Fatal(err)
	}
	storage.NewIniSTSStore(path).Lookup("irc.example", 100)

	cached, ok := ours.Lookup("irc.example", time.Unix(100, 0))
	if !ok || cached.ExpiryEpochSeconds != 86400 {
		t.Fatalf("a lapsed group another client dropped must not drop this store's unwritten policy: %+v ok=%v", cached, ok)
	}
}

// TestSTSStoreExpiryOnlyRefreshByAnotherClientKeepsUnwrittenPolicy: another client
// refreshes the 60s policy after this store's write failed. The refresh moves only
// the expiry, which is not a change, so the unwritten 86400s policy keeps answering
// after the recorded 60s expiry has passed.
func TestSTSStoreExpiryOnlyRefreshByAnotherClientKeepsUnwrittenPolicy(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	path := filepath.Join(t.TempDir(), "sts")
	if err := stsStoreOnFile(path).Store("irc.example", 6697, 60, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	makeReadOnly(t, path)
	ours := stsStoreOnFile(path)
	if err := ours.Store("irc.example", 6697, 86400, time.Unix(0, 0)); err == nil {
		t.Fatal("a read-only file must refuse the write")
	}
	if err := os.Chmod(path, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := stsStoreOnFile(path).Refresh("irc.example", 60, time.Unix(50, 0)); err != nil {
		t.Fatal(err)
	}

	for _, seconds := range []int64{70, 120} {
		cached, ok := ours.Lookup("irc.example", time.Unix(seconds, 0))
		if !ok || cached.ExpiryEpochSeconds != 86400 {
			t.Fatalf("at %ds the unwritten 86400s policy must hold over the refreshed 60s one: %+v ok=%v", seconds, cached, ok)
		}
	}
}

// TestSTSStoreMalformedFileIsNotAClearForUnwrittenPolicy: the file holds a 3600s
// policy when this store's write fails, then it is overwritten with garbage. A
// malformed file with no valid group for the host says nothing about it, so the
// unwritten 86400s policy still answers instead of the garbage reading as a clear.
func TestSTSStoreMalformedFileIsNotAClearForUnwrittenPolicy(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	path := filepath.Join(t.TempDir(), "sts")
	if err := stsStoreOnFile(path).Store("irc.example", 6697, 3600, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	makeReadOnly(t, path)
	ours := stsStoreOnFile(path)
	if err := ours.Store("irc.example", 6697, 86400, time.Unix(0, 0)); err == nil {
		t.Fatal("a read-only file must refuse the write")
	}
	if err := os.Chmod(path, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("this line has no equals sign\n"), 0o600); err != nil {
		t.Fatal(err)
	}

	cached, ok := ours.Lookup("irc.example", time.Unix(1, 0))
	if !ok || cached.ExpiryEpochSeconds != 86400 {
		t.Fatalf("a malformed file must not drop the unwritten 86400s policy: %+v ok=%v", cached, ok)
	}
}

// TestSTSStoreWriteMadeWhileUnreadableYieldsToLaterChange: the write fails while the
// file is unreadable, so nothing readable was recorded. Once the file reads again,
// its first view is the baseline, so another client's later save is seen, and a
// refresh does not write 6697 over it.
func TestSTSStoreWriteMadeWhileUnreadableYieldsToLaterChange(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	path := filepath.Join(t.TempDir(), "sts")
	if err := stsStoreOnFile(path).Store("irc.example", 6697, 60, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	makeUnreadable(t, path)
	ours := stsStoreOnFile(path)
	if err := ours.Store("irc.example", 6697, 86400, time.Unix(0, 0)); err == nil {
		t.Fatal("an unreadable file must refuse the write")
	}
	if err := os.Chmod(path, 0o600); err != nil {
		t.Fatal(err)
	}

	if cached, ok := ours.Lookup("irc.example", time.Unix(1, 0)); !ok || cached.ExpiryEpochSeconds != 86400 {
		t.Fatalf("the first readable lookup takes the baseline and still answers this store's policy: %+v ok=%v", cached, ok)
	}
	if err := stsStoreOnFile(path).Store("irc.example", 6698, 86400, time.Unix(2, 0)); err != nil {
		t.Fatal(err)
	}

	cached, ok := ours.Lookup("irc.example", time.Unix(3, 0))
	if !ok || cached.Port != 6698 || cached.ExpiryEpochSeconds != 86402 {
		t.Fatalf("the 6698 save after the baseline must win: %+v ok=%v", cached, ok)
	}
	if err := ours.Refresh("irc.example", 86400, time.Unix(4, 0)); err != nil {
		t.Fatal(err)
	}
	entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 5)
	if !ok || entry.Port != 6698 || entry.ExpiryEpochSeconds != 86404 {
		t.Fatalf("a refresh must not write 6697 over the 6698 save: %+v ok=%v", entry, ok)
	}
}

// TestSTSStoreRefreshOnUnreadableFileKeepsRecordedView: the file is readable when
// this store's write fails, then a refresh runs while it is unreadable. The refresh
// cannot write, so it must keep the view the policy was recorded against, and another
// client's later save is still seen once the file reads again.
func TestSTSStoreRefreshOnUnreadableFileKeepsRecordedView(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	path := filepath.Join(t.TempDir(), "sts")
	if err := stsStoreOnFile(path).Store("irc.example", 6697, 3600, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	makeReadOnly(t, path)
	ours := stsStoreOnFile(path)
	if err := ours.Store("irc.example", 6697, 86400, time.Unix(0, 0)); err == nil {
		t.Fatal("a read-only file must refuse the write")
	}
	makeUnreadable(t, path)
	if err := ours.Refresh("irc.example", 86400, time.Unix(10, 0)); err == nil {
		t.Fatal("an unreadable file must refuse the refresh write")
	}
	if err := os.Chmod(path, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := stsStoreOnFile(path).Store("irc.example", 6698, 3600, time.Unix(20, 0)); err != nil {
		t.Fatal(err)
	}

	cached, ok := ours.Lookup("irc.example", time.Unix(21, 0))
	if !ok || cached.Port != 6698 || cached.ExpiryEpochSeconds != 3620 {
		t.Fatalf("the 6698 save after the failed refresh must be seen as a change: %+v ok=%v", cached, ok)
	}
}

// TestSTSStoreRefreshThatCannotWriteKeepsFileView: another client's 3600s policy is
// on disk when this store's 86400s write fails. A refresh that also cannot write
// must leave the file's view as it was, so the unchanged file still yields to the
// unwritten policy, and a later save by another client is still seen.
func TestSTSStoreRefreshThatCannotWriteKeepsFileView(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	path := filepath.Join(t.TempDir(), "sts")
	if err := stsStoreOnFile(path).Store("irc.example", 6698, 3600, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	makeReadOnly(t, path)
	ours := stsStoreOnFile(path)
	if err := ours.Store("irc.example", 6697, 86400, time.Unix(0, 0)); err == nil {
		t.Fatal("a read-only file must refuse the write")
	}
	if err := ours.Refresh("irc.example", 86400, time.Unix(10, 0)); err == nil {
		t.Fatal("a read-only file must refuse the refresh write")
	}
	if err := os.Chmod(path, 0o600); err != nil {
		t.Fatal(err)
	}

	cached, ok := ours.Lookup("irc.example", time.Unix(11, 0))
	if !ok || cached.Port != 6697 || cached.ExpiryEpochSeconds != 86410 {
		t.Fatalf("an unchanged file must keep the unwritten policy after a failed refresh: %+v ok=%v", cached, ok)
	}
	if err := stsStoreOnFile(path).Store("irc.example", 6699, 3600, time.Unix(12, 0)); err != nil {
		t.Fatal(err)
	}
	cached, ok = ours.Lookup("irc.example", time.Unix(13, 0))
	if !ok || cached.Port != 6699 || cached.ExpiryEpochSeconds != 3612 {
		t.Fatalf("a later save by another client must still win: %+v ok=%v", cached, ok)
	}
}

// TestSTSStoreUnreadableFileKeepsUnwrittenPolicy: the file is unreadable when this
// store looks at it. Nothing can say the host moved, so the unwritten 86400 policy
// still answers.
func TestSTSStoreUnreadableFileKeepsUnwrittenPolicy(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	path := filepath.Join(t.TempDir(), "sts")
	if err := stsStoreOnFile(path).Store("irc.example", 6697, 60, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	makeReadOnly(t, path)
	ours := stsStoreOnFile(path)
	if err := ours.Store("irc.example", 6697, 86400, time.Unix(0, 0)); err == nil {
		t.Fatal("a read-only file must refuse the write")
	}
	makeUnreadable(t, path)

	cached, ok := ours.Lookup("irc.example", time.Unix(1, 0))
	if !ok || cached.ExpiryEpochSeconds != 86400 {
		t.Fatalf("an unreadable file must not drop the unwritten policy: %+v ok=%v", cached, ok)
	}
}

// makeUnreadable makes path unreadable and skips the test when the process can
// still read it (running as root).
func makeUnreadable(t *testing.T, path string) {
	t.Helper()
	if err := os.Chmod(path, 0o000); err != nil {
		t.Fatal(err)
	}
	if _, err := os.ReadFile(path); err == nil {
		_ = os.Chmod(path, 0o600)
		t.Skip("process can read a mode 000 file (running as root?); skipping")
	}
	t.Cleanup(func() { _ = os.Chmod(path, 0o600) })
}

// makeReadOnly makes path read-only and skips the test when the process can
// still write it (running as root).
func makeReadOnly(t *testing.T, path string) {
	t.Helper()
	if err := os.Chmod(path, 0o444); err != nil {
		t.Fatal(err)
	}
	if f, err := os.OpenFile(path, os.O_WRONLY, 0); err == nil {
		f.Close()
		_ = os.Chmod(path, 0o600)
		t.Skip("process can write a read-only file (running as root?); skipping")
	}
	t.Cleanup(func() { _ = os.Chmod(path, 0o600) })
}

// TestSTSStoreRefreshExtendsPolicyWhoseWriteFailed: a 86400s policy whose write
// failed (the file is malformed and cannot be recovered) is held in memory. A
// refresh of that duration extends it there, so it still holds past the first expiry.
func TestSTSStoreRefreshExtendsPolicyWhoseWriteFailed(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sts")
	if err := os.WriteFile(path, []byte("this line has no equals sign\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	store := stsStoreOnFile(path)
	restore := storage.TestingSetSTSHook("fail-backup", 0)
	defer restore()

	if err := store.Store("irc.example", 6697, 86400, time.Unix(0, 0)); err == nil {
		t.Fatal("the backup step must fail for this test")
	}
	_ = store.Refresh("irc.example", 86400, time.Unix(1000, 0))

	cached, ok := store.Lookup("irc.example", time.Unix(86500, 0))
	if !ok || cached.ExpiryEpochSeconds != 87400 {
		t.Fatalf("refresh must extend the unwritten policy to 1000+86400: %+v ok=%v", cached, ok)
	}
}

// TestSTSStoreLookupOnMalformedFileKeepsLiveMemoryPolicy: the file goes malformed
// and its group for the host has lapsed, but memory still holds the host's 3600s
// policy. Lookup answers from memory and leaves the file alone: no rewrite, no
// backup, and the host's group stays put, as the Rust client does.
func TestSTSStoreLookupOnMalformedFileKeepsLiveMemoryPolicy(t *testing.T) {
	path := filepath.Join(t.TempDir(), "sts")
	store := stsStoreOnFile(path)
	if err := store.Store("irc.example", 6697, 3600, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	key := storage.STSGroupKey("irc.example")
	lapsed := "this line has no equals sign\n[" + key + "]\nport=6697\nduration=60\nexpiry=60\n"
	if err := os.WriteFile(path, []byte(lapsed), 0o600); err != nil {
		t.Fatal(err)
	}

	cached, ok := store.Lookup("irc.example", time.Unix(100, 0))
	if !ok || cached.ExpiryEpochSeconds != 3600 {
		t.Fatalf("memory's live 3600s policy must answer: %+v ok=%v", cached, ok)
	}
	if _, ok := store.Lookup("irc.example", time.Unix(101, 0)); !ok {
		t.Fatal("the host must survive the first lookup")
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(raw) != lapsed {
		t.Fatalf("a lookup must not rewrite the malformed file while memory answers: %q", raw)
	}
	if _, err := os.Stat(filepath.Join(filepath.Dir(path), "sts.corrupt")); !os.IsNotExist(err) {
		t.Fatalf("a lookup must not back up the malformed file: %v", err)
	}
}

// stsOursOver3600 is this store's 6697/86400 policy written over the file's 6697/3600
// one. With saved the write lands, so the file holds 86400. Otherwise the write fails
// and the policy stays unsaved, with the 3600s policy as its baseline. The file is
// writable again on return.
func stsOursOver3600(t *testing.T, path string, saved bool) *STSStore {
	t.Helper()
	if err := stsStoreOnFile(path).Store("irc.example", 6697, 3600, time.Unix(0, 0)); err != nil {
		t.Fatal(err)
	}
	ours := stsStoreOnFile(path)
	if !saved {
		makeReadOnly(t, path)
	}
	err := ours.Store("irc.example", 6697, 86400, time.Unix(0, 0))
	if saved && err != nil {
		t.Fatal(err)
	}
	if !saved && err == nil {
		t.Fatal("a read-only file must refuse the write")
	}
	if err := os.Chmod(path, 0o600); err != nil {
		t.Fatal(err)
	}
	return ours
}

// TestSTSStoreMissingFileKeepsSavedAndUnwrittenPolicies: the file is deleted. A
// missing file says nothing, so the unsaved policy keeps answering and a refresh
// writes it back. The saved policy answers from memory, and a refresh has no file
// policy to extend, so it writes nothing.
func TestSTSStoreMissingFileKeepsSavedAndUnwrittenPolicies(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	for _, saved := range []bool{true, false} {
		path := filepath.Join(t.TempDir(), "sts")
		ours := stsOursOver3600(t, path, saved)
		if err := os.Remove(path); err != nil {
			t.Fatal(err)
		}

		if cached, ok := ours.Lookup("irc.example", time.Unix(1, 0)); !ok || cached.ExpiryEpochSeconds != 86400 {
			t.Fatalf("saved=%v: a missing file keeps the 86400s policy: %+v ok=%v", saved, cached, ok)
		}
		if err := ours.Refresh("irc.example", 86400, time.Unix(2, 0)); err != nil {
			t.Fatal(err)
		}
		entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 3)
		if ok != !saved || (ok && entry.ExpiryEpochSeconds != 86402) {
			t.Fatalf("saved=%v: refresh writes only the unsaved policy: %+v ok=%v", saved, entry, ok)
		}
	}
}

// TestSTSStoreUnreadableFileKeepsSavedAndUnwrittenPolicies: the file is unreadable
// when this store looks at it. Unreadable says nothing, so both kinds of policy keep
// answering.
func TestSTSStoreUnreadableFileKeepsSavedAndUnwrittenPolicies(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	for _, saved := range []bool{true, false} {
		path := filepath.Join(t.TempDir(), "sts")
		ours := stsOursOver3600(t, path, saved)
		makeUnreadable(t, path)

		cached, ok := ours.Lookup("irc.example", time.Unix(1, 0))
		if !ok || cached.ExpiryEpochSeconds != 86400 {
			t.Fatalf("saved=%v: an unreadable file keeps the 86400s policy: %+v ok=%v", saved, cached, ok)
		}
	}
}

// TestSTSStoreMalformedFileKeepsSavedAndUnwrittenPolicies: the file is overwritten
// with garbage. It says nothing about the host, so both kinds of policy keep
// answering. A refresh of the unsaved one recovers the file with it.
func TestSTSStoreMalformedFileKeepsSavedAndUnwrittenPolicies(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	for _, saved := range []bool{true, false} {
		path := filepath.Join(t.TempDir(), "sts")
		ours := stsOursOver3600(t, path, saved)
		if err := os.WriteFile(path, []byte("this line has no equals sign\n"), 0o600); err != nil {
			t.Fatal(err)
		}

		if cached, ok := ours.Lookup("irc.example", time.Unix(1, 0)); !ok || cached.ExpiryEpochSeconds != 86400 {
			t.Fatalf("saved=%v: a malformed file keeps the 86400s policy: %+v ok=%v", saved, cached, ok)
		}
		if !saved {
			if err := ours.Refresh("irc.example", 86400, time.Unix(2, 0)); err != nil {
				t.Fatal(err)
			}
			entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 3)
			if !ok || entry.ExpiryEpochSeconds != 86402 {
				t.Fatalf("the refresh recovers the file with the unsaved policy: %+v ok=%v", entry, ok)
			}
		}
	}
}

// TestSTSStoreInvalidGroupIsNotAClearForUnwrittenPolicy: a well-formed file whose group
// for the host is present but is not a valid policy says nothing about the host, so it
// is not a clear. The unsaved policy keeps answering, and a refresh writes it over the
// invalid group.
func TestSTSStoreInvalidGroupIsNotAClearForUnwrittenPolicy(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	key := storage.STSGroupKey("irc.example")
	for _, saved := range []bool{true, false} {
		path := filepath.Join(t.TempDir(), "sts")
		ours := stsOursOver3600(t, path, saved)
		invalid := "[" + key + "]\nport=0\nduration=3600\nexpiry=3600\n"
		if err := os.WriteFile(path, []byte(invalid), 0o600); err != nil {
			t.Fatal(err)
		}

		if cached, ok := ours.Lookup("irc.example", time.Unix(1, 0)); !ok || cached.ExpiryEpochSeconds != 86400 {
			t.Fatalf("saved=%v: an invalid group is not a clear for the 86400s policy: %+v ok=%v", saved, cached, ok)
		}
		if !saved {
			if err := ours.Refresh("irc.example", 86400, time.Unix(2, 0)); err != nil {
				t.Fatal(err)
			}
			entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 3)
			if !ok || entry.Port != 6697 || entry.ExpiryEpochSeconds != 86402 {
				t.Fatalf("the refresh writes the unsaved policy over the invalid group: %+v ok=%v", entry, ok)
			}
		}
	}
}

// TestSTSStoreCleanRewriteAfterDamageKeepsUnwrittenPolicy is finding 1 from another
// client. The file is damaged and this store looks at it. Another client then rewrites
// it clean without this host, which becomes the new baseline, so the unsaved policy
// keeps answering. A later save by that client is a change from that baseline, and
// the file answers.
func TestSTSStoreCleanRewriteAfterDamageKeepsUnwrittenPolicy(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	other := storage.STSGroupKey("other.example")
	for _, saved := range []bool{true, false} {
		path := filepath.Join(t.TempDir(), "sts")
		ours := stsOursOver3600(t, path, saved)
		if err := os.WriteFile(path, []byte("this line has no equals sign\n"), 0o600); err != nil {
			t.Fatal(err)
		}
		if cached, ok := ours.Lookup("irc.example", time.Unix(1, 0)); !ok || cached.ExpiryEpochSeconds != 86400 {
			t.Fatalf("saved=%v: the damaged file keeps the policy: %+v ok=%v", saved, cached, ok)
		}

		clean := "[" + other + "]\nport=6698\nduration=60\nexpiry=4102444800\n"
		if err := os.WriteFile(path, []byte(clean), 0o600); err != nil {
			t.Fatal(err)
		}
		if cached, ok := ours.Lookup("irc.example", time.Unix(2, 0)); !ok || cached.ExpiryEpochSeconds != 86400 {
			t.Fatalf("saved=%v: a clean view without the host keeps the policy: %+v ok=%v", saved, cached, ok)
		}

		if err := stsStoreOnFile(path).Store("irc.example", 6698, 3600, time.Unix(3, 0)); err != nil {
			t.Fatal(err)
		}
		if cached, ok := ours.Lookup("irc.example", time.Unix(4, 0)); !ok || cached.Port != 6698 || cached.ExpiryEpochSeconds != 3603 {
			t.Fatalf("saved=%v: a later save by another client wins: %+v ok=%v", saved, cached, ok)
		}
	}
}

// TestSTSStoreCleanRewriteNobodyHereSawDamagedIsAClear is known gap 1. The file is
// damaged and this store never looks at it. Another client's recovery then rewrites it
// clean without this host. From this store's side that is a clear of a live baseline,
// so the unsaved policy yields. The saved policy still answers from memory.
func TestSTSStoreCleanRewriteNobodyHereSawDamagedIsAClear(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	for _, saved := range []bool{true, false} {
		path := filepath.Join(t.TempDir(), "sts")
		ours := stsOursOver3600(t, path, saved)
		if err := os.WriteFile(path, []byte("this line has no equals sign\n"), 0o600); err != nil {
			t.Fatal(err)
		}
		if err := stsStoreOnFile(path).Store("other.example", 6698, 60, time.Unix(2, 0)); err != nil {
			t.Fatal(err)
		}

		// Saved answers from memory. Unsaved yields to the clean view, a clear of a live baseline.
		cached, ok := ours.Lookup("irc.example", time.Unix(3, 0))
		if ok != saved || (ok && cached.ExpiryEpochSeconds != 86400) {
			t.Fatalf("saved=%v: the unseen rewrite is a clear: %+v ok=%v", saved, cached, ok)
		}
	}
}

// TestSTSStoreNewerDurationAnotherClientSavesWins: another client saves the host with
// another duration. The file's policy answers, and a refresh with this store's duration
// does not write the old policy back.
func TestSTSStoreNewerDurationAnotherClientSavesWins(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	for _, saved := range []bool{true, false} {
		path := filepath.Join(t.TempDir(), "sts")
		ours := stsOursOver3600(t, path, saved)
		if err := stsStoreOnFile(path).Store("irc.example", 6697, 60, time.Unix(10, 0)); err != nil {
			t.Fatal(err)
		}

		if cached, ok := ours.Lookup("irc.example", time.Unix(11, 0)); !ok || cached.DurationSeconds != 60 || cached.ExpiryEpochSeconds != 70 {
			t.Fatalf("saved=%v: the other client's 60s policy wins: %+v ok=%v", saved, cached, ok)
		}
		if err := ours.Refresh("irc.example", 86400, time.Unix(12, 0)); err != nil {
			t.Fatal(err)
		}
		entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 13)
		if !ok || entry.DurationSeconds != 60 || entry.ExpiryEpochSeconds != 70 {
			t.Fatalf("saved=%v: a refresh must not write the 86400s policy back: %+v ok=%v", saved, entry, ok)
		}
	}
}

// TestSTSStoreLapsedNewerPolicyOnDiskDoesNotReplaceUnwrittenPolicy: another client saves
// a different port that has already lapsed. A lapsed group is not in force, so it does
// not replace a live policy.
func TestSTSStoreLapsedNewerPolicyOnDiskDoesNotReplaceUnwrittenPolicy(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	for _, saved := range []bool{true, false} {
		path := filepath.Join(t.TempDir(), "sts")
		ours := stsOursOver3600(t, path, saved)
		if err := stsStoreOnFile(path).Store("irc.example", 6698, 60, time.Unix(3000, 0)); err != nil {
			t.Fatal(err)
		}

		cached, ok := ours.Lookup("irc.example", time.Unix(3100, 0))
		if !ok || cached.Port != 6697 || cached.ExpiryEpochSeconds != 86400 {
			t.Fatalf("saved=%v: the lapsed 6698 group must not replace this store's policy: %+v ok=%v", saved, cached, ok)
		}
	}
}

// TestSTSStoreClearOnDiskYieldsUnwrittenPolicyNotSaved: another client clears the host.
// A clear ends an unsaved policy, and a refresh does not recreate the group. A saved
// policy keeps answering from memory, which is known gap 4.
func TestSTSStoreClearOnDiskYieldsUnwrittenPolicyNotSaved(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	for _, saved := range []bool{true, false} {
		path := filepath.Join(t.TempDir(), "sts")
		ours := stsOursOver3600(t, path, saved)
		if err := stsStoreOnFile(path).Store("irc.example", 6697, 0, time.Unix(10, 0)); err != nil {
			t.Fatal(err)
		}

		cached, ok := ours.Lookup("irc.example", time.Unix(11, 0))
		if ok != saved || (ok && cached.ExpiryEpochSeconds != 86400) {
			t.Fatalf("saved=%v: a clear on disk: %+v ok=%v", saved, cached, ok)
		}
		if err := ours.Refresh("irc.example", 86400, time.Unix(20, 0)); err != nil {
			t.Fatal(err)
		}
		raw, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		if strings.Contains(string(raw), storage.STSGroupKey("irc.example")) {
			t.Fatalf("saved=%v: a refresh must not recreate the cleared host: %q", saved, raw)
		}
	}
}

// TestSTSStoreExpiryOnlyRefreshByAnotherClientIsNotAChange: another client refreshes the
// host with the same port and duration. That moves only the expiry, so both kinds of
// policy keep answering, and the saved one reads the later expiry from the file.
func TestSTSStoreExpiryOnlyRefreshByAnotherClientIsNotAChange(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	for _, saved := range []bool{true, false} {
		path := filepath.Join(t.TempDir(), "sts")
		ours := stsOursOver3600(t, path, saved)
		duration, wantExpiry := int64(3600), int64(86400)
		if saved {
			duration, wantExpiry = 86400, 86450
		}
		if err := stsStoreOnFile(path).Refresh("irc.example", duration, time.Unix(50, 0)); err != nil {
			t.Fatal(err)
		}

		for _, seconds := range []int64{70, 120} {
			cached, ok := ours.Lookup("irc.example", time.Unix(seconds, 0))
			if !ok || cached.Port != 6697 || cached.DurationSeconds != 86400 || cached.ExpiryEpochSeconds != wantExpiry {
				t.Fatalf("saved=%v at %ds: %+v ok=%v", saved, seconds, cached, ok)
			}
		}
	}
}

// TestSessionStoreForAnotherHostKeepsUnwrittenPolicyOverDamage: a store for another host
// keeps this host's unsaved policy, so the write merges it into the file and marks it
// saved. A fresh reader then finds it there. Covers an invalid group (dropped by the
// write) and a malformed file (recovered by the write).
func TestSessionStoreForAnotherHostKeepsUnwrittenPolicyOverDamage(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	key := storage.STSGroupKey("irc.example")
	damages := []string{
		"[" + key + "]\nport=0\nduration=3600\nexpiry=3600\n",
		"this line has no equals sign\n",
	}
	for _, damage := range damages {
		path := filepath.Join(t.TempDir(), "sts")
		ours := stsOursOver3600(t, path, false)
		if err := os.WriteFile(path, []byte(damage), 0o600); err != nil {
			t.Fatal(err)
		}

		if err := ours.Store("other.example", 6699, 60, time.Unix(2, 0)); err != nil {
			t.Fatal(err)
		}
		entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 3)
		if !ok || entry.Port != 6697 || entry.ExpiryEpochSeconds != 86400 {
			t.Fatalf("the write for another host lands this store's unwritten policy: %+v ok=%v", entry, ok)
		}
		other, ok := storage.NewIniSTSStore(path).Lookup("other.example", 3)
		if !ok || other.Port != 6699 || other.ExpiryEpochSeconds != 62 {
			t.Fatalf("the other host is written too: %+v ok=%v", other, ok)
		}
	}
}

// TestSessionWriteForAnotherHostDoesNotWriteBackYieldedPolicy: another client saves a
// newer port for the host after this store's write failed. A store for another host then
// must not write this store's yielded 6697 policy back over that save.
func TestSessionWriteForAnotherHostDoesNotWriteBackYieldedPolicy(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("unix mode bits")
	}
	path := filepath.Join(t.TempDir(), "sts")
	ours := stsOursOver3600(t, path, false)
	if err := stsStoreOnFile(path).Store("irc.example", 6698, 3600, time.Unix(10, 0)); err != nil {
		t.Fatal(err)
	}

	if err := ours.Store("other.example", 6699, 60, time.Unix(20, 0)); err != nil {
		t.Fatal(err)
	}
	entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", 21)
	if !ok || entry.Port != 6698 || entry.ExpiryEpochSeconds != 3610 {
		t.Fatalf("a store for another host must not write the yielded 6697 policy back: %+v ok=%v", entry, ok)
	}
	other, ok := storage.NewIniSTSStore(path).Lookup("other.example", 21)
	if !ok || other.Port != 6699 || other.ExpiryEpochSeconds != 80 {
		t.Fatalf("the other host is written: %+v ok=%v", other, ok)
	}
}
