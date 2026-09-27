package storage

import (
	"encoding/json"
	"os"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

func playbackTempConfig(t *testing.T) string {
	t.Helper()
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	return ConfigPath()
}

func TestPlaybackNoteNewestTargetsNoted(t *testing.T) {
	playbackTempConfig(t)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	store := NewPlaybackTimeStore()

	t1 := time.UnixMilli(1700000000123).UTC()
	t2 := time.UnixMilli(1700000001000).UTC()

	if _, ok := store.Newest("net-1"); ok {
		t.Fatal("Newest on an empty network should report false")
	}
	if _, ok := store.Noted("net-1", "alice", mapping); ok {
		t.Fatal("Noted on an empty network should report false")
	}
	if !store.Note("net-1", "alice", t1, mapping) {
		t.Fatal("Note(alice) should be true")
	}
	if !store.Note("net-1", "bob", t2, mapping) {
		t.Fatal("Note(bob) should be true")
	}
	if got, ok := store.Newest("net-1"); !ok || !got.Equal(t2) {
		t.Fatalf("Newest = %v/%v, want %v/true", got, ok, t2)
	}
	want := []PlaybackTargetTime{{Target: "alice", When: t1}, {Target: "bob", When: t2}}
	if got := store.Targets("net-1"); !reflect.DeepEqual(got, want) {
		t.Fatalf("Targets = %v, want %v", got, want)
	}
	if got, ok := store.Noted("net-1", "ALICE", mapping); !ok || !got.Equal(t1) {
		t.Fatalf("Noted(ALICE) = %v/%v, want %v/true", got, ok, t1)
	}
	if _, ok := store.Noted("net-1", "carol", mapping); ok {
		t.Fatal("Noted of an unknown target should report false")
	}

	// Empty arguments are rejected.
	if store.Note("", "alice", t1, mapping) {
		t.Fatal("Note with an empty networkID should be false")
	}
	if store.Note("net-1", "", t1, mapping) {
		t.Fatal("Note with an empty target should be false")
	}
	if store.Note("net-1", "alice", time.Time{}, mapping) {
		t.Fatal("Note with a zero instant should be false")
	}
	if _, ok := store.Noted("", "alice", mapping); ok {
		t.Fatal("Noted with an empty networkID should report false")
	}
	if _, ok := store.Noted("net-1", "", mapping); ok {
		t.Fatal("Noted with an empty target should report false")
	}
}

func TestPlaybackNoteRejectsStale(t *testing.T) {
	playbackTempConfig(t)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	store := NewPlaybackTimeStore()

	base := time.UnixMilli(1000).UTC()
	if !store.Note("net-1", "alice", base, mapping) {
		t.Fatal("Note(base) should be true")
	}
	if store.Note("net-1", "alice", base, mapping) {
		t.Fatal("Note with an equal stamp should be false")
	}
	if store.Note("net-1", "alice", time.UnixMilli(999).UTC(), mapping) {
		t.Fatal("Note with an older stamp should be false")
	}
	if !store.Note("net-1", "alice", time.UnixMilli(1001).UTC(), mapping) {
		t.Fatal("Note with a newer stamp should be true")
	}
	if got, ok := store.Noted("net-1", "alice", mapping); !ok || got.UnixMilli() != 1001 {
		t.Fatalf("Noted = %v/%v, want ms 1001", got, ok)
	}
}

func TestPlaybackRekey(t *testing.T) {
	playbackTempConfig(t)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	store := NewPlaybackTimeStore()

	t1 := time.UnixMilli(1000).UTC()
	t2 := time.UnixMilli(2000).UTC()

	store.Note("net-1", "alice", t1, mapping)

	// Empty arguments and a missing oldTarget are no-ops.
	if store.Rekey("", "alice", "bob", mapping) {
		t.Fatal("Rekey with an empty networkID should be false")
	}
	if store.Rekey("net-1", "", "bob", mapping) {
		t.Fatal("Rekey with an empty oldTarget should be false")
	}
	if store.Rekey("net-1", "alice", "", mapping) {
		t.Fatal("Rekey with an empty newTarget should be false")
	}
	if store.Rekey("net-1", "carol", "dave", mapping) {
		t.Fatal("Rekey of a missing oldTarget should be false")
	}

	// Case-mapped equal but byte-different: rewrite the target text in place.
	if !store.Rekey("net-1", "alice", "Alice", mapping) {
		t.Fatal("Rekey(alice->Alice) should be true")
	}
	if got, ok := store.Noted("net-1", "ALICE", mapping); !ok || !got.Equal(t1) {
		t.Fatalf("Noted(ALICE) = %v/%v, want %v/true", got, ok, t1)
	}
	if store.Rekey("net-1", "Alice", "Alice", mapping) {
		t.Fatal("Rekey to the identical spelling should be false")
	}

	// Distinct rename keeps the stamp.
	if !store.Rekey("net-1", "Alice", "bob", mapping) {
		t.Fatal("Rekey(Alice->bob) should be true")
	}
	if got, ok := store.Noted("net-1", "bob", mapping); !ok || !got.Equal(t1) {
		t.Fatalf("Noted(bob) = %v/%v, want %v/true", got, ok, t1)
	}

	// Rename onto an existing distinct target raises it to the max stamp.
	if !store.Note("net-1", "carol", t2, mapping) {
		t.Fatal("Note(carol) should be true")
	}
	if !store.Rekey("net-1", "carol", "bob", mapping) {
		t.Fatal("Rekey(carol->bob) should be true")
	}
	want := []PlaybackTargetTime{{Target: "bob", When: t2}}
	if got := store.Targets("net-1"); !reflect.DeepEqual(got, want) {
		t.Fatalf("Targets = %v, want %v", got, want)
	}

	// Merging an older stamp must not lower the existing one.
	store.Note("net-1", "dave", t2, mapping)
	store.Note("net-1", "erin", t1, mapping)
	if !store.Rekey("net-1", "erin", "dave", mapping) {
		t.Fatal("Rekey(erin->dave) should be true")
	}
	if got, ok := store.Noted("net-1", "dave", mapping); !ok || !got.Equal(t2) {
		t.Fatalf("Noted(dave) = %v/%v, want %v/true", got, ok, t2)
	}
}

func TestPlaybackForget(t *testing.T) {
	playbackTempConfig(t)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	store := NewPlaybackTimeStore()

	store.Note("net-1", "alice", time.UnixMilli(1000).UTC(), mapping)
	store.Forget("net-1")

	if _, ok := store.Newest("net-1"); ok {
		t.Fatal("Newest after Forget should report false")
	}
	fresh := NewPlaybackTimeStore()
	if got := fresh.Targets("net-1"); len(got) != 0 {
		t.Fatalf("fresh Targets after Forget = %v, want empty", got)
	}
	settings := OpenSettings("")
	if got := settings.Value("playbackTimes/net-1", "times"); got != "" {
		t.Fatalf("stored times after Forget = %q, want empty", got)
	}
}

func TestPlaybackPersistenceAndNaming(t *testing.T) {
	playbackTempConfig(t)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	t1 := time.UnixMilli(1700000000123).UTC()

	store := NewPlaybackTimeStore()
	if !store.Note("net-1", "alice", t1, mapping) {
		t.Fatal("Note(alice) should be true")
	}

	fresh := NewPlaybackTimeStore()
	if got, ok := fresh.Noted("net-1", "alice", mapping); !ok || !got.Equal(t1) {
		t.Fatalf("fresh Noted = %v/%v, want %v/true", got, ok, t1)
	}

	// On disk the value lives at playbackTimes/<id> with key times, as compact
	// JSON. Field order matches QJsonObject's sorted keys.
	settings := OpenSettings("")
	stored := settings.Value("playbackTimes/net-1", "times")
	if stored == "" {
		t.Fatal("stored playback times value is empty")
	}
	var rows []playbackJSONRow
	if err := json.Unmarshal([]byte(stored), &rows); err != nil {
		t.Fatalf("stored value is not JSON: %v (%q)", err, stored)
	}
	want := []playbackJSONRow{{MS: "1700000000123", Target: "alice"}}
	if !reflect.DeepEqual(rows, want) {
		t.Fatalf("stored rows = %v, want %v", rows, want)
	}
	raw, err := os.ReadFile(ConfigPath())
	if err != nil {
		t.Fatalf("ReadFile(%q) = %v", ConfigPath(), err)
	}
	if !strings.Contains(string(raw), "[playbackTimes]") {
		t.Fatalf("config file missing [playbackTimes] section:\n%s", raw)
	}
}

func TestPlaybackMalformedJSONYieldsEmpty(t *testing.T) {
	playbackTempConfig(t)

	seed := OpenSettings("")
	seed.SetValue("playbackTimes/net-1", "times", "not json")
	seed.Sync()

	store := NewPlaybackTimeStore()
	if got := store.Targets("net-1"); len(got) != 0 {
		t.Fatalf("Targets from malformed JSON = %v, want empty", got)
	}
}

func TestPlaybackSkipsUnparseableRows(t *testing.T) {
	playbackTempConfig(t)

	seed := OpenSettings("")
	seed.SetValue("playbackTimes/net-1", "times",
		`[{"target":"alice","ms":"1000"},{"target":"","ms":"2000"},`+
			`{"target":"bob","ms":"nope"},{"target":"carol","ms":3000},`+
			`"string",123,{"target":"dave","ms":"4000"}]`)
	seed.Sync()

	store := NewPlaybackTimeStore()
	want := []PlaybackTargetTime{
		{Target: "alice", When: time.UnixMilli(1000).UTC()},
		{Target: "dave", When: time.UnixMilli(4000).UTC()},
	}
	if got := store.Targets("net-1"); !reflect.DeepEqual(got, want) {
		t.Fatalf("Targets = %v, want %v", got, want)
	}
}

func TestPlaybackEphemeralNeverWrites(t *testing.T) {
	configPath := playbackTempConfig(t)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	t1 := time.UnixMilli(1000).UTC()

	store := NewPlaybackTimeStore()
	store.SetEphemeral(true)
	if !store.Note("net-1", "alice", t1, mapping) {
		t.Fatal("ephemeral Note should be true")
	}
	if got, ok := store.Noted("net-1", "alice", mapping); !ok || !got.Equal(t1) {
		t.Fatalf("ephemeral Noted = %v/%v, want %v/true", got, ok, t1)
	}

	if _, err := os.Stat(configPath); !os.IsNotExist(err) {
		t.Fatalf("ephemeral store wrote %q (stat err = %v)", configPath, err)
	}
	fresh := NewPlaybackTimeStore()
	if got := fresh.Targets("net-1"); len(got) != 0 {
		t.Fatalf("disk-backed store saw ephemeral data: %v", got)
	}

	// Turning ephemeral on again clears the cache.
	store.SetEphemeral(true)
	if got := store.Targets("net-1"); len(got) != 0 {
		t.Fatalf("Targets after SetEphemeral(true) = %v, want empty", got)
	}
}

func TestPlaybackPlayStamp(t *testing.T) {
	cases := []struct {
		name string
		when time.Time
		ok   bool
		want string
	}{
		{"milliseconds", time.UnixMilli(1700000000123).UTC(), true, "1700000000.123"},
		{"whole second", time.UnixMilli(1700000000000).UTC(), true, "1700000000.000"},
		{"single digit fraction", time.UnixMilli(1700000000007).UTC(), true, "1700000000.007"},
		{"epoch", time.UnixMilli(0).UTC(), true, "0.000"},
		{"zero value", time.Time{}, true, "0"},
		{"not ok", time.UnixMilli(1700000000123).UTC(), false, "0"},
		{"negative fraction", time.UnixMilli(-123).UTC(), true, "-0.123"},
		{"negative seconds", time.UnixMilli(-1500).UTC(), true, "-1.500"},
	}
	for _, tc := range cases {
		if got := PlaybackPlayStamp(tc.when, tc.ok); got != tc.want {
			t.Errorf("%s: PlaybackPlayStamp = %q, want %q", tc.name, got, tc.want)
		}
	}
}
