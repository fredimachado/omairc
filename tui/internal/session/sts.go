package session

import (
	"errors"
	"math"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/fredimachado/omairc/tui/internal/storage"
)

// STS (Strict Transport Security) support.
// The `STSStore` is the process STS policy cache (MemoryStsStore in
// `client/src/session/sts.rs`). Durable INI persistence is
// `storage.IniSTSStore` (`client/src/storage/stspolicies.rs`). A store updates
// memory first and keeps a policy whose write failed there until a write lands,
// or until another client changes that host in the file. Lookups answer with
// that unsaved policy first, then from the file when it holds a live policy for
// the host, and from memory when it does not. A disk failure never drops a
// policy that already applies in memory, and a corrupt file never forces a
// plaintext connect for a host that still has a valid in-memory entry.

// STSAdvertisement is the parsed value of one `sts` CAP token. A nil field
// means the advertisement did not carry that key. It mirrors
// IrcStsAdvertisement.
type STSAdvertisement struct {
	Port            *uint16
	DurationSeconds *int64
}

// STSCached is one cached STS policy. It mirrors IrcStsCached.
type STSCached struct {
	Port               uint16
	DurationSeconds    int64
	ExpiryEpochSeconds int64
}

// ParseSTSAdvertisement scans CAP LS/NEW tokens for the last `sts` token and
// parses its value. ok is false when no `sts` token was present; a bare `sts`
// token yields a zero advertisement with ok true. It mirrors
// parseIrcStsAdvertisement / `parse_sts_advertisement`.
func ParseSTSAdvertisement(tokens []string) (STSAdvertisement, bool) {
	var advertisement STSAdvertisement
	found := false
	for _, token := range tokens {
		if !strings.EqualFold(stsTokenName(token), "sts") {
			continue
		}
		advertisement = parseSTSValue(stsTokenValue(token))
		found = true
	}
	return advertisement, found
}

// parseSTSValue parses a comma-separated `key=value` list. The first valid
// port and duration win; a malformed or out-of-range value is ignored. It
// mirrors parseStsValue / `parse_sts_value`.
func parseSTSValue(value string) STSAdvertisement {
	var advertisement STSAdvertisement
	for _, part := range strings.Split(value, ",") {
		if part == "" {
			continue
		}
		key := strings.ToLower(stsTokenName(part))
		raw := stsTokenValue(part)
		switch key {
		case "port":
			if advertisement.Port != nil {
				continue
			}
			port, err := strconv.ParseUint(raw, 10, 32)
			if err == nil && port >= 1 && port <= 65535 {
				value := uint16(port)
				advertisement.Port = &value
			}
		case "duration":
			if advertisement.DurationSeconds != nil {
				continue
			}
			duration, err := strconv.ParseInt(raw, 10, 64)
			if err == nil && duration >= 0 {
				value := duration
				advertisement.DurationSeconds = &value
			}
		}
	}
	return advertisement
}

func stsTokenName(token string) string {
	if separator := strings.IndexByte(token, '='); separator != -1 {
		return token[:separator]
	}
	return token
}

func stsTokenValue(token string) string {
	if separator := strings.IndexByte(token, '='); separator != -1 {
		return token[separator+1:]
	}
	return ""
}

// stsFileView is what the disk store held for one host when it was read.
// unreadable means the file says nothing about the host. policy is nil when a
// well-formed file has no group for the host, which is a clear.
type stsFileView struct {
	unreadable bool
	policy     *STSCached
}

// identity is the port and duration of the policy the view holds, with the
// expiry left out. It is the zero STSCached when the group is missing.
func (v stsFileView) identity() STSCached {
	if v.policy == nil {
		return STSCached{}
	}
	return STSCached{Port: v.policy.Port, DurationSeconds: v.policy.DurationSeconds}
}

// live is identity while the policy has not lapsed at nowUnix. It is the zero
// STSCached when the group is missing or has lapsed.
func (v stsFileView) live(nowUnix int64) STSCached {
	if v.policy == nil || v.policy.ExpiryEpochSeconds <= nowUnix {
		return STSCached{}
	}
	return v.identity()
}

