package storage

import (
	"github.com/fredimachado/omairc/tui/internal/irc"
)

// Group and key names must match src/irc/ircopendirect.cpp byte for byte so the
// Qt client and the terminal client share one INI section.
const (
	openDirectGroup      = "openDirects"
	dismissedDirectGroup = "dismissedDirects"
	openDirectTargetsKey = "targets"
)

// OpenDirectStore persists the per-network list of direct messages to reopen
// after ISUPPORT, and the direct messages the user closed, mirroring
// IrcOpenDirectStore. Open targets live in "openDirects/<networkId>"; dismissed
// targets live in "dismissedDirects/<networkId>". Both use the key "targets".
//
// A store is a thin cache over disk: the first read of a network loads its list
// and later calls reuse it, exactly like IrcOpenDirectStore's mutable QHash.
// When ephemeral, nothing touches disk and SetEphemeral(true) drops the cache.
type OpenDirectStore struct {
	ephemeral bool
	cache     map[string][]string
	dismissed map[string][]string
}

// NewOpenDirectStore returns an empty, disk-backed store.
func NewOpenDirectStore() *OpenDirectStore {
	return &OpenDirectStore{
		cache:     make(map[string][]string),
		dismissed: make(map[string][]string),
	}
}

// SetEphemeral switches the store between disk and an in-memory-only cache.
// Turning it on clears the cache; turning it off keeps whatever the cache
// holds, mirroring IrcOpenDirectStore::setEphemeral.
func (s *OpenDirectStore) SetEphemeral(ephemeral bool) {
	s.ephemeral = ephemeral
	if ephemeral {
		s.cache = make(map[string][]string)
		s.dismissed = make(map[string][]string)
	}
}

func (s *OpenDirectStore) load(group, networkID string, cache map[string][]string) []string {
	if networkID == "" {
		return nil
	}
	if s.ephemeral {
		return cache[networkID]
	}
	settings := OpenSettings("")
	return settings.StringList(group+"/"+networkID, openDirectTargetsKey)
}

func (s *OpenDirectStore) save(group, networkID string, targets []string, cache map[string][]string) {
	if networkID == "" {
		return
	}
	if s.ephemeral {
		if len(targets) == 0 {
			delete(cache, networkID)
		} else {
			cache[networkID] = targets
		}
		return
	}
	settings := OpenSettings("")
	section := group + "/" + networkID
	if len(targets) == 0 {
		settings.RemoveGroup(section)
	} else {
		settings.SetStringList(section, openDirectTargetsKey, targets)
	}
	settings.Sync()
}

func (s *OpenDirectStore) cached(group, networkID string, cache map[string][]string) []string {
	if found, ok := cache[networkID]; ok {
		return append([]string(nil), found...)
	}
	loaded := s.load(group, networkID, cache)
	cache[networkID] = loaded
	return append([]string(nil), loaded...)
}

// Targets returns the stored list for networkID in insertion order. An empty
// networkID returns nil; a network with no persisted direct messages returns an
// empty list.
func (s *OpenDirectStore) Targets(networkID string) []string {
	if networkID == "" {
		return nil
	}
	return s.cached(openDirectGroup, networkID, s.cache)
}

// Listed returns Targets with case-mapped duplicates collapsed, preserving the
// first spelling and the original order.
func (s *OpenDirectStore) Listed(networkID string, mapping irc.CaseMapping) []string {
	return uniqueTargets(s.Targets(networkID), mapping)
}

// Add appends target when it is newly case-mapped distinct and saves. It
// returns false for an empty networkID/target or an already-listed target.
func (s *OpenDirectStore) Add(networkID, target string, mapping irc.CaseMapping) bool {
	return s.add(openDirectGroup, s.cache, networkID, target, mapping)
}

// Remove drops every case-mapped match of target (scanning from the end) and
// saves. It returns whether anything changed.
func (s *OpenDirectStore) Remove(networkID, target string, mapping irc.CaseMapping) bool {
	return s.remove(openDirectGroup, s.cache, networkID, target, mapping)
}

