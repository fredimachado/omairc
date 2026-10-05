package storage

import (
	"bytes"
	"encoding/json"
	"strconv"
	"strings"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// Group and key names must match src/irc/ircscrollplace.cpp byte for byte so
// the Qt client and the terminal client share one INI section.
const (
	scrollPlacesGroup = "scrollPlaces"
	scrollPlacesKey   = "places"
)

// ScrollPlace is where one conversation's transcript was last left. FollowEnd
// means the viewport was pinned to the bottom. Otherwise the anchor is the
// first visible message: its msgid when the server gave one, else the author,
// kind, body, and server time.
type ScrollPlace struct {
	FollowEnd bool
	MsgID     string
	Author    string
	Kind      string
	Body      string
	EpochMs   int64
	HasTime   bool
}

type scrollEntry struct {
	target string
	place  ScrollPlace
}

// scrollFollowJSON is {"follow":true,"target":"..."}. Field order matches
// QJsonObject's sorted keys.
type scrollFollowJSON struct {
	Follow bool   `json:"follow"`
	Target string `json:"target"`
}

// scrollMsgidJSON is {"follow":false,"msgid":"...","target":"..."}.
type scrollMsgidJSON struct {
	Follow bool   `json:"follow"`
	MsgID  string `json:"msgid"`
	Target string `json:"target"`
}

// scrollContentJSON is the author/kind/body anchor. ms is omitted when the
// message had no timestamp. Empty author and body stay in the object.
type scrollContentJSON struct {
	Author string `json:"author"`
	Body   string `json:"body"`
	Follow bool   `json:"follow"`
	Kind   string `json:"kind"`
	MS     string `json:"ms,omitempty"`
	Target string `json:"target"`
}

// ScrollPlaceStore persists one place per target per network, mirroring
// IrcScrollPlaceStore. Group "scrollPlaces/<networkId>", key "places".
// When ephemeral, nothing touches disk. SetEphemeral(true) clears the cache.
type ScrollPlaceStore struct {
	ephemeral bool
	cache     map[string][]scrollEntry
}

// NewScrollPlaceStore returns an empty, disk-backed store.
func NewScrollPlaceStore() *ScrollPlaceStore {
	return &ScrollPlaceStore{cache: make(map[string][]scrollEntry)}
}

// SetEphemeral switches the store between disk and an in-memory-only cache.
// Turning it on clears the cache; turning it off keeps the cache.
func (s *ScrollPlaceStore) SetEphemeral(ephemeral bool) {
	s.ephemeral = ephemeral
	if ephemeral {
		s.cache = make(map[string][]scrollEntry)
	}
}

func (s *ScrollPlaceStore) load(networkID string) []scrollEntry {
	if networkID == "" {
		return nil
	}
	if s.ephemeral {
		return s.cache[networkID]
	}
	settings := OpenSettings("")
	stored := settings.Value(scrollPlacesGroup+"/"+networkID, scrollPlacesKey)
	if stored == "" {
		return nil
	}
	var rawRows []json.RawMessage
	if err := json.Unmarshal([]byte(stored), &rawRows); err != nil {
		return nil
	}
	var rows []scrollEntry
	for _, raw := range rawRows {
		var object map[string]json.RawMessage
		if err := json.Unmarshal(raw, &object); err != nil {
			continue
		}
		entry, ok := scrollEntryFromObject(object)
		if !ok {
			continue
		}
		rows = append(rows, entry)
	}
	return rows
}

func scrollEntryFromObject(object map[string]json.RawMessage) (scrollEntry, bool) {
	rawFollow, hasFollow := object["follow"]
	if !hasFollow {
		return scrollEntry{}, false
	}
	var follow bool
	if err := json.Unmarshal(rawFollow, &follow); err != nil {
		return scrollEntry{}, false
	}
	var target string
	if rawTarget, ok := object["target"]; ok {
		_ = json.Unmarshal(rawTarget, &target)
	}
	if target == "" {
		return scrollEntry{}, false
	}
	place := ScrollPlace{FollowEnd: follow}
	if follow {
		return scrollEntry{target: target, place: place}, true
	}
	var msgid string
	if rawMsgID, ok := object["msgid"]; ok {
		_ = json.Unmarshal(rawMsgID, &msgid)
	}
	place.MsgID = msgid
	if msgid != "" {
		return scrollEntry{target: target, place: place}, true
	}
	if rawAuthor, ok := object["author"]; ok {
		_ = json.Unmarshal(rawAuthor, &place.Author)
	}
	if rawBody, ok := object["body"]; ok {
		_ = json.Unmarshal(rawBody, &place.Body)
	}
	if rawKind, ok := object["kind"]; ok {
		_ = json.Unmarshal(rawKind, &place.Kind)
	}
	var msText string
	if rawMS, ok := object["ms"]; ok {
		_ = json.Unmarshal(rawMS, &msText)
	}
	if msText != "" {
		ms, err := strconv.ParseInt(msText, 10, 64)
		if err != nil {
			return scrollEntry{}, false
		}
		place.EpochMs = ms
		place.HasTime = true
	}
	return scrollEntry{target: target, place: place}, true
}

func (entry scrollEntry) jsonValue() any {
	if entry.place.FollowEnd {
		return scrollFollowJSON{Follow: true, Target: entry.target}
	}
	if entry.place.MsgID != "" {
		return scrollMsgidJSON{Follow: false, MsgID: entry.place.MsgID, Target: entry.target}
	}
	row := scrollContentJSON{
		Author: entry.place.Author,
		Body:   entry.place.Body,
		Follow: false,
		Kind:   entry.place.Kind,
		Target: entry.target,
	}
	if entry.place.HasTime {
		row.MS = strconv.FormatInt(entry.place.EpochMs, 10)
	}
	return row
}

func (s *ScrollPlaceStore) save(networkID string, rows []scrollEntry) {
	if networkID == "" {
		return
	}
	if s.ephemeral {
		if len(rows) == 0 {
			delete(s.cache, networkID)
		} else {
			s.cache[networkID] = rows
		}
		return
	}
	settings := OpenSettings("")
	group := scrollPlacesGroup + "/" + networkID
	if len(rows) == 0 {
		settings.RemoveGroup(group)
	} else {
		encoded := make([]any, 0, len(rows))
		for _, row := range rows {
			encoded = append(encoded, row.jsonValue())
		}
		settings.SetValue(group, scrollPlacesKey, scrollEncode(encoded))
	}
	settings.Sync()
}

func scrollEncode(rows []any) string {
	var buffer bytes.Buffer
	encoder := json.NewEncoder(&buffer)
	encoder.SetEscapeHTML(false)
	if err := encoder.Encode(rows); err != nil {
		return ""
	}
	return strings.TrimRight(buffer.String(), "\n")
}

func (s *ScrollPlaceStore) entries(networkID string) []scrollEntry {
	if networkID == "" {
		return nil
	}
	if found, ok := s.cache[networkID]; ok {
		return append([]scrollEntry(nil), found...)
	}
	loaded := s.load(networkID)
	s.cache[networkID] = loaded
	return append([]scrollEntry(nil), loaded...)
}

func scrollIndexOfTarget(rows []scrollEntry, target string, mapping irc.CaseMapping) int {
	for index := range rows {
		if mapping.Equals(rows[index].target, target) {
			return index
		}
	}
	return -1
}

// Place returns the stored place for target.
func (s *ScrollPlaceStore) Place(networkID, target string, mapping irc.CaseMapping) (ScrollPlace, bool) {
	if networkID == "" || target == "" {
		return ScrollPlace{}, false
	}
	rows := s.entries(networkID)
	index := scrollIndexOfTarget(rows, target, mapping)
	if index < 0 {
		return ScrollPlace{}, false
	}
	return rows[index].place, true
}

// Remember records place for target, replacing any previous place.
func (s *ScrollPlaceStore) Remember(networkID, target string, place ScrollPlace, mapping irc.CaseMapping) {
	if networkID == "" || target == "" {
		return
	}
	rows := s.entries(networkID)
	index := scrollIndexOfTarget(rows, target, mapping)
	if index >= 0 {
		rows[index] = scrollEntry{target: target, place: place}
	} else {
		rows = append(rows, scrollEntry{target: target, place: place})
	}
	s.cache[networkID] = rows
	s.save(networkID, rows)
}

// Rekey moves oldTarget's place onto newTarget, replacing a place that
// already names the new target. A case-only rename rewrites the spelling.
func (s *ScrollPlaceStore) Rekey(networkID, oldTarget, newTarget string, mapping irc.CaseMapping) bool {
	if networkID == "" || oldTarget == "" || newTarget == "" {
		return false
	}
	rows := s.entries(networkID)
	oldIndex := scrollIndexOfTarget(rows, oldTarget, mapping)
	if oldIndex < 0 {
		return false
	}
	kept := rows[oldIndex].place
	if mapping.Equals(rows[oldIndex].target, newTarget) {
		if rows[oldIndex].target == newTarget {
			return false
		}
		rows[oldIndex].target = newTarget
	} else {
		rows = append(rows[:oldIndex], rows[oldIndex+1:]...)
		existing := scrollIndexOfTarget(rows, newTarget, mapping)
		if existing >= 0 {
			rows[existing] = scrollEntry{target: newTarget, place: kept}
		} else {
			rows = append(rows, scrollEntry{target: newTarget, place: kept})
		}
	}
	s.cache[networkID] = rows
	s.save(networkID, rows)
	return true
}

// Forget drops every place for networkID.
func (s *ScrollPlaceStore) Forget(networkID string) {
	if networkID == "" {
		return
	}
	delete(s.cache, networkID)
	s.save(networkID, nil)
}

// PlacesJSON is the compact JSON stored for networkID, for tests that lock
// the Qt byte contract. An unknown network returns "".
func (s *ScrollPlaceStore) PlacesJSON(networkID string) string {
	if networkID == "" {
		return ""
	}
	if s.ephemeral {
		rows := s.cache[networkID]
		if len(rows) == 0 {
			return ""
		}
		encoded := make([]any, 0, len(rows))
		for _, row := range rows {
			encoded = append(encoded, row.jsonValue())
		}
		return scrollEncode(encoded)
	}
	settings := OpenSettings("")
	return settings.Value(scrollPlacesGroup+"/"+networkID, scrollPlacesKey)
}