// stsFileMoved reports whether the file's policy for a host changed since
// recorded was read: another client saved a different port or duration, or
// cleared the host. An unreadable file on either side says nothing. It mirrors
// file_moved in client/src/storage/stspolicies.rs.
func stsFileMoved(recorded, now stsFileView, nowUnix int64) bool {
	if recorded.unreadable || now.unreadable {
		return false
	}
	// A refresh by another client moves only the expiry. The same port and
	// duration is never a change, even once the recorded expiry has passed.
	if recorded.identity() == now.identity() {
		return false
	}
	return recorded.live(nowUnix) != now.live(nowUnix)
}

// stsHeld is one policy in memory. unpersisted is set when a write of the policy
// failed, so the file does not hold it. It holds the file's view of the host
// from when that write failed, so a later change by another client lets the file
// answer.
type stsHeld struct {
	cached      STSCached
	unpersisted *stsFileView
}

// STSStore is the process STS policy cache. Lookup expires through the caller's
// now, so a FakeClock can drive expiry deterministically. An optional disk
// store persists learned policies across restarts. The file answers first when
// it holds a live policy; memory answers when it does not, so a corrupt or
// unwritable file never weakens a policy this process learned.
type STSStore struct {
	mu      sync.Mutex
	entries map[string]stsHeld
	disk    *storage.IniSTSStore
}

// NewSTSStore returns an empty in-memory store.
func NewSTSStore() *STSStore {
	return &STSStore{entries: make(map[string]stsHeld)}
}

