package controller

// Persisted composer drafts through the controller (drafts.go): what reaches
// drafts.json, what the secret gate keeps off it, and what a later run reads
// back. The Rust equivalent is client/src/controller/tests/drafts.rs.

import (
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

const draftsEmptyFile = `{"drafts":{},"version":1}`

func draftController(t *testing.T, path string, clock *session.FakeClock) *Controller {
	t.Helper()
	c := New()
	if clock != nil {
		c.SetClock(clock)
	} else {
		c.SetClock(session.NewFakeClock(time.Date(2026, 9, 12, 12, 0, 0, 0, time.UTC)))
	}
	c.SetDraftStore(storage.NewDraftStoreAt(path))
	return c
}

func draftsOnDisk(t *testing.T, path string) string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	return string(data)
}

func TestDraftIsWrittenAndReadBackByANewController(t *testing.T) {
	path := filepath.Join(t.TempDir(), storage.DraftsFile)
	first := draftController(t, path, nil)
	first.RememberDraft("omarchy", "#a", "half typed")
	if !strings.Contains(draftsOnDisk(t, path), `"#a":{"ms":`) {
		t.Fatalf("file = %s", draftsOnDisk(t, path))
	}
	second := draftController(t, path, nil)
	if got, ok := second.DraftFor("omarchy", "#a"); !ok || got != "half typed" {
		t.Fatalf("DraftFor = %q, %v", got, ok)
	}
	if _, ok := second.DraftFor("omarchy", "#b"); ok {
		t.Fatal("no draft for #b")
	}
}

func TestDraftKeyFollowsTheNetworksCaseMapping(t *testing.T) {
	path := filepath.Join(t.TempDir(), storage.DraftsFile)
	c := draftController(t, path, nil)
	c.RememberDraft("omarchy", "#Mixed", "text")
	if !strings.Contains(draftsOnDisk(t, path), `"#mixed"`) {
		t.Fatalf("file = %s", draftsOnDisk(t, path))
	}
	if got, ok := c.DraftFor("omarchy", "#MIXED"); !ok || got != "text" {
		t.Fatalf("DraftFor = %q, %v", got, ok)
	}
}

func TestEmptiedDraftIsRemoved(t *testing.T) {
	path := filepath.Join(t.TempDir(), storage.DraftsFile)
	c := draftController(t, path, nil)
	c.RememberDraft("omarchy", "#a", "half typed")
	c.RememberDraft("omarchy", "#a", "  ")
	if got := draftsOnDisk(t, path); got != draftsEmptyFile {
		t.Fatalf("file = %q", got)
	}
	if _, ok := c.DraftFor("omarchy", "#a"); ok {
		t.Fatal("the draft should be gone")
	}
}

func TestDraftNotUpdatedForSevenDaysIsGoneAfterRelaunch(t *testing.T) {
	path := filepath.Join(t.TempDir(), storage.DraftsFile)
	start := time.Date(2026, 9, 12, 12, 0, 0, 0, time.UTC)
	draftController(t, path, session.NewFakeClock(start)).RememberDraft("omarchy", "#a", "half typed")

	kept := draftController(t, path, session.NewFakeClock(start.Add(storage.DraftTTL-time.Second)))
	if _, ok := kept.DraftFor("omarchy", "#a"); !ok {
		t.Fatal("a draft just under seven days old must stay")
	}
	expired := draftController(t, path, session.NewFakeClock(start.Add(storage.DraftTTL+time.Second)))
	if _, ok := expired.DraftFor("omarchy", "#a"); ok {
		t.Fatal("a draft older than seven days must be gone after relaunch")
	}
}

func TestSecretIsNeverWrittenAndClearsAnEarlierDraft(t *testing.T) {
	for _, secret := range []string{
		"/msg NickServ IDENTIFY pw",
		"/ns identify pw",
		"/quote PASS hunter2",
		"/oper a b",
		"identify pw",
		"PASS hunter2",
	} {
		path := filepath.Join(t.TempDir(), storage.DraftsFile)
		c := draftController(t, path, nil)
		c.RememberDraft("omarchy", "#a", "half typed")
		c.RememberDraft("omarchy", "#a", secret)
		got := draftsOnDisk(t, path)
		if strings.Contains(got, "pw") || strings.Contains(got, "hunter2") {
			t.Fatalf("%q reached the file: %s", secret, got)
		}
		if got != draftsEmptyFile {
			t.Fatalf("%q must also clear the earlier draft, file = %q", secret, got)
		}
		if _, ok := c.DraftFor("omarchy", "#a"); ok {
			t.Fatalf("%q left a draft in memory", secret)
		}
	}
}

