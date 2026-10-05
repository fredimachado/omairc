package storage

import (
	"github.com/fredimachado/omairc/tui/internal/irc"
)

// Group and key names must match src/irc/ircclosedconversation.cpp byte for
// byte so the Qt client and the terminal client share one INI section.
const (
	closedConversationGroup      = "closedConversations"
	closedConversationTargetsKey = "targets"
)

// ClosedConversationStore persists the per-network list of conversations the
// user closed, mirroring IrcClosedConversationStore. Catch-up must not create
// them again. Group "closedConversations/<networkId>", key "targets".
type ClosedConversationStore struct {
	ephemeral bool
	cache     map[string][]string
}

// NewClosedConversationStore returns an empty, disk-backed store.
func NewClosedConversationStore() *ClosedConversationStore {
	return &ClosedConversationStore{cache: make(map[string][]string)}
}

// SetEphemeral switches the store between disk and an in-memory-only cache.
// Turning it on clears the cache, mirroring IrcClosedConversationStore::setEphemeral.
func (s *ClosedConversationStore) SetEphemeral(ephemeral bool) {
	s.ephemeral = ephemeral
	if ephemeral {
		s.cache = make(map[string][]string)
	}
}

func (s *ClosedConversationStore) load(networkID string) []string {
	if networkID == "" {
		return nil
	}
	if s.ephemeral {
		return s.cache[networkID]
	}
	settings := OpenSettings("")
	return settings.StringList(closedConversationGroup+"/"+networkID, closedConversationTargetsKey)
}

func (s *ClosedConversationStore) save(networkID string, targets []string) {
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
	group := closedConversationGroup + "/" + networkID
	if len(targets) == 0 {
		settings.RemoveGroup(group)
	} else {
		settings.SetStringList(group, closedConversationTargetsKey, targets)
	}
	settings.Sync()
}

func (s *ClosedConversationStore) cached(networkID string) []string {
	if found, ok := s.cache[networkID]; ok {
		return append([]string(nil), found...)
	}
	loaded := s.load(networkID)
	s.cache[networkID] = loaded
	return append([]string(nil), loaded...)
}

// Contains reports whether target is stored for networkID under mapping.
func (s *ClosedConversationStore) Contains(networkID, target string, mapping irc.CaseMapping) bool {
	if networkID == "" || target == "" {
		return false
	}
	return openDirectListContains(s.cached(networkID), target, mapping)
}

// Add appends target when it is newly case-mapped distinct and saves.
func (s *ClosedConversationStore) Add(networkID, target string, mapping irc.CaseMapping) bool {
	if networkID == "" || target == "" {
		return false
	}
	current := s.cached(networkID)
	if openDirectListContains(current, target, mapping) {
		return false
	}
	current = append(current, target)
	s.cache[networkID] = current
	s.save(networkID, current)
	return true
}

// Remove drops every case-mapped match of target and saves.
func (s *ClosedConversationStore) Remove(networkID, target string, mapping irc.CaseMapping) bool {
	if networkID == "" || target == "" {
		return false
	}
	current := s.cached(networkID)
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
	s.save(networkID, current)
	return true
}

// Rekey renames oldTarget to newTarget.
func (s *ClosedConversationStore) Rekey(networkID, oldTarget, newTarget string, mapping irc.CaseMapping) bool {
	if networkID == "" || oldTarget == "" || newTarget == "" {
		return false
	}
	current := s.cached(networkID)
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
	s.save(networkID, current)
	return true
}

// Forget drops the cache entry and removes the persisted group.
func (s *ClosedConversationStore) Forget(networkID string) {
	if networkID == "" {
		return
	}
	delete(s.cache, networkID)
	s.save(networkID, nil)
}