// AttachDisk installs the durable INI store. nil clears persistence. Existing
// in-memory entries are kept: attaching a corrupt disk must not weaken them.
func (s *STSStore) AttachDisk(disk *storage.IniSTSStore) {
	if s == nil {
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.disk = disk
}

// Lookup returns the cached policy for host at now, dropping an invalid or
// expired entry. An unpersisted policy answers first while the file has not
// moved on from it, so the file never replaces it. Then the file answers when it
// holds a live policy for host, and that policy is hydrated into memory, so a
// stale memory entry never beats a newer one another client saved. Memory
// answers last, when the file has none. A malformed file is recovered through
// flushRecoveryLocked so other in-memory hosts are not dropped. It mirrors
// StsStore::lookup.
func (s *STSStore) Lookup(host string, now time.Time) (STSCached, bool) {
	key := stsGroupKey(host)
	if key == "" {
		return STSCached{}, false
	}
	nowUnix := now.Unix()
	s.mu.Lock()
	defer s.mu.Unlock()
	view := s.fileViewLocked(host)
	if cached, ok := s.unpersistedLocked(key, view, nowUnix); ok && cached.ExpiryEpochSeconds > nowUnix {
		return cached, true
	}
	if cached, ok := s.lookupDiskLocked(host, nowUnix); ok {
		s.entries[key] = stsHeld{cached: cached}
		return cached, true
	}
	if held, ok := s.entries[key]; ok {
		if held.cached.Port >= 1 && held.cached.Port <= 65535 && held.cached.ExpiryEpochSeconds > nowUnix {
			return held.cached, true
		}
		delete(s.entries, key)
	}
	return STSCached{}, false
}

// lookupDiskLocked reads host's live policy from the disk store, or reports
// false when the file has none. Caller must hold s.mu.
func (s *STSStore) lookupDiskLocked(host string, nowUnix int64) (STSCached, bool) {
	if s.disk == nil {
		return STSCached{}, false
	}
	if storage.ProbeSettingsIni(s.disk.Path()) == storage.IniProbeMalformed {
		return s.lookupMalformedDiskLocked(host, nowUnix)
	}
	entry, ok := s.disk.Lookup(host, nowUnix)
	if !ok {
		return STSCached{}, false
	}
	return STSCached{
		Port:               entry.Port,
		DurationSeconds:    entry.DurationSeconds,
		ExpiryEpochSeconds: entry.ExpiryEpochSeconds,
	}, true
}

// lookupMalformedDiskLocked inspects a malformed sts file without a blind
// rewrite. A stale group triggers flushRecoveryLocked so unexpired in-memory
// hosts survive. Caller must hold s.mu.
func (s *STSStore) lookupMalformedDiskLocked(host string, nowUnix int64) (STSCached, bool) {
	entry, present, stale := s.disk.Inspect(host, nowUnix)
	if stale {
		// Memory that still holds the host answers in Lookup, so recovery must not clear it.
		if held, ok := s.entries[stsGroupKey(host)]; ok && held.cached.ExpiryEpochSeconds > nowUnix {
			return STSCached{}, false
		}
		_ = s.flushRecoveryLocked(nowUnix, true, host)
		return STSCached{}, false
	}
	if !present {
		return STSCached{}, false
	}
	return STSCached{
		Port:               entry.Port,
		DurationSeconds:    entry.DurationSeconds,
		ExpiryEpochSeconds: entry.ExpiryEpochSeconds,
	}, true
}

// Store caches a policy for host at now. A zero duration clears the entry, and
// an empty host or a zero port is ignored. Memory is updated first; a disk
// failure is returned without rolling memory back. The mutex covers the disk
// write so concurrent Stores from shared-manager sessions cannot lose hosts.
// A well-formed file gets only this host's mutation. Malformed-file recovery
// flushes every unexpired in-memory entry so a host that only lives in memory
// is not dropped, without resurrecting a host another client cleared on disk.
func (s *STSStore) Store(host string, port uint16, durationSeconds int64, now time.Time) error {
	if durationSeconds == 0 {
		return s.clearAt(host, now.Unix())
	}
	key := stsGroupKey(host)
	if key == "" || port < 1 {
		return nil
	}
	cached := STSCached{
		Port:               port,
		DurationSeconds:    durationSeconds,
		ExpiryEpochSeconds: stsExpiry(now.Unix(), durationSeconds),
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	// Unpersisted until a write lands: a failed write keeps the policy in memory,
	// and the next recovery writes it into the file. The file's view before this
	// write is what the file still holds if the write fails.
	view := s.fileViewLocked(host)
	s.entries[key] = stsHeld{cached: cached, unpersisted: &view}
	err := s.persistLocked(host, port, durationSeconds, now.Unix(), false)
	s.settleLocked(key, err)
	return err
}

// Refresh extends host's policy to now plus durationSeconds, for a connection
// that closes holding that duration. It writes only when the cache still holds
// host with that same duration: a policy another client replaced or cleared is
// left alone, and a missing one is not recreated. With no disk store the
// in-memory entry is refreshed the same way. It mirrors StsStore::refresh.
func (s *STSStore) Refresh(host string, durationSeconds int64, now time.Time) error {
	key := stsGroupKey(host)
	if key == "" || durationSeconds <= 0 {
		return nil
	}
	nowUnix := now.Unix()
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.disk == nil {
		if held, ok := s.entries[key]; ok && held.cached.DurationSeconds == durationSeconds {
			held.cached.ExpiryEpochSeconds = stsExpiry(nowUnix, durationSeconds)
			s.entries[key] = held
		}
		return nil
	}
	view := s.fileViewLocked(host)
	// An unpersisted policy is the one lookup serves, so a refresh extends it. Otherwise the file's.
	cached, ok := s.unpersistedLocked(key, view, nowUnix)
	if !ok {
		if view.policy == nil {
			return nil
		}
		cached = *view.policy
	}
	if cached.DurationSeconds != durationSeconds {
		return nil
	}
	cached.ExpiryEpochSeconds = stsExpiry(nowUnix, durationSeconds)
	s.entries[key] = stsHeld{cached: cached, unpersisted: &view}
	err := s.persistLocked(host, cached.Port, durationSeconds, nowUnix, false)
	s.settleLocked(key, err)
	return err
}

// fileViewLocked reads host's group from the disk store. Caller must hold s.mu.
func (s *STSStore) fileViewLocked(host string) stsFileView {
	if s.disk == nil {
		return stsFileView{}
	}
	entry, ok, readable := s.disk.Group(host)
	if !readable {
		return stsFileView{unreadable: true}
	}
	if !ok {
		return stsFileView{}
	}
	return stsFileView{policy: &STSCached{
		Port:               entry.Port,
		DurationSeconds:    entry.DurationSeconds,
		ExpiryEpochSeconds: entry.ExpiryEpochSeconds,
	}}
}

// unpersistedLocked returns key's unpersisted policy while the file has not moved
// on from the view its write failed against. Once it has, another client saved or
// cleared the host: the unpersisted copy is dropped, and the file answers. Caller
// must hold s.mu.
func (s *STSStore) unpersistedLocked(key string, view stsFileView, nowUnix int64) (STSCached, bool) {
	held, ok := s.entries[key]
	if !ok || held.unpersisted == nil {
		return STSCached{}, false
	}
	if stsFileMoved(*held.unpersisted, view, nowUnix) {
		delete(s.entries, key)
		return STSCached{}, false
	}
	return held.cached, true
}

// Clear drops the cached policy for host. Memory is cleared first; a disk
// failure is returned without restoring memory.
func (s *STSStore) Clear(host string) error {
	return s.clearAt(host, time.Now().Unix())
}

func (s *STSStore) clearAt(host string, nowUnix int64) error {
	key := stsGroupKey(host)
	if key == "" {
		return nil
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.entries, key)
	return s.persistLocked(host, 0, 0, nowUnix, true)
}

// persistLocked writes one mutation to a well-formed file, or recovers a
// malformed file by flushing every unexpired in-memory entry. Caller must hold s.mu.
func (s *STSStore) persistLocked(host string, port uint16, durationSeconds, nowUnix int64, clearing bool) error {
	if s.disk == nil {
		return nil
	}
	path := s.disk.Path()
	if storage.ProbeSettingsIni(path) == storage.IniProbeMalformed {
		return s.flushRecoveryLocked(nowUnix, clearing, host)
	}
	if clearing {
		return stsPersistStatus(s.disk.Clear(host))
	}
	return stsPersistStatus(s.disk.Store(host, port, durationSeconds, nowUnix))
}

// flushRecoveryLocked merges every unexpired in-memory entry into a recovered
// clean file. Expired entries are skipped so they cannot return to disk. Once the
// write lands, the unsaved entries it carried are in the file.
func (s *STSStore) flushRecoveryLocked(nowUnix int64, clearing bool, clearHost string) error {
	unsaved := make(map[string]storage.STSEntry)
	known := make(map[string]storage.STSEntry, len(s.entries))
	for key, held := range s.entries {
		if held.cached.Port < 1 || held.cached.ExpiryEpochSeconds <= nowUnix {
			continue
		}
		entry := storage.STSEntry{
			Port:               held.cached.Port,
			DurationSeconds:    held.cached.DurationSeconds,
			ExpiryEpochSeconds: held.cached.ExpiryEpochSeconds,
		}
		if held.unpersisted != nil {
			unsaved[key] = entry
		} else {
			known[key] = entry
		}
	}
	var clearKeys []string
	if clearing {
		if key := stsGroupKey(clearHost); key != "" {
			clearKeys = []string{key}
			delete(unsaved, key)
			delete(known, key)
		}
	}
	err := stsPersistStatus(s.disk.MergeEntries(unsaved, known, clearKeys, nowUnix))
	if err == nil {
		for key := range unsaved {
			s.settleLocked(key, nil)
		}
	}
	return err
}

// settleLocked marks key's policy as held by the file once a write has landed.
// A failed write leaves it unpersisted.
func (s *STSStore) settleLocked(key string, err error) {
	if held, ok := s.entries[key]; ok && err == nil {
		held.unpersisted = nil
		s.entries[key] = held
	}
}

func stsPersistStatus(status storage.Status) error {
	switch status {
	case storage.StatusWritten, storage.StatusAbsent:
		return nil
	case storage.StatusFormatError:
		return errors.New("STS policy file is malformed")
	case storage.StatusAccessError:
		return errors.New("STS policy file is not writable")
	default:
		return errors.New("STS policy file could not be saved")
	}
}

func stsExpiry(nowUnix, durationSeconds int64) int64 {
	if durationSeconds <= 0 {
		return nowUnix
	}
	if nowUnix > math.MaxInt64-durationSeconds {
		return math.MaxInt64
	}
	return nowUnix + durationSeconds
}

// stsGroupKey is storage.STSGroupKey: the SHA-256 of the folded host.
func stsGroupKey(host string) string {
	return storage.STSGroupKey(host)
}
