package storage

import (
	"crypto/sha256"
	"encoding/hex"
	"math"
	"strconv"
	"strings"
)

// The STS policy cache on disk: the `sts` INI file beside the settings file, in
// the format IrcStsStore / desktop IniStsStore write. Each host is one group,
// named by its STSGroupKey, holding port, duration, and expiry (epoch seconds).
// Desktop and this client read and write the same entries.
// The file goes through Settings, so a malformed or unreadable file is never
// overwritten. Sync already restricts the file to its owner (0600).

const (
	stsPortKey     = "port"
	stsDurationKey = "duration"
	stsExpiryKey   = "expiry"
)

// STSEntry is one cached STS policy on disk. It mirrors IrcStsCached /
// session.STSCached without importing session.
type STSEntry struct {
	Port               uint16
	DurationSeconds    int64
	ExpiryEpochSeconds int64
}

// IniSTSStore is the on-disk STS cache at Path. Every call reads or writes the
// file, so an entry another client wrote is seen on the next call. It mirrors
// client/src/storage/stspolicies.rs IniStsStore.
type IniSTSStore struct {
	path string
}

// NewIniSTSStore returns a cache in the file at path. The file and its folder
// are created on the first write. An empty path never reads or writes.
func NewIniSTSStore(path string) *IniSTSStore {
	return &IniSTSStore{path: path}
}

// Path returns the backing file path.
func (s *IniSTSStore) Path() string {
	return s.path
}

// Lookup returns the cached policy for host at nowUnix, or ok false when there
// is none or it has expired. An expired or invalid entry is removed from the
// file. It mirrors IniStsStore::lookup.
func (s *IniSTSStore) Lookup(host string, nowUnix int64) (STSEntry, bool) {
	key := STSGroupKey(host)
	if key == "" || s.path == "" {
		return STSEntry{}, false
	}
	settings := OpenSettings(s.path)
	if !settings.Contains(key, stsPortKey) || !settings.Contains(key, stsExpiryKey) {
		return STSEntry{}, false
	}
	port := settings.Uint(key, stsPortKey, 0)
	durationSeconds := settings.Int64(key, stsDurationKey, 0)
	expiryEpochSeconds := settings.Int64(key, stsExpiryKey, 0)

	if port < 1 || port > 65535 {
		s.remove(key)
		return STSEntry{}, false
	}
	if expiryEpochSeconds <= nowUnix {
		s.remove(key)
		return STSEntry{}, false
	}
	return STSEntry{
		Port:               uint16(port),
		DurationSeconds:    durationSeconds,
		ExpiryEpochSeconds: expiryEpochSeconds,
	}, true
}

// Store caches port for durationSeconds from nowUnix. A zero duration clears
// the host. An empty host or a zero port is ignored. It mirrors
// IniStsStore::store. A malformed or unwritable file is left alone
// (StatusFormatError / StatusAccessError); the caller keeps any in-memory
// policy.
func (s *IniSTSStore) Store(host string, port uint16, durationSeconds, nowUnix int64) Status {
	if durationSeconds == 0 {
		return s.Clear(host)
	}
	key := STSGroupKey(host)
	if key == "" || port < 1 || s.path == "" {
		return StatusWritten
	}
	expiry := stsSaturatingExpiry(nowUnix, durationSeconds)
	settings := OpenSettings(s.path)
	if blocked := settings.WriteBlocked(); blocked != StatusWritten {
		return blocked
	}
	settings.SetUint(key, stsPortKey, uint64(port))
	settings.SetValue(key, stsDurationKey, strconv.FormatInt(durationSeconds, 10))
	settings.SetValue(key, stsExpiryKey, strconv.FormatInt(expiry, 10))
	return settings.Sync()
}

// Clear drops the policy for host, if any. It mirrors IniStsStore::clear.
func (s *IniSTSStore) Clear(host string) Status {
	key := STSGroupKey(host)
	if key == "" || s.path == "" {
		return StatusWritten
	}
	return s.remove(key)
}

// remove drops group key. Nothing is written when the group is not there.
func (s *IniSTSStore) remove(key string) Status {
	settings := OpenSettings(s.path)
	found := false
	for _, group := range settings.ChildGroups("") {
		if group == key {
			found = true
			break
		}
	}
	if !found {
		return StatusWritten
	}
	if blocked := settings.WriteBlocked(); blocked != StatusWritten {
		return blocked
	}
	settings.RemoveGroup(key)
	return settings.Sync()
}

// STSGroupKey hashes the trimmed, case-folded host as lowercase hex SHA-256.
// An empty host has no key. It mirrors sts_group_key / the anonymous groupKey
// in session/sts.go.
func STSGroupKey(host string) string {
	folded := strings.ToLower(strings.TrimSpace(host))
	if folded == "" {
		return ""
	}
	sum := sha256.Sum256([]byte(folded))
	return hex.EncodeToString(sum[:])
}

func stsSaturatingExpiry(nowUnix, durationSeconds int64) int64 {
	if durationSeconds <= 0 {
		return nowUnix
	}
	if nowUnix > math.MaxInt64-durationSeconds {
		return math.MaxInt64
	}
	return nowUnix + durationSeconds
}
