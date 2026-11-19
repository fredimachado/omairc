package storage

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// keep identical to client/src/storage/drafts.rs
const (
	draftsFixture   = `{"drafts":{"libera":{"#a":{"ms":1780000000000,"text":"half typed"}}},"version":1}`
	draftsFixtureMs = int64(1780000000000)
	draftsEmpty     = `{"drafts":{},"version":1}`
)

func msTime(ms int64) time.Time { return time.UnixMilli(ms) }

func draftFile(t *testing.T) (*DraftStore, string) {
	t.Helper()
	path := filepath.Join(t.TempDir(), DraftsFile)
	return NewDraftStoreAt(path), path
}

func readFile(t *testing.T, path string) string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	return string(data)
}

var rfc = irc.NewCaseMapping(irc.KindRfc1459)

func remember(s *DraftStore, net, target, text string, ms int64) {
	s.Remember(net, target, text, rfc, "#&", msTime(ms))
}

func TestDraftRoundTripsAndSurvivesANewStore(t *testing.T) {
	s, path := draftFile(t)
	remember(s, "libera", "#a", "half typed", draftsFixtureMs)
	again := NewDraftStoreAt(path)
	again.Load(msTime(draftsFixtureMs + 1000))
	if got, ok := again.Get("libera", "#a", rfc); !ok || got != "half typed" {
		t.Fatalf("Get = %q, %v", got, ok)
	}
}

func TestDraftFixtureBytesAreExact(t *testing.T) {
	s, path := draftFile(t)
	remember(s, "libera", "#a", "half typed", draftsFixtureMs)
	if got := readFile(t, path); got != draftsFixture {
		t.Fatalf("file = %q, want %q", got, draftsFixture)
	}
}

func TestDraftReadsTheRustWrittenFixture(t *testing.T) {
	s, path := draftFile(t)
	if err := os.WriteFile(path, []byte(draftsFixture), 0o600); err != nil {
		t.Fatal(err)
	}
	s.Load(msTime(draftsFixtureMs + 1000))
	if got, ok := s.Get("libera", "#a", rfc); !ok || got != "half typed" {
		t.Fatalf("Get = %q, %v", got, ok)
	}
}

func TestDraftKeysAreSortedAtEveryLevel(t *testing.T) {
	s, path := draftFile(t)
	remember(s, "oftc", "#z", "z", 1)
	remember(s, "libera", "#b", "b", 2)
	remember(s, "libera", "#a", "a", 3)
	want := `{"drafts":{"libera":{"#a":{"ms":3,"text":"a"},"#b":{"ms":2,"text":"b"}},"oftc":{"#z":{"ms":1,"text":"z"}}},"version":1}`
	if got := readFile(t, path); got != want {
		t.Fatalf("file = %q, want %q", got, want)
	}
}

func TestDraftTextIsEscapedAndReadBackExactly(t *testing.T) {
	s, path := draftFile(t)
	text := "say \"hi\" \\ <b>&amp;</b> tab\t é\U0001F600"
	remember(s, "libera", "#a", text, 5)
	if !strings.Contains(readFile(t, path), "<b>&amp;</b>") {
		t.Fatalf("HTML was escaped: %s", readFile(t, path))
	}
	again := NewDraftStoreAt(path)
	again.Load(msTime(6))
	if got, _ := again.Get("libera", "#a", rfc); got != text {
		t.Fatalf("Get = %q, want %q", got, text)
	}
}

func TestDraftOlderThanSevenDaysIsDroppedOnLoad(t *testing.T) {
	s, path := draftFile(t)
	if err := os.WriteFile(path, []byte(draftsFixture), 0o600); err != nil {
		t.Fatal(err)
	}
	ttl := DraftTTL.Milliseconds()
	s.Load(msTime(draftsFixtureMs + ttl))
	if _, ok := s.Get("libera", "#a", rfc); ok {
		t.Fatal("a draft exactly seven days old must be dropped")
	}
	s.Load(msTime(draftsFixtureMs + ttl - 1))
	if got, ok := s.Get("libera", "#a", rfc); !ok || got != "half typed" {
		t.Fatalf("a draft just under seven days old must stay: %q %v", got, ok)
	}
}

