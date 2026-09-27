package storage

import (
	"github.com/fredimachado/omairc/tui/internal/irc"
)

// Group and key names must match src/irc/ircopendirect.cpp byte for byte so the
// Qt client and the terminal client share one INI section.
const (
	openDirectGroup      = "openDirects"
	openDirectTargetsKey = "targets"
)

// OpenDirectStore persists the per-network list of direct messages to reopen
// after ISUPPORT, mirroring IrcOpenDirectStore. Group "openDirects/<networkId>",
// key "targets" (a QSettings string list).
//
// A store is a thin cache over disk: the first read of a network loads its list
// and later calls reuse it, exactly like IrcOpenDirectStore's mutable QHash.
// When ephemeral, nothing touches disk and SetEphemeral(true) drops the cache.
type OpenDirectStore struct {
	ephemeral bool
	cache     map[string][]string
}

// NewOpenDirectStore returns an empty, disk-backed store.
func NewOpenDirectStore() *OpenDirectStore {
	return &OpenDirectStore{cache: make(map[string][]string)}
}

// SetEphemeral switches the store between disk and an in-memory-only cache.
// Turning it on clears the cache; turning it off keeps whatever the cache
// holds, mirroring IrcOpenDirectStore::setEphemeral.
func (s *OpenDirectStore) SetEphemeral(ephemeral bool) {
	s.ephemeral = ephemeral
	if ephemeral {
		s.cache = make(map[string][]string)
	}
}

// openDirectLoad reads the persisted list for networkID. An empty networkID or
// an absent key yields nil.
func (s *OpenDirectStore) openDirectLoad(networkID string) []string {
	if networkID == "" {
		return nil
	}
	if s.ephemeral {
		return s.cache[networkID]
	}
	settings := OpenSettings("")
	return settings.StringList(openDirectGroup+"/"+networkID, openDirectTargetsKey)
}

// openDirectSave persists targets for networkID. An empty list removes the
// group instead of writing an empty one. It is a no-op for an empty networkID.
func (s *OpenDirectStore) openDirectSave(networkID string, targets []string) {
	if networkID == "" {
		return
	}
	if s.ephemeral {
		if len(targets) == 0 {
			delete(s.cache, networkID)
		} else {
			s.cache[networkID] = targets
		}
		return
	}
	settings := OpenSettings("")
	group := openDirectGroup + "/" + networkID
	if len(targets) == 0 {
		settings.RemoveGroup(group)
	} else {
		settings.SetStringList(group, openDirectTargetsKey, targets)
	}
	settings.Sync()
}

// openDirectCached returns a copy of the cached list for networkID, loading it
// from disk on first use.
func (s *OpenDirectStore) openDirectCached(networkID string) []string {
	if found, ok := s.cache[networkID]; ok {
		return append([]string(nil), found...)
	}
	loaded := s.openDirectLoad(networkID)
	s.cache[networkID] = loaded
	return append([]string(nil), loaded...)
}

// Targets returns the stored list for networkID in insertion order. An empty
// networkID returns nil; a network with no persisted direct messages returns an
// empty list.
func (s *OpenDirectStore) Targets(networkID string) []string {
	if networkID == "" {
		return nil
	}
	return s.openDirectCached(networkID)
}

// Listed returns Targets with case-mapped duplicates collapsed, preserving the
// first spelling and the original order.
func (s *OpenDirectStore) Listed(networkID string, mapping irc.CaseMapping) []string {
	var unique []string
	for _, target := range s.Targets(networkID) {
		if !openDirectListContains(unique, target, mapping) {
			unique = append(unique, target)
		}
	}
	return unique
}

// Add appends target when it is newly case-mapped distinct and saves. It
// returns false for an empty networkID/target or an already-listed target.
func (s *OpenDirectStore) Add(networkID, target string, mapping irc.CaseMapping) bool {
	if networkID == "" || target == "" {
		return false
	}
	current := s.openDirectCached(networkID)
	if openDirectListContains(current, target, mapping) {
		return false
	}
	current = append(current, target)
	s.cache[networkID] = current
	s.openDirectSave(networkID, current)
	return true
}

// Remove drops every case-mapped match of target (scanning from the end) and
// saves. It returns whether anything changed.
func (s *OpenDirectStore) Remove(networkID, target string, mapping irc.CaseMapping) bool {
	if networkID == "" || target == "" {
		return false
	}
	current := s.openDirectCached(networkID)
	changed := false
	for index := len(current) - 1; index >= 0; index-- {
		if openDirectTargetEquals(mapping, current[index], target) {
			current = append(current[:index], current[index+1:]...)
			changed = true
		}
	}
	if !changed {
		return false
	}
	s.cache[networkID] = current
	s.openDirectSave(networkID, current)
	return true
}

// Rekey renames oldTarget to newTarget. A case-mapped-but-byte-different
// spelling is rewritten in place; otherwise the old entry is dropped and
// newTarget appended unless it is already present. It returns false for empty
// arguments or a missing oldTarget.
func (s *OpenDirectStore) Rekey(networkID, oldTarget, newTarget string, mapping irc.CaseMapping) bool {
	if networkID == "" || oldTarget == "" || newTarget == "" {
		return false
	}
	current := s.openDirectCached(networkID)
	oldIndex := openDirectIndexOfTarget(current, oldTarget, mapping)
	if oldIndex < 0 {
		return false
	}
	if openDirectTargetEquals(mapping, current[oldIndex], newTarget) {
		if current[oldIndex] == newTarget {
			return false
		}
		current[oldIndex] = newTarget
	} else {
		current = append(current[:oldIndex], current[oldIndex+1:]...)
		if !openDirectListContains(current, newTarget, mapping) {
			current = append(current, newTarget)
		}
	}
	s.cache[networkID] = current
	s.openDirectSave(networkID, current)
	return true
}

// Forget drops the cache entry and removes the persisted group.
func (s *OpenDirectStore) Forget(networkID string) {
	if networkID == "" {
		return
	}
	delete(s.cache, networkID)
	s.openDirectSave(networkID, nil)
}

// openDirectTargetEquals compares two targets under mapping.
func openDirectTargetEquals(mapping irc.CaseMapping, left, right string) bool {
	return mapping.Equals(left, right)
}

// openDirectIndexOfTarget returns the first case-mapped match, or -1.
func openDirectIndexOfTarget(targets []string, target string, mapping irc.CaseMapping) int {
	for index, candidate := range targets {
		if openDirectTargetEquals(mapping, candidate, target) {
			return index
		}
	}
	return -1
}

// openDirectListContains reports whether targets already holds a case-mapped
// match of target.
func openDirectListContains(targets []string, target string, mapping irc.CaseMapping) bool {
	return openDirectIndexOfTarget(targets, target, mapping) >= 0
}
