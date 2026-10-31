package storage

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"io"
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
// malformed, this store writes a clean temp file, hard-links sts to a
// non-clobbering sts.corrupt backup (then sts.corrupt.1, .2, …), and renames
// the temp over sts. A failure at any step leaves sts untouched. An unreadable
// or unwritable file is still left alone. Sync already restricts the file to
// its owner (0600).

const (
	stsPortKey     = "port"
	stsDurationKey = "duration"
	stsExpiryKey   = "expiry"

	// Cap on sts.corrupt / sts.corrupt.N attempts so a persistent error cannot
	// spin forever under the store mutex.
	maxSTSCorruptBackups = 10000
)

// Test hooks for recovery failure paths. Tests only; zero value is normal.
type stsTestHook int

const (
	stsHookNone stsTestHook = iota
	stsHookFailInstall
	stsHookFailBackup
	stsHookFailCopyAfterCreate
	stsHookMaxBackups
)

var (
	testSTSHook              stsTestHook
	testSTSMaxCorruptBackups int
)

// TestingSetSTSHook configures recovery failure hooks for tests. name is one of
// "fail-install", "fail-backup", "fail-copy", or "max-backups". restore clears it.
func TestingSetSTSHook(name string, maxBackups int) (restore func()) {
	prev, prevMax := testSTSHook, testSTSMaxCorruptBackups
	switch name {
	case "fail-install":
		testSTSHook = stsHookFailInstall
	case "fail-backup":
		testSTSHook = stsHookFailBackup
	case "fail-copy":
		testSTSHook = stsHookFailCopyAfterCreate
	case "max-backups":
		testSTSHook = stsHookMaxBackups
		testSTSMaxCorruptBackups = maxBackups
	default:
		testSTSHook = stsHookNone
	}
	return func() {
		testSTSHook = prev
		testSTSMaxCorruptBackups = prevMax
	}
}

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
// is none or it has expired. An expired or invalid entry is removed from a
// well-formed file. On a malformed file it does not rewrite (recovery needs the
// caller's in-memory map; see session.STSStore). It mirrors IniStsStore::lookup.
func (s *IniSTSStore) Lookup(host string, nowUnix int64) (STSEntry, bool) {
	entry, present, stale := s.Inspect(host, nowUnix)
	if !present {
		return STSEntry{}, false
	}
	if stale {
		if ProbeSettingsIni(s.path) == IniProbeMalformed {
			return STSEntry{}, false
		}
		s.remove(STSGroupKey(host))
		return STSEntry{}, false
	}
	return entry, true
}

