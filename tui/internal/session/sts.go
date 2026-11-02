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
// or until another client changes that host in the file, which leaves it as saved
// memory. Lookups answer with
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

// stsFileView is what the disk store held for one host when it was read. It
// mirrors FileView in client/src/storage/stspolicies.rs. cached is set only for
// storage.STSViewPolicy.
type stsFileView struct {
	view   storage.STSView
	cached STSCached
}

// policy reports the host's policy the view holds, if any.
func (v stsFileView) policy() (STSCached, bool) {
	return v.cached, v.view == storage.STSViewPolicy
}

// known reports what the view says about the host: nil for a clear, a pointer
// to the policy it holds, and ok false when the view says nothing.
func (v stsFileView) known() (policy *STSCached, ok bool) {
	switch v.view {
	case storage.STSViewClear:
		return nil, true
	case storage.STSViewPolicy:
		cached := v.cached
		return &cached, true
	default:
		return nil, false
	}
}

// stsIdentity is the port and duration of a policy, with the expiry left out. It
// is the zero STSCached for a clear.
func stsIdentity(policy *STSCached) STSCached {
	if policy == nil {
		return STSCached{}
	}
	return STSCached{Port: policy.Port, DurationSeconds: policy.DurationSeconds}
}

// stsFileMoved reports whether the file moved from recorded to now. Only a view
// that says something can move. Another client saving a different live policy
// moves it, and so does clearing the host while the recorded policy was live. A
// lapsed policy is not in force, so it moves nothing. A refresh by another client
// keeps the port and duration, so it never moves. It mirrors file_moved in
// client/src/storage/stspolicies.rs.
func stsFileMoved(recorded, now stsFileView, nowUnix int64) bool {
	before, ok := recorded.known()
	if !ok {
		return false
	}
	after, ok := now.known()
	if !ok {
		return false
	}
	if stsIdentity(before) == stsIdentity(after) {
		return false
	}
	if after != nil {
		return after.ExpiryEpochSeconds > nowUnix
	}
	return before != nil && before.ExpiryEpochSeconds > nowUnix
}