func TestDraftFutureStampIsKept(t *testing.T) {
	s, path := draftFile(t)
	if err := os.WriteFile(path, []byte(draftsFixture), 0o600); err != nil {
		t.Fatal(err)
	}
	s.Load(msTime(draftsFixtureMs - 10000))
	if _, ok := s.Get("libera", "#a", rfc); !ok {
		t.Fatal("a future stamp is kept")
	}
}

func TestDraftStorePrunesExpiredEntriesOfOtherKeys(t *testing.T) {
	s, path := draftFile(t)
	if err := os.WriteFile(path, []byte(draftsFixture), 0o600); err != nil {
		t.Fatal(err)
	}
	remember(s, "oftc", "#b", "fresh", draftsFixtureMs+DraftTTL.Milliseconds())
	text := readFile(t, path)
	if strings.Contains(text, "half typed") || !strings.Contains(text, "fresh") {
		t.Fatalf("file = %s", text)
	}
}

func TestDraftWhitespaceOnlyTextDeletesTheEntryAndEmptyNetworks(t *testing.T) {
	s, path := draftFile(t)
	remember(s, "libera", "#a", "x", 10)
	remember(s, "oftc", "#b", "y", 11)
	remember(s, "libera", "#a", "   ", 12)
	if got, want := readFile(t, path), `{"drafts":{"oftc":{"#b":{"ms":11,"text":"y"}}},"version":1}`; got != want {
		t.Fatalf("file = %q, want %q", got, want)
	}
	remember(s, "oftc", "#b", "", 13)
	if got := readFile(t, path); got != draftsEmpty {
		t.Fatalf("file = %q, want %q", got, draftsEmpty)
	}
}

func TestDraftUnchangedTextDoesNotBumpMsOrWrite(t *testing.T) {
	s, path := draftFile(t)
	marked := draftsFixture + "\n"
	if err := os.WriteFile(path, []byte(marked), 0o600); err != nil {
		t.Fatal(err)
	}
	remember(s, "libera", "#a", "half typed", draftsFixtureMs+5000)
	if got := readFile(t, path); got != marked {
		t.Fatalf("an unchanged draft rewrote the file: %q", got)
	}
	remember(s, "libera", "#a", "changed", draftsFixtureMs+6000)
	again := NewDraftStoreAt(path)
	again.Load(msTime(draftsFixtureMs + 7000))
	if !strings.Contains(readFile(t, path), `"ms":1780000006000`) {
		t.Fatalf("a changed draft takes the new stamp: %s", readFile(t, path))
	}
}

func TestDraftDeletingAnAbsentKeyWritesNothing(t *testing.T) {
	s, path := draftFile(t)
	remember(s, "libera", "#a", "", 1)
	if _, err := os.Stat(path); err == nil {
		t.Fatal("nothing should have been written")
	}
}

func TestDraftNewerVersionIsNeverOverwritten(t *testing.T) {
	s, path := draftFile(t)
	newer := `{"drafts":{"libera":{"#a":{"ms":1,"text":"x"}}},"version":2}`
	if err := os.WriteFile(path, []byte(newer), 0o600); err != nil {
		t.Fatal(err)
	}
	s.Load(msTime(2))
	if _, ok := s.Get("libera", "#a", rfc); ok {
		t.Fatal("a newer file reads as empty")
	}
	remember(s, "libera", "#b", "y", 3)
	if got := readFile(t, path); got != newer {
		t.Fatalf("a newer file was overwritten: %q", got)
	}
}

func TestDraftCorruptFileReadsEmptyAndIsReplaced(t *testing.T) {
	s, path := draftFile(t)
	if err := os.WriteFile(path, []byte("{not json"), 0o600); err != nil {
		t.Fatal(err)
	}
	s.Load(msTime(1))
	remember(s, "libera", "#a", "x", 7)
	if got, want := readFile(t, path), `{"drafts":{"libera":{"#a":{"ms":7,"text":"x"}}},"version":1}`; got != want {
		t.Fatalf("file = %q, want %q", got, want)
	}
}