// Inspect reports the on-disk STS entry for host without writing. present is
// false when port or expiry is missing. stale is true when present but the
// port is invalid or the entry has expired.
func (s *IniSTSStore) Inspect(host string, nowUnix int64) (entry STSEntry, present, stale bool) {
	key := STSGroupKey(host)
	if key == "" || s.path == "" {
		return STSEntry{}, false, false
	}
	settings := OpenSTSSettings(s.path)
	if !settings.Contains(key, stsPortKey) || !settings.Contains(key, stsExpiryKey) {
		return STSEntry{}, false, false
	}
	port := settings.Uint(key, stsPortKey, 0)
	durationSeconds := settings.Int64(key, stsDurationKey, 0)
	expiryEpochSeconds := settings.Int64(key, stsExpiryKey, 0)
	entry = STSEntry{
		Port:               uint16(port), // may truncate; stale check uses the full uint
		DurationSeconds:    durationSeconds,
		ExpiryEpochSeconds: expiryEpochSeconds,
	}
	if port < 1 || port > 65535 || expiryEpochSeconds <= nowUnix {
		return entry, true, true
	}
	return entry, true, false
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
	settings := OpenSTSSettings(s.path)
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
	settings := OpenSTSSettings(s.path)
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
	settings := OpenSTSSettings(s.path)
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
// policy wins. An existing entry with an invalid port never wins. duration=0
// clears go through Clear and never call this.
func mergeSTSPolicy(settings *Settings, key string, incoming STSEntry, nowUnix int64) {
	if settings.Contains(key, stsPortKey) && settings.Contains(key, stsExpiryKey) {
		existingPort := settings.Uint(key, stsPortKey, 0)
		existingExpiry := settings.Int64(key, stsExpiryKey, 0)
		portOK := existingPort >= 1 && existingPort <= 65535
		if portOK && existingExpiry > nowUnix && existingExpiry > incoming.ExpiryEpochSeconds {
			return
		}
	}
	settings.SetUint(key, stsPortKey, uint64(incoming.Port))
	settings.SetValue(key, stsDurationKey, strconv.FormatInt(incoming.DurationSeconds, 10))
	settings.SetValue(key, stsExpiryKey, strconv.FormatInt(incoming.ExpiryEpochSeconds, 10))
}

// persistSTSSettings writes settings to path. On a malformed file, writes a
// clean temp file, hard-links a backup, then renames the temp over sts. A
// failure at any step leaves sts untouched. Access errors leave the file alone.
func persistSTSSettings(settings *Settings, path string) Status {
	switch settings.WriteBlocked() {
	case StatusAccessError:
		return StatusAccessError
	case StatusFormatError:
		// Merge may have kept existing values (dirty=false). Still rewrite.
		settings.markDirty()
		if err := recoverMalformedSTS(path, settings.encode()); err != nil {
			return StatusFormatError
		}
		return StatusWritten
	default:
		return settings.Sync()
	}
}

// recoverMalformedSTS writes clean data to a temp sibling, hard-links path to a
// backup, then renames the temp over path. On any failure, leaves path untouched
// and removes the temp.
func recoverMalformedSTS(path string, data []byte) error {
	temp, err := writeSTSTemp(path, data)
	if err != nil {
		return err
	}
	if !linkSTSCorruptBackup(path) {
		_ = os.Remove(temp)
		return errors.New("sts corrupt backup failed")
	}
	if testSTSHook == stsHookFailInstall {
		_ = os.Remove(temp)
		return errors.New("test: install after backup failed")
	}
	if err := os.Rename(temp, path); err != nil {
		_ = os.Remove(temp)
		return err
	}
	return nil
}

// writeSTSTemp creates a same-directory temp file with data, fsyncs it, and
// returns its path.
func writeSTSTemp(path string, data []byte) (string, error) {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return "", err
	}
	tmp, err := os.CreateTemp(dir, ".omairc-sts-*.tmp")
	if err != nil {
		return "", err
	}
	tmpName := tmp.Name()
	cleanup := true
	defer func() {
		_ = tmp.Close()
		if cleanup {
			_ = os.Remove(tmpName)
		}
	}()
	if err := tmp.Chmod(0o600); err != nil {
		return "", err
	}
	if _, err := tmp.Write(data); err != nil {
		return "", err
	}
	if err := tmp.Sync(); err != nil {
		return "", err
	}
	if err := tmp.Close(); err != nil {
		return "", err
	}
	cleanup = false
	return tmpName, nil
}

func stsCorruptBackupLimit() int {
	if testSTSHook == stsHookMaxBackups && testSTSMaxCorruptBackups > 0 {
		return testSTSMaxCorruptBackups
	}
	return maxSTSCorruptBackups
}

// linkSTSCorruptBackup hard-links path to path.corrupt / path.corrupt.N.
// Retries only on ErrExist; falls back to an exclusive-create copy. Fails
// closed on other errors and caps the loop.
func linkSTSCorruptBackup(path string) bool {
	if path == "" {
		return false
	}
	if testSTSHook == stsHookFailBackup {
		return false
	}
	if testSTSHook == stsHookFailCopyAfterCreate {
		candidate := filepath.Join(filepath.Dir(path), filepath.Base(path)+".corrupt")
		err := copySTSExclusive(path, candidate)
		return err == nil
	}
	dir := filepath.Dir(path)
	name := filepath.Base(path)
	for n := 0; n < stsCorruptBackupLimit(); n++ {
		var candidate string
		if n == 0 {
			candidate = filepath.Join(dir, name+".corrupt")
		} else {
			candidate = filepath.Join(dir, name+".corrupt."+strconv.Itoa(n))
		}
		err := os.Link(path, candidate)
		if err == nil {
			return true
		}
		if os.IsExist(err) {
			continue
		}
		// Link can fail for EXDEV and similar; try an exclusive copy.
		copyErr := copySTSExclusive(path, candidate)
		if copyErr == nil {
			return true
		}
		if os.IsExist(copyErr) {
			continue
		}
		return false
	}
	return false
}

// copySTSExclusive copies src to a new dst created with O_EXCL, then fsyncs.
func copySTSExclusive(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()

	out, err := os.OpenFile(dst, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
	if err != nil {
		return err
	}
	cleanup := true
	defer func() {
		_ = out.Close()
		if cleanup {
			_ = os.Remove(dst)
		}
	}()
	if testSTSHook == stsHookFailCopyAfterCreate {
		if _, err := out.Write([]byte("partial")); err != nil {
			return err
		}
		return errors.New("test: copy write failed")
	}
	if _, err := io.Copy(out, in); err != nil {
		return err
	}
	if err := out.Sync(); err != nil {
		return err
	}
	if err := out.Close(); err != nil {
		return err
	}
	cleanup = false
	return nil
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