func TestSecretTypedInANickServQueryIsNeverWritten(t *testing.T) {
	for _, target := range []string{"NickServ", "nickserv"} {
		path := filepath.Join(t.TempDir(), storage.DraftsFile)
		c := draftController(t, path, nil)
		c.RememberDraft("omarchy", target, "IDENTIFY pw")
		c.RememberDraft("omarchy", target, "identify account pw")
		if strings.Contains(draftsOnDisk(t, path), "pw") {
			t.Fatalf("%s: %s", target, draftsOnDisk(t, path))
		}
	}
}

func TestMeLinePersistsAndOtherSlashLinesDoNot(t *testing.T) {
	path := filepath.Join(t.TempDir(), storage.DraftsFile)
	c := draftController(t, path, nil)
	c.RememberDraft("omarchy", "#a", "/me waves")
	if !strings.Contains(draftsOnDisk(t, path), "/me waves") {
		t.Fatalf("file = %s", draftsOnDisk(t, path))
	}
	for _, line := range []string{"/join #a", "/msg NickServ IDENTIFY pw", "/quote PASS x"} {
		c.RememberDraft("omarchy", "#a", line)
		if got := draftsOnDisk(t, path); got != draftsEmptyFile {
			t.Fatalf("%q: file = %q", line, got)
		}
	}
}

func TestEphemeralControllerKeepsNothing(t *testing.T) {
	c := New()
	c.RememberDraft("omarchy", "#a", "half typed")
	if _, ok := c.DraftFor("omarchy", "#a"); ok {
		t.Fatal("an ephemeral controller keeps no drafts")
	}
}

func TestSetEphemeralFalseLoadsTheSharedFile(t *testing.T) {
	state := t.TempDir()
	t.Setenv("XDG_STATE_HOME", state)
	t.Setenv("OMAIRC_DEV_HOME", "")
	path := storage.DraftsPath()
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatal(err)
	}
	now := time.Now()
	fixture := `{"drafts":{"libera":{"#a":{"ms":` + strconv.FormatInt(now.UnixMilli(), 10) + `,"text":"from the desktop"}}},"version":1}`
	if err := os.WriteFile(path, []byte(fixture), 0o600); err != nil {
		t.Fatal(err)
	}
	c := New()
	if _, ok := c.DraftFor("libera", "#a"); ok {
		t.Fatal("a fresh controller is ephemeral")
	}
	c.SetEphemeral(false)
	if got, ok := c.DraftFor("libera", "#a"); !ok || got != "from the desktop" {
		t.Fatalf("DraftFor = %q, %v", got, ok)
	}
	c.SetEphemeral(true)
	if _, ok := c.DraftFor("libera", "#a"); ok {
		t.Fatal("going ephemeral clears the drafts")
	}
}

func TestAnotherClientsDraftSurvivesAnEmptyComposerHere(t *testing.T) {
	path := filepath.Join(t.TempDir(), storage.DraftsFile)
	c := draftController(t, path, nil)
	// Another client writes #a after this process loaded its cache.
	other := storage.NewDraftStoreAt(path)
	other.Remember("omarchy", "#a", "theirs", irc.NewCaseMapping(irc.KindRfc1459), "#&", time.Now())
	c.RememberDraft("omarchy", "#a", "")
	if !strings.Contains(draftsOnDisk(t, path), "theirs") {
		t.Fatalf("file = %s", draftsOnDisk(t, path))
	}
	// A write of our own for another key does not adopt their draft as ours either.
	c.RememberDraft("omarchy", "#b", "ours")
	c.RememberDraft("omarchy", "#a", "")
	if !strings.Contains(draftsOnDisk(t, path), "theirs") {
		t.Fatalf("file after our own write = %s", draftsOnDisk(t, path))
	}
}

func TestOnlyAChangeAgainstThisProcesssOwnViewReachesTheFile(t *testing.T) {
	path := filepath.Join(t.TempDir(), storage.DraftsFile)
	c := draftController(t, path, nil)
	c.RememberDraft("omarchy", "#a", "mine")
	if !strings.Contains(draftsOnDisk(t, path), "mine") {
		t.Fatalf("file = %s", draftsOnDisk(t, path))
	}
	c.RememberDraft("omarchy", "#a", "")
	if got := draftsOnDisk(t, path); got != draftsEmptyFile {
		t.Fatalf("a draft we stored is ours to delete, file = %q", got)
	}
}