func TestDraftMalformedEntriesAreSkippedAndUnknownFieldsDroppedOnRewrite(t *testing.T) {
	s, path := draftFile(t)
	in := `{"drafts":{"libera":{"#a":{"ms":5,"text":"keep","extra":true},"#b":{"ms":"x","text":"bad"},"#c":{"ms":5},"#d":{"ms":5,"text":"  "}},"oftc":3},"version":1}`
	if err := os.WriteFile(path, []byte(in), 0o600); err != nil {
		t.Fatal(err)
	}
	s.Load(msTime(6))
	remember(s, "libera", "#e", "new", 6)
	want := `{"drafts":{"libera":{"#a":{"ms":5,"text":"keep"},"#e":{"ms":6,"text":"new"}}},"version":1}`
	if got := readFile(t, path); got != want {
		t.Fatalf("file = %q, want %q", got, want)
	}
}

func TestDraftOversizeTextIsNotWrittenAndClearsTheEarlierDraft(t *testing.T) {
	s, path := draftFile(t)
	remember(s, "libera", "#a", "small", 1)
	remember(s, "libera", "#a", strings.Repeat("x", MaxDraftBytes+1), 2)
	if got := readFile(t, path); got != draftsEmpty {
		t.Fatalf("file = %q", got)
	}
	exact := strings.Repeat("x", MaxDraftBytes)
	remember(s, "libera", "#a", exact, 3)
	if got, ok := s.Get("libera", "#a", rfc); !ok || got != exact {
		t.Fatal("a draft at the limit is kept")
	}
}

func TestDraftOldestEntriesGoPastTheCap(t *testing.T) {
	s, path := draftFile(t)
	for i := 0; i < MaxDrafts+3; i++ {
		remember(s, "libera", "#c"+pad3(i), "t", int64(1000+i))
	}
	again := NewDraftStoreAt(path)
	again.Load(msTime(5000))
	for _, gone := range []string{"#c000", "#c002"} {
		if _, ok := again.Get("libera", gone, rfc); ok {
			t.Fatalf("%s should have been dropped", gone)
		}
	}
	for _, kept := range []string{"#c003", "#c" + pad3(MaxDrafts+2)} {
		if _, ok := again.Get("libera", kept, rfc); !ok {
			t.Fatalf("%s should have been kept", kept)
		}
	}
}

func pad3(i int) string {
	s := "000" + string(rune('0'+i/100)) + string(rune('0'+i/10%10)) + string(rune('0'+i%10))
	return s[len(s)-3:]
}

func TestDraftKeyFollowsTheCaseMapping(t *testing.T) {
	s, path := draftFile(t)
	remember(s, "libera", "#A", "upper", 1)
	if !strings.Contains(readFile(t, path), `"#a"`) {
		t.Fatalf("the key is stored normalized: %s", readFile(t, path))
	}
	if got, ok := s.Get("libera", "#a", rfc); !ok || got != "upper" {
		t.Fatalf("Get(#a) = %q %v", got, ok)
	}
	s.Remember("libera", "#x[y", "br", rfc, "#&", msTime(2))
	if got, ok := s.Get("libera", "#X{Y", rfc); !ok || got != "br" {
		t.Fatalf("rfc1459 folds [ and {: %q %v", got, ok)
	}
	ascii := irc.NewCaseMapping(irc.KindAscii)
	if _, ok := s.Get("libera", "#x[y", ascii); ok {
		t.Fatal("ascii keeps [ and { distinct")
	}
}

func TestDraftEphemeralWritesNothing(t *testing.T) {
	s, path := draftFile(t)
	s.SetEphemeral(true)
	remember(s, "libera", "#a", "x", 1)
	if _, err := os.Stat(path); err == nil {
		t.Fatal("an ephemeral store wrote a file")
	}
	if _, ok := s.Get("libera", "#a", rfc); ok {
		t.Fatal("an ephemeral store keeps nothing")
	}
}

func TestDraftGateRejectsSecretsAndClearsTheEarlierDraft(t *testing.T) {
	s, path := draftFile(t)
	remember(s, "libera", "#a", "half typed", 1)
	s.Remember("libera", "#a", "/msg NickServ IDENTIFY pw", rfc, "#&", msTime(2))
	if got := readFile(t, path); got != draftsEmpty {
		t.Fatalf("file = %q", got)
	}
}
