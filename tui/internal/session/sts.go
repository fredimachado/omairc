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
// The in-memory `STSStore` is the process source of truth (MemoryStsStore in
// `client/src/session/sts.rs`). Durable INI persistence is
// `storage.IniSTSStore` (`client/src/storage/stspolicies.rs`). Memory is
// updated first; a disk failure never drops a policy that already applies in
// memory, and a corrupt file never forces a plaintext connect for a host that
// still has a valid in-memory entry.

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

// STSStore is the process STS policy cache. Lookup expires through the caller's
// now, so a FakeClock can drive expiry deterministically. An optional disk
// store persists learned policies across restarts; memory always wins over a
// corrupt or unwritable file.
type STSStore struct {
	mu      sync.Mutex
	entries map[string]STSCached
	disk    *storage.IniSTSStore
}

// NewSTSStore returns an empty in-memory store.
func NewSTSStore() *STSStore {
	return &STSStore{entries: make(map[string]STSCached)}
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
// expired entry. Memory is checked first; on a miss the disk store is consulted
// and hydrated into memory. It mirrors StsStore::lookup.
func (s *STSStore) Lookup(host string, now time.Time) (STSCached, bool) {
	key := stsGroupKey(host)
	if key == "" {
		return STSCached{}, false
	}
	nowUnix := now.Unix()
	s.mu.Lock()
	defer s.mu.Unlock()
	if entry, ok := s.entries[key]; ok {
		if entry.Port >= 1 && entry.Port <= 65535 && entry.ExpiryEpochSeconds > nowUnix {
			return entry, true
		}
		delete(s.entries, key)
	}
	if s.disk == nil {
		return STSCached{}, false
	}
	entry, ok := s.disk.Lookup(host, nowUnix)
	if !ok {
		return STSCached{}, false
	}
	cached := STSCached{
		Port:               entry.Port,
		DurationSeconds:    entry.DurationSeconds,
		ExpiryEpochSeconds: entry.ExpiryEpochSeconds,
	}
	s.entries[key] = cached
	return cached, true
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
	s.entries[key] = cached
	return s.persistLocked(host, port, durationSeconds, now.Unix(), false)
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
// clean file. Expired entries are skipped so they cannot return to disk.
func (s *STSStore) flushRecoveryLocked(nowUnix int64, clearing bool, clearHost string) error {
	entries := make(map[string]storage.STSEntry, len(s.entries))
	for key, cached := range s.entries {
		if cached.Port < 1 || cached.ExpiryEpochSeconds <= nowUnix {
			continue
		}
		entries[key] = storage.STSEntry{
			Port:               cached.Port,
			DurationSeconds:    cached.DurationSeconds,
			ExpiryEpochSeconds: cached.ExpiryEpochSeconds,
		}
	}
	var clearKeys []string
	if clearing {
		if key := stsGroupKey(clearHost); key != "" {
			clearKeys = []string{key}
			delete(entries, key)
		}
	}
	return stsPersistStatus(s.disk.MergeEntries(entries, clearKeys, nowUnix))
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
