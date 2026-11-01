package session

import (
	"math"
	"os"
	"path/filepath"
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
	if status := storage.NewIniSTSStore(path).Store("irc.example", 6697, 60, now.Unix()); status != storage.StatusWritten {
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