// Rekey renames oldTarget to newTarget. A case-mapped-but-byte-different
// spelling is rewritten in place; otherwise the old entry is dropped and
// newTarget appended unless it is already present. It returns false for empty
// arguments or a missing oldTarget.
func (s *OpenDirectStore) Rekey(networkID, oldTarget, newTarget string, mapping irc.CaseMapping) bool {
	return s.rekey(openDirectGroup, s.cache, networkID, oldTarget, newTarget, mapping)
}

// IsDismissed reports whether the user closed target. A stamp or a transcript
// does not dismiss it.
func (s *OpenDirectStore) IsDismissed(networkID, target string, mapping irc.CaseMapping) bool {
	if networkID == "" || target == "" {
		return false
	}
	return openDirectListContains(s.cached(dismissedDirectGroup, networkID, s.dismissed), target, mapping)
}

// DismissedListed returns the dismissed targets with case-mapped duplicates
// collapsed.
func (s *OpenDirectStore) DismissedListed(networkID string, mapping irc.CaseMapping) []string {
	if networkID == "" {
		return nil
	}
	return uniqueTargets(s.cached(dismissedDirectGroup, networkID, s.dismissed), mapping)
}

// Dismiss records a user close. It mirrors IrcOpenDirectStore::dismiss.
func (s *OpenDirectStore) Dismiss(networkID, target string, mapping irc.CaseMapping) bool {
	return s.add(dismissedDirectGroup, s.dismissed, networkID, target, mapping)
}

// Undismiss drops a user close, so opening the direct again lets catch-up
// fill it. It mirrors IrcOpenDirectStore::undismiss.
func (s *OpenDirectStore) Undismiss(networkID, target string, mapping irc.CaseMapping) bool {
	return s.remove(dismissedDirectGroup, s.dismissed, networkID, target, mapping)
}

// RekeyDismissed renames a dismissed target. A nick change is a rename of an
// existing dismiss, not a new close.
func (s *OpenDirectStore) RekeyDismissed(networkID, oldTarget, newTarget string, mapping irc.CaseMapping) bool {
	return s.rekey(dismissedDirectGroup, s.dismissed, networkID, oldTarget, newTarget, mapping)
}

// Forget drops both lists for networkID.
func (s *OpenDirectStore) Forget(networkID string) {
	if networkID == "" {
		return
	}
	delete(s.cache, networkID)
	delete(s.dismissed, networkID)
	s.save(openDirectGroup, networkID, nil, s.cache)
	s.save(dismissedDirectGroup, networkID, nil, s.dismissed)
}

func (s *OpenDirectStore) add(group string, cache map[string][]string, networkID, target string, mapping irc.CaseMapping) bool {
	if networkID == "" || target == "" {
		return false
	}
	current := s.cached(group, networkID, cache)
	if openDirectListContains(current, target, mapping) {
		return false
	}
	current = append(current, target)
	cache[networkID] = current
	s.save(group, networkID, current, cache)
	return true
}

func (s *OpenDirectStore) remove(group string, cache map[string][]string, networkID, target string, mapping irc.CaseMapping) bool {
	if networkID == "" || target == "" {
		return false
	}
	current := s.cached(group, networkID, cache)
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
	cache[networkID] = current
	s.save(group, networkID, current, cache)
	return true
}

func (s *OpenDirectStore) rekey(group string, cache map[string][]string, networkID, oldTarget, newTarget string, mapping irc.CaseMapping) bool {
	if networkID == "" || oldTarget == "" || newTarget == "" {
		return false
	}
	current := s.cached(group, networkID, cache)
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
	cache[networkID] = current
	s.save(group, networkID, current, cache)
	return true
}

func uniqueTargets(targets []string, mapping irc.CaseMapping) []string {
	var unique []string
	for _, target := range targets {
		if !openDirectListContains(unique, target, mapping) {
			unique = append(unique, target)
		}
	}
	return unique
}

func openDirectTargetEquals(mapping irc.CaseMapping, left, right string) bool {
	return mapping.Equals(left, right)
}

func openDirectIndexOfTarget(targets []string, target string, mapping irc.CaseMapping) int {
	for index, candidate := range targets {
		if openDirectTargetEquals(mapping, candidate, target) {
			return index
		}
	}
	return -1
}

func openDirectListContains(targets []string, target string, mapping irc.CaseMapping) bool {
	return openDirectIndexOfTarget(targets, target, mapping) >= 0
}
