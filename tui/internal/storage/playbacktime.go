package storage

import (
	"bytes"
	"encoding/json"
	"strconv"
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// Group and key names must match src/irc/ircplaybacktime.cpp byte for byte so
// the Qt client and the terminal client share one INI section.
const (
	playbackTimesGroup = "playbackTimes"
	playbackTimesKey   = "times"
)

// PlaybackTargetTime pairs a target with its newest parseable server-time.
type PlaybackTargetTime struct {
	Target string
	When   time.Time
}

// playbackEntry is one persisted target/stamp row. The stamp stays a UTC
// millisecond instant, exactly like IrcPlaybackTimeStore::Entry.
type playbackEntry struct {
	target  string
	epochMs int64
}

// playbackJSONRow is the compact JSON wire shape: {"ms":"<millis>","target":"..."}.
// The field order matches QJsonObject's sorted keys, so the bytes agree with Qt.
type playbackJSONRow struct {
	MS     string `json:"ms"`
	Target string `json:"target"`
}

// PlaybackTimeStore persists the newest server-time per target per network,
// mirroring IrcPlaybackTimeStore. Group "playbackTimes/<networkId>", key
// "times" (a compact JSON array of {"ms":"<millis>","target":"..."}).
//
// Like IrcOpenDirectStore it caches loaded rows per network and, when
// ephemeral, never touches disk. SetEphemeral(true) clears the cache.
type PlaybackTimeStore struct {
	ephemeral bool
	cache     map[string][]playbackEntry
}

// NewPlaybackTimeStore returns an empty, disk-backed store.
func NewPlaybackTimeStore() *PlaybackTimeStore {
	return &PlaybackTimeStore{cache: make(map[string][]playbackEntry)}
}

// SetEphemeral switches the store between disk and an in-memory-only cache.
// Turning it on clears the cache; turning it off keeps the cache, mirroring
// IrcPlaybackTimeStore::setEphemeral.
func (s *PlaybackTimeStore) SetEphemeral(ephemeral bool) {
	s.ephemeral = ephemeral
	if ephemeral {
		s.cache = make(map[string][]playbackEntry)
	}
}

// playbackLoad reads and parses the persisted rows for networkID. Only objects
// with a non-empty target and a parseable integer-string ms survive; malformed
// JSON yields nil.
func (s *PlaybackTimeStore) playbackLoad(networkID string) []playbackEntry {
	if networkID == "" {
		return nil
	}
	if s.ephemeral {
		return s.cache[networkID]
	}
	settings := OpenSettings("")
	stored := settings.Value(playbackTimesGroup+"/"+networkID, playbackTimesKey)
	if stored == "" {
		return nil
	}
	var rawRows []json.RawMessage
	if err := json.Unmarshal([]byte(stored), &rawRows); err != nil {
		return nil
	}
	var rows []playbackEntry
	for _, raw := range rawRows {
		var object map[string]json.RawMessage
		if err := json.Unmarshal(raw, &object); err != nil {
			// Not a JSON object; skip it like QJsonValue::isObject().
			continue
		}
		var target string
		if rawTarget, ok := object["target"]; ok {
			_ = json.Unmarshal(rawTarget, &target)
		}
		if target == "" {
			continue
		}
		var msText string
		if rawMS, ok := object["ms"]; ok {
			_ = json.Unmarshal(rawMS, &msText)
		}
		ms, err := strconv.ParseInt(msText, 10, 64)
		if err != nil {
			continue
		}
		rows = append(rows, playbackEntry{target: target, epochMs: ms})
	}
	return rows
}

// playbackSave persists rows for networkID. An empty list removes the whole
// playbackTimes/<networkId> group instead of writing an empty JSON array. It is
// a no-op for an empty networkID.
func (s *PlaybackTimeStore) playbackSave(networkID string, rows []playbackEntry) {
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
	group := playbackTimesGroup + "/" + networkID
	if len(rows) == 0 {
		settings.RemoveGroup(group)
	} else {
		jsonRows := make([]playbackJSONRow, 0, len(rows))
		for _, row := range rows {
			jsonRows = append(jsonRows, playbackJSONRow{
				MS:     strconv.FormatInt(row.epochMs, 10),
				Target: row.target,
			})
		}
		settings.SetValue(group, playbackTimesKey, playbackEncodeRows(jsonRows))
	}
	settings.Sync()
}

// playbackEncodeRows renders the compact JSON QJsonDocument::Compact produces,
// without Go's HTML escaping so the bytes stay close to Qt's.
func playbackEncodeRows(rows []playbackJSONRow) string {
	var buffer bytes.Buffer
	encoder := json.NewEncoder(&buffer)
	encoder.SetEscapeHTML(false)
	if err := encoder.Encode(rows); err != nil {
		return ""
	}
	return strings.TrimRight(buffer.String(), "\n")
}

// playbackEntries returns a copy of the cached rows for networkID, loading them
// from disk on first use.
func (s *PlaybackTimeStore) playbackEntries(networkID string) []playbackEntry {
	if networkID == "" {
		return nil
	}
	if found, ok := s.cache[networkID]; ok {
		return append([]playbackEntry(nil), found...)
	}
	loaded := s.playbackLoad(networkID)
	s.cache[networkID] = loaded
	return append([]playbackEntry(nil), loaded...)
}

// Newest returns the maximum stamp across all targets, or the zero time and
// false when the network holds none.
func (s *PlaybackTimeStore) Newest(networkID string) (time.Time, bool) {
	rows := s.playbackEntries(networkID)
	if len(rows) == 0 {
		return time.Time{}, false
	}
	maxMs := rows[0].epochMs
	for _, row := range rows {
		if row.epochMs > maxMs {
			maxMs = row.epochMs
		}
	}
	return time.UnixMilli(maxMs).UTC(), true
}

// Targets returns every target/stamp pair in insertion order.
func (s *PlaybackTimeStore) Targets(networkID string) []PlaybackTargetTime {
	rows := s.playbackEntries(networkID)
	stamps := make([]PlaybackTargetTime, 0, len(rows))
	for _, row := range rows {
		stamps = append(stamps, PlaybackTargetTime{
			Target: row.target,
			When:   time.UnixMilli(row.epochMs).UTC(),
		})
	}
	return stamps
}

// Noted returns the stored stamp for target, or the zero time and false when
// the networkID/target is empty or the target is unknown.
func (s *PlaybackTimeStore) Noted(networkID, target string, mapping irc.CaseMapping) (time.Time, bool) {
	if networkID == "" || target == "" {
		return time.Time{}, false
	}
	rows := s.playbackEntries(networkID)
	index := playbackIndexOfTarget(rows, target, mapping)
	if index < 0 {
		return time.Time{}, false
	}
	return time.UnixMilli(rows[index].epochMs).UTC(), true
}

// Note records when for target, rejecting an empty networkID/target, a zero
// instant, and any stamp that is not strictly newer than the stored one. It
// returns whether the store changed.
func (s *PlaybackTimeStore) Note(networkID, target string, when time.Time, mapping irc.CaseMapping) bool {
	if networkID == "" || target == "" || when.IsZero() {
		return false
	}
	ms := when.UTC().UnixMilli()
	rows := s.playbackEntries(networkID)
	index := playbackIndexOfTarget(rows, target, mapping)
	if index >= 0 {
		if ms <= rows[index].epochMs {
			return false
		}
		rows[index].epochMs = ms
	} else {
		rows = append(rows, playbackEntry{target: target, epochMs: ms})
	}
	s.cache[networkID] = rows
	s.playbackSave(networkID, rows)
	return true
}

// Rekey renames oldTarget to newTarget. A case-mapped-but-byte-different
// spelling is rewritten in place; otherwise the old entry is dropped and its
// stamp merged into newTarget (raised to the max, or appended when new).
// It returns false for empty arguments or a missing oldTarget.
func (s *PlaybackTimeStore) Rekey(networkID, oldTarget, newTarget string, mapping irc.CaseMapping) bool {
	if networkID == "" || oldTarget == "" || newTarget == "" {
		return false
	}
	rows := s.playbackEntries(networkID)
	oldIndex := playbackIndexOfTarget(rows, oldTarget, mapping)
	if oldIndex < 0 {
		return false
	}
	ms := rows[oldIndex].epochMs
	if playbackTargetEquals(mapping, rows[oldIndex].target, newTarget) {
		if rows[oldIndex].target == newTarget {
			return false
		}
		rows[oldIndex].target = newTarget
	} else {
		rows = append(rows[:oldIndex], rows[oldIndex+1:]...)
		existing := playbackIndexOfTarget(rows, newTarget, mapping)
		if existing >= 0 {
			if ms > rows[existing].epochMs {
				rows[existing].epochMs = ms
			}
		} else {
			rows = append(rows, playbackEntry{target: newTarget, epochMs: ms})
		}
	}
	s.cache[networkID] = rows
	s.playbackSave(networkID, rows)
	return true
}

// Forget drops the cache entry and removes the persisted group.
func (s *PlaybackTimeStore) Forget(networkID string) {
	if networkID == "" {
		return
	}
	delete(s.cache, networkID)
	s.playbackSave(networkID, nil)
}

// playbackIndexOfTarget returns the first case-mapped match, or -1.
func playbackIndexOfTarget(rows []playbackEntry, target string, mapping irc.CaseMapping) int {
	for index, row := range rows {
		if playbackTargetEquals(mapping, row.target, target) {
			return index
		}
	}
	return -1
}

// playbackTargetEquals compares two targets under mapping.
func playbackTargetEquals(mapping irc.CaseMapping, left, right string) bool {
	return mapping.Equals(left, right)
}

// PlaybackPlayStamp renders a ZNC PLAY "from" stamp: Unix seconds plus a
// 3-digit millisecond fraction, or "0" when ok is false. Negative instants
// keep a leading '-'. It mirrors ircPlaybackPlayStamp.
func PlaybackPlayStamp(when time.Time, ok bool) string {
	if !ok || when.IsZero() {
		return "0"
	}
	ms := when.UTC().UnixMilli()
	negative := ms < 0
	magnitude := ms
	if negative {
		magnitude = -ms
	}
	seconds := magnitude / 1000
	fraction := strconv.FormatInt(magnitude%1000, 10)
	for len(fraction) < 3 {
		fraction = "0" + fraction
	}
	text := strconv.FormatInt(seconds, 10) + "." + fraction
	if negative {
		text = "-" + text
	}
	return text
}
