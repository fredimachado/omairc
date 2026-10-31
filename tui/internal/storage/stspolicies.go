package storage

import (
	"crypto/sha256"
	"encoding/hex"
	"math"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

// The STS policy cache on disk: the `sts` INI file beside the settings file, in
// the format IrcStsStore / desktop IniStsStore write. Each host is one group,
// named by its STSGroupKey, holding port, duration, and expiry (epoch seconds).
// Desktop and this client read and write the same entries.
// A well-formed file is updated in place through Settings. When the file is
// malformed, this store renames it to a non-clobbering sts.corrupt backup
// (then sts.corrupt.1, .2, …) and writes a clean file from the parseable
// entries plus the new policy. A failed rename leaves the file alone. An
// unreadable or unwritable file is still left alone. Sync already restricts
// the file to its owner (0600).

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
// IniStsStore::store. A malformed file is backed up and rewritten from
// parseable entries plus the new policy; an unwritable file is left alone
// (StatusAccessError). The caller keeps any in-memory policy on failure.
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
	mergeSTSPolicy(settings, key, STSEntry{
		Port:               port,
		DurationSeconds:    durationSeconds,
		ExpiryEpochSeconds: expiry,
	}, nowUnix)
	return persistSTSSettings(settings, s.path)
}

// Clear drops the policy for host, if any. It mirrors IniStsStore::clear.
func (s *IniSTSStore) Clear(host string) Status {
	key := STSGroupKey(host)
	if key == "" || s.path == "" {
		return StatusWritten
	}
	return s.remove(key)
}

// MergeEntries writes every entry (keyed by STS group key) into the file,
// removing clearKeys first. Parseable on-disk groups are kept and merged with
// the same strongest-expiry rule as Store. Used by session.STSStore so a
// malformed-file recovery includes every in-memory policy.
func (s *IniSTSStore) MergeEntries(entries map[string]STSEntry, clearKeys []string, nowUnix int64) Status {
	if s == nil || s.path == "" {
		return StatusWritten
	}
	settings := OpenSettings(s.path)
	for _, key := range clearKeys {
		if key != "" {
			settings.RemoveGroup(key)
		}
	}
	for key, entry := range entries {
		if key == "" || entry.Port < 1 {
			continue
		}
		mergeSTSPolicy(settings, key, entry, nowUnix)
	}
	return persistSTSSettings(settings, s.path)
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
	settings.RemoveGroup(key)
	return persistSTSSettings(settings, s.path)
}

// mergeSTSPolicy keeps the stronger unexpired policy when the same host
// already has one. A longer remaining expiry wins; on a tie the incoming
// policy wins. duration=0 clears go through Clear and never call this.
func mergeSTSPolicy(settings *Settings, key string, incoming STSEntry, nowUnix int64) {
	if settings.Contains(key, stsPortKey) && settings.Contains(key, stsExpiryKey) {
		existingExpiry := settings.Int64(key, stsExpiryKey, 0)
		if existingExpiry > nowUnix && existingExpiry > incoming.ExpiryEpochSeconds {
			return
		}
	}
	settings.SetUint(key, stsPortKey, uint64(incoming.Port))
	settings.SetValue(key, stsDurationKey, strconv.FormatInt(incoming.DurationSeconds, 10))
	settings.SetValue(key, stsExpiryKey, strconv.FormatInt(incoming.ExpiryEpochSeconds, 10))
}

// persistSTSSettings writes settings to path. On a malformed file, renames it
// to a backup first; a failed rename leaves the file alone. Access errors
// leave the file alone.
func persistSTSSettings(settings *Settings, path string) Status {
	switch settings.WriteBlocked() {
	case StatusAccessError:
		return StatusAccessError
	case StatusFormatError:
		if !renameSTSCorruptBackup(path) {
			return StatusFormatError
		}
		// Merge may have kept existing values (dirty=false). Still rewrite.
		settings.markDirty()
		return settings.Sync()
	default:
		return settings.Sync()
	}
}

// renameSTSCorruptBackup renames path to path.corrupt, or path.corrupt.N when
// that name is taken. Never overwrites an existing backup.
func renameSTSCorruptBackup(path string) bool {
	backup := nextSTSCorruptPath(path)
	if backup == "" {
		return false
	}
	return os.Rename(path, backup) == nil
}

// nextSTSCorruptPath returns the next unused backup path beside path:
// sts.corrupt, then sts.corrupt.1, …
func nextSTSCorruptPath(path string) string {
	if path == "" {
		return ""
	}
	dir := filepath.Dir(path)
	name := filepath.Base(path)
	primary := filepath.Join(dir, name+".corrupt")
	if _, err := os.Stat(primary); os.IsNotExist(err) {
		return primary
	}
	for n := 1; ; n++ {
		candidate := filepath.Join(dir, name+".corrupt."+strconv.Itoa(n))
		if _, err := os.Stat(candidate); os.IsNotExist(err) {
			return candidate
		}
	}
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
