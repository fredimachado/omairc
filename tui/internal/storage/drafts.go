package storage

import (
	"bytes"
	"encoding/json"
	"errors"
	"os"
	"sort"
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// DraftsFile is the shared composer-drafts file name in the state directory.
const DraftsFile = "drafts.json"

// Drafts are each conversation's unsent composer text, kept across a restart in
// drafts.json beside scrollplaces.ini. The desktop client reads and writes the
// same file; keep this byte for byte compatible with
// client/src/storage/drafts.rs.
//
// Wire format, version 1 (compact JSON, UTF-8, no trailing newline, keys sorted
// at every level):
//
//	{"drafts":{"<networkId>":{"<normalized target>":{"ms":<epoch ms>,"text":"..."}}},"version":1}
//
// ms is when the text last changed. An entry at least DraftTTL old is dropped on
// load and pruned on every save. Empty text removes its entry. A file whose
// version is newer than this client's is read as empty and never overwritten.
const (
	// DraftTTL is how long an unchanged draft is kept.
	DraftTTL = 7 * 24 * time.Hour
	// DraftsVersion is the only format version this client writes.
	DraftsVersion = 1
	// MaxDraftBytes is the longest text kept. A longer draft is not persisted.
	MaxDraftBytes = 16 * 1024
	// MaxDrafts is the most entries kept across every network.
	MaxDrafts = 200
)

// StoredDraft is one stored draft.
type StoredDraft struct {
	Text string
	// Ms is the wall-clock time, in milliseconds since the epoch, at which the
	// text last changed.
	Ms int64
}

// draftMap is network id, then normalized target, then the draft.
type draftMap map[string]map[string]StoredDraft

// DraftStore keeps composer drafts on disk. New starts ephemeral so unit tests
// and the demo never touch the user's state; the controller opts in through
// SetEphemeral(false), which the shell does for a live launch only.
type DraftStore struct {
	path      string
	ephemeral bool
	cache     draftMap
}

// NewDraftStore returns an ephemeral store for the shared state-dir file.
func NewDraftStore() *DraftStore {
	return &DraftStore{ephemeral: true, cache: draftMap{}}
}

// NewDraftStoreAt returns a disk-backed store at path (tests).
func NewDraftStoreAt(path string) *DraftStore {
	return &DraftStore{path: path, cache: draftMap{}}
}

// SetEphemeral switches the store between disk and nothing. Turning it on
// clears the cache.
func (s *DraftStore) SetEphemeral(ephemeral bool) {
	s.ephemeral = ephemeral
	if ephemeral {
		s.cache = draftMap{}
	}
}

func (s *DraftStore) file() string {
	if s.path != "" {
		return s.path
	}
	return DraftsPath()
}

// Load reads the file into the cache, dropping every expired draft. A missing,
// corrupt or newer-version file reads as empty.
func (s *DraftStore) Load(now time.Time) {
	s.cache = draftMap{}
	if s.ephemeral {
		return
	}
	path := s.file()
	if path == "" {
		return
	}
	if m, state := readDrafts(path); state == draftsOK {
		pruneDrafts(m, now.UnixMilli())
		s.cache = m
	}
}

// Get returns the stored draft for target on networkID under mapping.
func (s *DraftStore) Get(networkID, target string, mapping irc.CaseMapping) (string, bool) {
	if s == nil || s.ephemeral {
		return "", false
	}
	draft, ok := s.cache[networkID][mapping.Normalize(target)]
	if !ok {
		return "", false
	}
	return draft.Text, true
}

// Remember stores text as the draft for target on networkID, or removes the
// draft when text is blank, too long, or irc.DraftMayBePersisted rejects it. A
// rejected text also removes any earlier draft for the key, so a stale one
// cannot linger on disk. The file is re-read first, so another client's keys
// are kept. Nothing is written when the key's text does not change.
func (s *DraftStore) Remember(networkID, target, text string, mapping irc.CaseMapping, channelTypes string, now time.Time) {
	if s == nil || s.ephemeral || networkID == "" || target == "" {
		return
	}
	normalized := mapping.Normalize(target)
	if normalized == "" {
		return
	}
	wanted := text
	if strings.TrimSpace(wanted) == "" || len(wanted) > MaxDraftBytes || !irc.DraftMayBePersisted(wanted, channelTypes) {
		wanted = ""
	}
	nowMs := now.UnixMilli()
	have, had := s.cache[networkID][normalized]
	if wanted == "" && !had {
		return
	}
	if wanted != "" && had && have.Text == wanted {
		return
	}
	path := s.file()
	if path == "" {
		return
	}
	m, state := readDrafts(path)
	switch state {
	case draftsNewer, draftsUnreadable:
		return
	case draftsReplaceable:
		m = draftMap{}
	}
	pruneDrafts(m, nowMs)
	cur, hasCur := m[networkID][normalized]
	switch {
	case wanted == "" && !hasCur:
		s.cache = m
		return
	case wanted != "" && hasCur && cur.Text == wanted:
		s.cache = m
		return
	case wanted == "":
		delete(m[networkID], normalized)
		if len(m[networkID]) == 0 {
			delete(m, networkID)
		}
	default:
		if m[networkID] == nil {
			m[networkID] = map[string]StoredDraft{}
		}
		m[networkID][normalized] = StoredDraft{Text: wanted, Ms: nowMs}
	}
	capDrafts(m)
	data := encodeDrafts(m)
	if data == nil {
		return
	}
	if err := writeSettingsAtomic(path, data, false); err != nil {
		return
	}
	s.cache = m
}

type draftsState int

const (
	draftsOK draftsState = iota
	// draftsReplaceable is a missing or non-version-1 document: it reads as
	// empty and may be overwritten.
	draftsReplaceable
	// draftsNewer was written by a newer client and is never overwritten.
	draftsNewer
	draftsUnreadable
)

func readDrafts(path string) (draftMap, draftsState) {
	data, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return draftMap{}, draftsReplaceable
		}
		return draftMap{}, draftsUnreadable
	}
	var root map[string]json.RawMessage
	if err := json.Unmarshal(data, &root); err != nil {
		return draftMap{}, draftsReplaceable
	}
	var version int64
	if err := json.Unmarshal(root["version"], &version); err != nil {
		return draftMap{}, draftsReplaceable
	}
	if version > DraftsVersion {
		return draftMap{}, draftsNewer
	}
	if version != DraftsVersion {
		return draftMap{}, draftsReplaceable
	}
	out := draftMap{}
	var networks map[string]json.RawMessage
	if json.Unmarshal(root["drafts"], &networks) != nil {
		return out, draftsOK
	}
	for network, raw := range networks {
		var targets map[string]json.RawMessage
		if network == "" || json.Unmarshal(raw, &targets) != nil {
			continue
		}
		for target, entryRaw := range targets {
			var entry struct {
				Ms   *int64  `json:"ms"`
				Text *string `json:"text"`
			}
			if target == "" || json.Unmarshal(entryRaw, &entry) != nil {
				continue
			}
			if entry.Ms == nil || entry.Text == nil || strings.TrimSpace(*entry.Text) == "" {
				continue
			}
			if out[network] == nil {
				out[network] = map[string]StoredDraft{}
			}
			out[network][target] = StoredDraft{Text: *entry.Text, Ms: *entry.Ms}
		}
	}
	return out, draftsOK
}