// stsHeld is one policy in memory. unsaved is set when a write of the policy
// failed, so the file does not hold it. recorded is the view the policy is compared
// with. It is the zero value (Unreadable) until a readable view is recorded, and
// Unknown after a view that says nothing about the host.
type stsHeld struct {
	host     string
	cached   STSCached
	unsaved  bool
	recorded stsFileView
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
// expired entry. Every call first settles the unsaved policies against the file.
// An unsaved policy then answers while it stands, so the file never replaces it.
// Then the file answers when it holds a live policy for host, and that policy is
// hydrated into memory, so a stale memory entry never beats a newer one another
// client saved. Memory answers last, when the file has none. A malformed file is
// recovered through flushRecoveryLocked so other in-memory hosts are not dropped.
// It mirrors StsStore::lookup.
func (s *STSStore) Lookup(host string, now time.Time) (STSCached, bool) {
	key := stsGroupKey(host)
	if key == "" {
		return STSCached{}, false
	}
	nowUnix := now.Unix()
	s.mu.Lock()
	defer s.mu.Unlock()
	s.settleUnsavedLocked(nowUnix)
	if held, ok := s.entries[key]; ok && held.unsaved && held.cached.ExpiryEpochSeconds > nowUnix {
		return held.cached, true
	}
	if cached, ok := s.lookupDiskLocked(host, nowUnix); ok {
		s.entries[key] = stsHeld{host: host, cached: cached}
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

// lookupDiskLocked reads host's live policy from the disk store, or reports false
// when the file has none. A stale group is dropped from a well-formed file. Caller
// must hold s.mu.
func (s *STSStore) lookupDiskLocked(host string, nowUnix int64) (STSCached, bool) {
	if s.disk == nil {
		return STSCached{}, false
	}
	if storage.ProbeSettingsIni(s.disk.Path()) == storage.IniProbeMalformed {
		return s.lookupMalformedDiskLocked(host, nowUnix)
	}
	entry, present, stale := s.disk.Inspect(host, nowUnix)
	if !present {
		return STSCached{}, false
	}
	if stale {
		// Expired or invalid on disk: drop it from the file.
		_ = s.persistLocked(host, 0, 0, nowUnix, true)
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
// an empty host or a zero port is ignored. Memory is updated first, and the
// policy stays unsaved until a write lands. A disk failure is returned without
// rolling memory back. The mutex covers the disk write so concurrent Stores from
// shared-manager sessions cannot lose hosts. A well-formed file gets this host's
// mutation and every kept unsaved policy. Malformed-file recovery flushes every
// unexpired in-memory entry so a host that only lives in memory is not dropped,
// without resurrecting a host another client cleared on disk.
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
	s.settleUnsavedLocked(now.Unix())
	// The file's view before this write is what the file still holds if the write fails.
	recorded := s.fileViewLocked(host)
	s.entries[key] = stsHeld{host: host, cached: cached, unsaved: true, recorded: recorded}
	return s.persistLocked(host, port, durationSeconds, now.Unix(), false)
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
	s.settleUnsavedLocked(nowUnix)
	file := s.fileViewLocked(host)
	held, unsaved := s.entries[key]
	unsaved = unsaved && held.unsaved
	// An unsaved policy is the one lookup serves, so a refresh extends it. Otherwise the file's policy is extended.
	cached := held.cached
	if !unsaved {
		policy, ok := file.policy()
		if !ok {
			return nil
		}
		cached = policy
	}
	if cached.DurationSeconds != durationSeconds {
		return nil
	}
	cached.ExpiryEpochSeconds = stsExpiry(nowUnix, durationSeconds)
	// An unsaved policy keeps the baseline that settleUnsavedLocked left. Otherwise
	// the file's view is the baseline, since this refresh extends that policy.
	recorded := file
	if unsaved {
		recorded = held.recorded
	}
	s.entries[key] = stsHeld{host: host, cached: cached, unsaved: true, recorded: recorded}
	return s.persistLocked(host, cached.Port, durationSeconds, nowUnix, false)
}

// fileViewLocked reads host's group from the disk store. Caller must hold s.mu.
func (s *STSStore) fileViewLocked(host string) stsFileView {
	if s.disk == nil {
		return stsFileView{}
	}
	entry, view := s.disk.Group(host)
	return stsFileView{
		view: view,
		cached: STSCached{
			Port:               entry.Port,
			DurationSeconds:    entry.DurationSeconds,
			ExpiryEpochSeconds: entry.ExpiryEpochSeconds,
		},
	}
}

// settleUnsavedLocked settles every unsaved policy against the file's view of its
// host, before a call changes anything. An unreadable view keeps the baseline. An
// unknown view ends it. A readable view that moved from a live baseline marks the
// policy saved, so the file answers and nothing writes it back. Otherwise the view becomes the baseline. Caller must
// hold s.mu.
func (s *STSStore) settleUnsavedLocked(nowUnix int64) {
	if s.disk == nil {
		return
	}
	for key, held := range s.entries {
		if !held.unsaved {
			continue
		}
		file := s.fileViewLocked(held.host)
		if stsFileMoved(held.recorded, file, nowUnix) {
			// The file moved on, so it must never get this policy back. It stays in memory
			// as a saved policy: the file answers first, and memory answers until expiry.
			held.unsaved = false
			s.entries[key] = held
			continue
		}
		if file.view == storage.STSViewUnreadable {
			continue
		}
		held.recorded = file
		s.entries[key] = held
	}
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
	s.settleUnsavedLocked(nowUnix)
	delete(s.entries, key)
	return s.persistLocked(host, 0, 0, nowUnix, true)
}

// persistLocked writes one mutation to a well-formed file, or recovers a
// malformed file by flushing every unexpired in-memory entry. Every unsaved entry
// the write carries is saved once it lands. Caller must hold s.mu.
func (s *STSStore) persistLocked(host string, port uint16, durationSeconds, nowUnix int64, clearing bool) error {
	if s.disk == nil {
		s.settleWrittenLocked(nowUnix)
		return nil
	}
	path := s.disk.Path()
	if storage.ProbeSettingsIni(path) == storage.IniProbeMalformed {
		return s.flushRecoveryLocked(nowUnix, clearing, host)
	}
	unsaved := s.unsavedLocked(nowUnix)
	var status storage.Status
	if clearing {
		status = s.disk.Clear(host, unsaved)
	} else {
		status = s.disk.Store(host, port, durationSeconds, nowUnix, unsaved)
	}
	err := stsPersistStatus(status)
	if status == storage.StatusWritten {
		s.settleWrittenLocked(nowUnix)
	}
	return err
}

// flushRecoveryLocked merges every unexpired in-memory entry into a recovered
// clean file. Expired entries are skipped so they cannot return to disk. Once the
// write lands, every unsaved entry it carried is in the file. Caller must hold s.mu.
func (s *STSStore) flushRecoveryLocked(nowUnix int64, clearing bool, clearHost string) error {
	unsaved := s.unsavedLocked(nowUnix)
	known := make(map[string]storage.STSEntry, len(s.entries))
	for key, held := range s.entries {
		if held.unsaved || held.cached.Port < 1 || held.cached.ExpiryEpochSeconds <= nowUnix {
			continue
		}
		known[key] = storage.STSEntry{
			Port:               held.cached.Port,
			DurationSeconds:    held.cached.DurationSeconds,
			ExpiryEpochSeconds: held.cached.ExpiryEpochSeconds,
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
		s.settleWrittenLocked(nowUnix)
	}
	return err
}

// unsavedLocked returns the unexpired unsaved policies, as the disk store writes
// them. Caller must hold s.mu.
func (s *STSStore) unsavedLocked(nowUnix int64) map[string]storage.STSEntry {
	unsaved := make(map[string]storage.STSEntry)
	for key, held := range s.entries {
		if !held.unsaved || held.cached.Port < 1 || held.cached.ExpiryEpochSeconds <= nowUnix {
			continue
		}
		unsaved[key] = storage.STSEntry{
			Port:               held.cached.Port,
			DurationSeconds:    held.cached.DurationSeconds,
			ExpiryEpochSeconds: held.cached.ExpiryEpochSeconds,
		}
	}
	return unsaved
}

// settleWrittenLocked marks every unexpired policy as saved once a write has
// landed. Each one is in the file now: an unsaved one was merged, and a saved one
// was there already or came from recovery. Caller must hold s.mu.
func (s *STSStore) settleWrittenLocked(nowUnix int64) {
	for key, held := range s.entries {
		if held.unsaved && held.cached.ExpiryEpochSeconds > nowUnix {
			held.unsaved = false
			s.entries[key] = held
		}
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
