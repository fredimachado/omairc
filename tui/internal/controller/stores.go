package controller

import (
	"strings"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// This file holds the in-memory stores the Phase 7 slash catalog reads and
// writes. The Qt stores (IrcIgnoreStore, IrcMuteStore, IrcMonitorStore,
// IrcHighlightStore, IrcAvatarStore, and the QSettings preferences) persist to
// disk; Phase 7 keeps the same case-mapped list semantics in memory and defers
// persistence to a later phase. The seams in seams.go name what is still
// deferred.

// caseListStore is the shared case-mapped string list behind the ignore, mute,
// monitor, and highlight stores. It mirrors the list half of IrcIgnoreStore,
// IrcMuteStore, IrcMonitorStore, and IrcHighlightStore: a per-network ordered
// list where membership and removal compare through the network's case
// mapping, and adding a duplicate is a no-op.
type caseListStore struct {
	lists map[string][]string
}

func (s *caseListStore) entries(networkID string) []string {
	if networkID == "" {
		return nil
	}
	return s.lists[networkID]
}

func (s *caseListStore) listed(networkID string, mapping irc.CaseMapping) []string {
	var unique []string
	for _, value := range s.entries(networkID) {
		if !caseListContains(unique, value, mapping) {
			unique = append(unique, value)
		}
	}
	return unique
}

func (s *caseListStore) contains(networkID, value string, mapping irc.CaseMapping) bool {
	if networkID == "" || value == "" {
		return false
	}
	return caseListContains(s.entries(networkID), value, mapping)
}

func (s *caseListStore) add(networkID, value string, mapping irc.CaseMapping) bool {
	if networkID == "" || value == "" {
		return false
	}
	if s.lists == nil {
		s.lists = make(map[string][]string)
	}
	current := s.lists[networkID]
	if caseListContains(current, value, mapping) {
		return false
	}
	s.lists[networkID] = append(append([]string(nil), current...), value)
	return true
}

func (s *caseListStore) remove(networkID, value string, mapping irc.CaseMapping) bool {
	if networkID == "" || value == "" {
		return false
	}
	current := s.lists[networkID]
	changed := false
	kept := make([]string, 0, len(current))
	for _, candidate := range current {
		if mapping.Equals(candidate, value) {
			changed = true
			continue
		}
		kept = append(kept, candidate)
	}
	if !changed {
		return false
	}
	if len(kept) == 0 {
		delete(s.lists, networkID)
	} else {
		s.lists[networkID] = kept
	}
	return true
}

func (s *caseListStore) forget(networkID string) {
	if networkID == "" {
		return
	}
	delete(s.lists, networkID)
}

func caseListContains(values []string, value string, mapping irc.CaseMapping) bool {
	for _, candidate := range values {
		if mapping.Equals(candidate, value) {
			return true
		}
	}
	return false
}

func copiedStrings(values []string) []string {
	if len(values) == 0 {
		return nil
	}
	out := make([]string, len(values))
	copy(out, values)
	return out
}

// IgnoreStore is the in-memory /ignore list. It mirrors IrcIgnoreStore.
type IgnoreStore struct{ list caseListStore }

// NewIgnoreStore returns an empty ignore store.
func NewIgnoreStore() *IgnoreStore { return &IgnoreStore{} }

// Nicks returns the stored nicks in insertion order.
func (s *IgnoreStore) Nicks(networkID string) []string {
	return copiedStrings(s.list.entries(networkID))
}

// Listed returns the stored nicks deduplicated under the network's case
// mapping.
func (s *IgnoreStore) Listed(networkID string, mapping irc.CaseMapping) []string {
	return s.list.listed(networkID, mapping)
}

// Contains reports whether nick is ignored.
func (s *IgnoreStore) Contains(networkID, nick string, mapping irc.CaseMapping) bool {
	return s.list.contains(networkID, nick, mapping)
}

// Add records nick and reports whether it was newly added.
func (s *IgnoreStore) Add(networkID, nick string, mapping irc.CaseMapping) bool {
	return s.list.add(networkID, nick, mapping)
}

// Remove drops nick and reports whether anything changed.
func (s *IgnoreStore) Remove(networkID, nick string, mapping irc.CaseMapping) bool {
	return s.list.remove(networkID, nick, mapping)
}

// Forget drops the whole network's list.
func (s *IgnoreStore) Forget(networkID string) { s.list.forget(networkID) }

// MuteStore is the in-memory /mute list. It mirrors IrcMuteStore.
type MuteStore struct{ list caseListStore }

// NewMuteStore returns an empty mute store.
func NewMuteStore() *MuteStore { return &MuteStore{} }

// Targets returns the stored targets in insertion order.
func (s *MuteStore) Targets(networkID string) []string {
	return copiedStrings(s.list.entries(networkID))
}

// Listed returns the stored targets deduplicated under the network's case
// mapping.
func (s *MuteStore) Listed(networkID string, mapping irc.CaseMapping) []string {
	return s.list.listed(networkID, mapping)
}

// Contains reports whether target is muted.
func (s *MuteStore) Contains(networkID, target string, mapping irc.CaseMapping) bool {
	return s.list.contains(networkID, target, mapping)
}

// Add records target and reports whether it was newly added.
func (s *MuteStore) Add(networkID, target string, mapping irc.CaseMapping) bool {
	return s.list.add(networkID, target, mapping)
}

// Remove drops target and reports whether anything changed.
func (s *MuteStore) Remove(networkID, target string, mapping irc.CaseMapping) bool {
	return s.list.remove(networkID, target, mapping)
}

// Forget drops the whole network's list.
func (s *MuteStore) Forget(networkID string) { s.list.forget(networkID) }

// MonitorStore is the in-memory /monitor list. It mirrors IrcMonitorStore.
type MonitorStore struct{ list caseListStore }

// NewMonitorStore returns an empty monitor store.
func NewMonitorStore() *MonitorStore { return &MonitorStore{} }

// Nicks returns the stored nicks in insertion order.
func (s *MonitorStore) Nicks(networkID string) []string {
	return copiedStrings(s.list.entries(networkID))
}

// Listed returns the stored nicks deduplicated under the network's case
// mapping.
func (s *MonitorStore) Listed(networkID string, mapping irc.CaseMapping) []string {
	return s.list.listed(networkID, mapping)
}

// Contains reports whether nick is watched.
func (s *MonitorStore) Contains(networkID, nick string, mapping irc.CaseMapping) bool {
	return s.list.contains(networkID, nick, mapping)
}

// Add records nick and reports whether it was newly added.
func (s *MonitorStore) Add(networkID, nick string, mapping irc.CaseMapping) bool {
	return s.list.add(networkID, nick, mapping)
}

// Remove drops nick and reports whether anything changed.
func (s *MonitorStore) Remove(networkID, nick string, mapping irc.CaseMapping) bool {
	return s.list.remove(networkID, nick, mapping)
}

// Forget drops the whole network's list.
func (s *MonitorStore) Forget(networkID string) { s.list.forget(networkID) }

// HighlightStore is the in-memory /highlight word list. It mirrors
// IrcHighlightStore.
type HighlightStore struct{ list caseListStore }

// NewHighlightStore returns an empty highlight store.
func NewHighlightStore() *HighlightStore { return &HighlightStore{} }

// Words returns the stored words in insertion order.
func (s *HighlightStore) Words(networkID string) []string {
	return copiedStrings(s.list.entries(networkID))
}

// Listed returns the stored words deduplicated under the network's case
// mapping.
func (s *HighlightStore) Listed(networkID string, mapping irc.CaseMapping) []string {
	return s.list.listed(networkID, mapping)
}

// Contains reports whether word is a highlight word.
func (s *HighlightStore) Contains(networkID, word string, mapping irc.CaseMapping) bool {
	return s.list.contains(networkID, word, mapping)
}

// Add records word and reports whether it was newly added.
func (s *HighlightStore) Add(networkID, word string, mapping irc.CaseMapping) bool {
	return s.list.add(networkID, word, mapping)
}

// Remove drops word and reports whether anything changed.
func (s *HighlightStore) Remove(networkID, word string, mapping irc.CaseMapping) bool {
	return s.list.remove(networkID, word, mapping)
}

// Forget drops the whole network's list.
func (s *HighlightStore) Forget(networkID string) { s.list.forget(networkID) }

// AvatarStore is the in-memory per-network standing avatar URL. It mirrors the
// per-network lookup half of IrcAvatarStore; the fetch cache and disk
// persistence are deferred.
type AvatarStore struct{ urls map[string]string }

// NewAvatarStore returns an empty avatar store.
func NewAvatarStore() *AvatarStore { return &AvatarStore{} }

// URLForNetwork returns the stored avatar URL for one network, or "".
func (s *AvatarStore) URLForNetwork(networkID string) string {
	if networkID == "" {
		return ""
	}
	return s.urls[networkID]
}

// SetURL records the avatar URL for one network. An empty URL clears it.
func (s *AvatarStore) SetURL(networkID, url string) {
	if networkID == "" {
		return
	}
	if url == "" {
		delete(s.urls, networkID)
		return
	}
	if s.urls == nil {
		s.urls = make(map[string]string)
	}
	s.urls[networkID] = url
}

// Forget drops one network's stored URL.
func (s *AvatarStore) Forget(networkID string) {
	if networkID == "" {
		return
	}
	delete(s.urls, networkID)
}

// Preferences is the in-memory /pref toggle set: reopen direct messages,
// show peer avatars, and open conversations at unread. All three default on.
// It mirrors the three global Preferences getters the Qt window reads; disk
// persistence lands in phase 11.
type Preferences struct {
	values map[irc.PrefName]bool
	set    map[irc.PrefName]bool
}

// NewPreferences returns the default toggle set: every pref on.
func NewPreferences() *Preferences {
	return &Preferences{}
}

// Enabled reports the stored value for one pref, defaulting to on.
func (p *Preferences) Enabled(name irc.PrefName) bool {
	if p.set[name] {
		return p.values[name]
	}
	return true
}

// SetEnabled stores one pref.
func (p *Preferences) SetEnabled(name irc.PrefName, enabled bool) {
	if p.values == nil {
		p.values = make(map[irc.PrefName]bool)
		p.set = make(map[irc.PrefName]bool)
	}
	p.values[name] = enabled
	p.set[name] = true
}

// ignoreNickIsUsable reports whether nick is a plausible /ignore or /monitor
// target: non-empty, not a channel, and free of wildcard and mask punctuation.
// It mirrors ignoreNickIsUsable in irccommanddispatcher.cpp and
// ircmonitorcoordinator.cpp.
func ignoreNickIsUsable(nick string, features irc.ServerFeatures) bool {
	if nick == "" {
		return false
	}
	if features.IsChannel(nick) {
		return false
	}
	for index := 0; index < len(nick); index++ {
		switch nick[index] {
		case '!', '@', '*', ',':
			return false
		}
	}
	return true
}

// muteTargetIsUsable reports whether target may be muted: a channel, or a nick
// that passes ignoreNickIsUsable. It mirrors muteTargetIsUsable in
// irccommanddispatcher.cpp.
func muteTargetIsUsable(target string, features irc.ServerFeatures) bool {
	if target == "" {
		return false
	}
	if features.IsChannel(target) {
		return true
	}
	return ignoreNickIsUsable(target, features)
}

// firstToken returns the first whitespace-separated token of argument, or the
// whole argument when it carries none. It mirrors the anonymous firstToken
// helpers in irccommanddispatcher.cpp and ircmonitorcoordinator.cpp.
func firstToken(argument string) string {
	if space := strings.IndexByte(argument, ' '); space >= 0 {
		return argument[:space]
	}
	return argument
}

// restAfterFirstToken returns the trimmed remainder after the first space, or
// "". It mirrors the anonymous restAfterFirstToken helpers.
func restAfterFirstToken(argument string) string {
	space := strings.IndexByte(argument, ' ')
	if space < 0 {
		return ""
	}
	return strings.TrimSpace(argument[space+1:])
}

// parameterText returns one message parameter as display text, or "". It
// mirrors the anonymous parameter() helper that wraps ircWireText.
func parameterText(message irc.Message, index int) string {
	if index < 0 || index >= len(message.Params) {
		return ""
	}
	return irc.WireText([]byte(message.Params[index]))
}