// pruneDrafts drops every draft at least DraftTTL old. A stamp in the future is
// kept.
func pruneDrafts(m draftMap, nowMs int64) {
	ttl := DraftTTL.Milliseconds()
	for network, targets := range m {
		for target, draft := range targets {
			if nowMs-draft.Ms >= ttl {
				delete(targets, target)
			}
		}
		if len(targets) == 0 {
			delete(m, network)
		}
	}
}

// capDrafts keeps the newest MaxDrafts entries. Ties go by network then target.
func capDrafts(m draftMap) {
	type row struct {
		ms              int64
		network, target string
	}
	var rows []row
	for network, targets := range m {
		for target, draft := range targets {
			rows = append(rows, row{draft.Ms, network, target})
		}
	}
	if len(rows) <= MaxDrafts {
		return
	}
	sort.Slice(rows, func(i, j int) bool {
		a, b := rows[i], rows[j]
		if a.ms != b.ms {
			return a.ms < b.ms
		}
		if a.network != b.network {
			return a.network < b.network
		}
		return a.target < b.target
	})
	for _, r := range rows[:len(rows)-MaxDrafts] {
		delete(m[r.network], r.target)
		if len(m[r.network]) == 0 {
			delete(m, r.network)
		}
	}
}

type draftEntryJSON struct {
	Ms   int64  `json:"ms"`
	Text string `json:"text"`
}

type draftsFileJSON struct {
	Drafts  map[string]map[string]draftEntryJSON `json:"drafts"`
	Version int                                  `json:"version"`
}

// encodeDrafts writes the version-1 bytes. encoding/json sorts map keys and
// struct fields are declared in sorted order, which matches the Rust writer.
func encodeDrafts(m draftMap) []byte {
	out := draftsFileJSON{Drafts: map[string]map[string]draftEntryJSON{}, Version: DraftsVersion}
	for network, targets := range m {
		out.Drafts[network] = map[string]draftEntryJSON{}
		for target, draft := range targets {
			out.Drafts[network][target] = draftEntryJSON{Ms: draft.Ms, Text: draft.Text}
		}
	}
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(out); err != nil {
		return nil
	}
	return bytes.TrimSuffix(buf.Bytes(), []byte("\n"))
}
