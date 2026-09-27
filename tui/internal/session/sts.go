package session

import (
	"crypto/sha256"
	"encoding/hex"
	"strconv"
	"strings"
	"sync"
)

// STS (Strict Transport Security) support, ported from src/irc/ircsts.h and
// src/irc/ircsts.cpp.
//
// IMPORTANT: this is an in-memory-only port. The C++ IrcStsStore persists the
// cache as an INI file under the Omairc config root (rootDir/filePath/
// restrictPermissions, ircsts.cpp:79-100). File persistence is deferred to a
// later phase; this store keeps the same lookup/save/clear semantics — group
// key, port range validation, and epoch expiry on lookup — but never touches
// the filesystem. No os or path import belongs here.

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
// parseIrcStsAdvertisement (ircsts.cpp:66-75).
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
// mirrors parseStsValue (ircsts.cpp:29-55).
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

// stsTokenName returns the capability name of a token, dropping any "=value".
func stsTokenName(token string) string {
	if separator := strings.IndexByte(token, '='); separator != -1 {
		return token[:separator]
	}
	return token
}

// stsTokenValue returns everything after the first '=', or "".
func stsTokenValue(token string) string {
	if separator := strings.IndexByte(token, '='); separator != -1 {
		return token[separator+1:]
	}
	return ""
}

// STSStore is the in-memory STS policy cache. Unlike the C++ IrcStsStore it
// never persists; the lookup/save/clear contract is otherwise identical,
// including the sha256 group key over the case-folded host and the epoch
// expiry check.
//
// Lookup expires through the injected Clock, so a FakeClock can drive expiry
// deterministically. A nil Clock falls back to RealClock.
type STSStore struct {
	mu      sync.Mutex
	clock   Clock
	entries map[string]STSCached
}

// NewSTSStore returns an empty in-memory store. clock may be nil, in which
// case the store reads wall-clock time through RealClock.
func NewSTSStore(clock Clock) *STSStore {
	if clock == nil {
		clock = RealClock{}
	}
	return &STSStore{clock: clock, entries: make(map[string]STSCached)}
}

// Lookup returns the cached policy for host, dropping an invalid or expired
// entry. It mirrors IrcStsStore::lookup (ircsts.cpp:96-126).
func (s *STSStore) Lookup(host string) (STSCached, bool) {
	key := stsGroupKey(host)
	if key == "" {
		return STSCached{}, false
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	entry, ok := s.entries[key]
	if !ok {
		return STSCached{}, false
	}
	if entry.Port < 1 || entry.Port > 65535 {
		delete(s.entries, key)
		return STSCached{}, false
	}
	if entry.ExpiryEpochSeconds <= s.clock.Now().Unix() {
		delete(s.entries, key)
		return STSCached{}, false
	}
	return entry, true
}

// Store caches a policy for host. A zero duration clears the entry, and an
// empty host or a zero port is ignored, mirroring IrcStsStore::save
// (ircsts.cpp:128-148).
func (s *STSStore) Store(host string, port uint16, durationSeconds int64) {
	if durationSeconds == 0 {
		s.Clear(host)
		return
	}
	key := stsGroupKey(host)
	if key == "" || port < 1 {
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.entries[key] = STSCached{
		Port:               port,
		DurationSeconds:    durationSeconds,
		ExpiryEpochSeconds: s.clock.Now().Unix() + durationSeconds,
	}
}

// Clear drops the cached policy for host. It mirrors IrcStsStore::clear
// (ircsts.cpp:150-160).
func (s *STSStore) Clear(host string) {
	key := stsGroupKey(host)
	if key == "" {
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.entries, key)
}

// stsGroupKey hashes the trimmed, case-folded host. An empty host has no key.
// It mirrors the anonymous groupKey (ircsts.cpp:57-64).
func stsGroupKey(host string) string {
	folded := strings.ToLower(strings.TrimSpace(host))
	if folded == "" {
		return ""
	}
	sum := sha256.Sum256([]byte(folded))
	return hex.EncodeToString(sum[:])
}
