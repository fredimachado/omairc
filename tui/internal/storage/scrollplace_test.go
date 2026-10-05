package storage

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

func TestScrollPlaceJSONBytesRekeyForget(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	store := NewScrollPlaceStore()

	store.Remember("net-1", "#omarchy", ScrollPlace{FollowEnd: true}, mapping)
	if got, want := store.PlacesJSON("net-1"), `[{"follow":true,"target":"#omarchy"}]`; got != want {
		t.Fatalf("follow JSON = %s, want %s", got, want)
	}

	store.Remember("net-1", "#omarchy", ScrollPlace{MsgID: "abc"}, mapping)
	if got, want := store.PlacesJSON("net-1"), `[{"follow":false,"msgid":"abc","target":"#omarchy"}]`; got != want {
		t.Fatalf("msgid JSON = %s, want %s", got, want)
	}

	store.Remember("net-1", "#omarchy", ScrollPlace{
		Author:  "anna",
		Body:    "hi <&>",
		Kind:    "message",
		EpochMs: 1700000000123,
		HasTime: true,
	}, mapping)
	wantContent := `[{"author":"anna","body":"hi <&>","follow":false,"kind":"message","ms":"1700000000123","target":"#omarchy"}]`
	if got := store.PlacesJSON("net-1"); got != wantContent {
		t.Fatalf("content JSON = %s, want %s", got, wantContent)
	}

	store.Remember("net-1", "#lab", ScrollPlace{Kind: "event", Body: "x"}, mapping)
	wantBoth := wantContent[:len(wantContent)-1] + `,{"author":"","body":"x","follow":false,"kind":"event","target":"#lab"}]`
	if got := store.PlacesJSON("net-1"); got != wantBoth {
		t.Fatalf("two places = %s, want %s", got, wantBoth)
	}

	fresh := NewScrollPlaceStore()
	place, ok := fresh.Place("net-1", "#OMARCHY", mapping)
	if !ok || place.FollowEnd || place.Body != "hi <&>" || !place.HasTime || place.EpochMs != 1700000000123 {
		t.Fatalf("reloaded place = %+v/%v", place, ok)
	}
	lab, ok := fresh.Place("net-1", "#lab", mapping)
	if !ok || lab.Author != "" || lab.Kind != "event" || lab.HasTime {
		t.Fatalf("reloaded lab = %+v/%v", lab, ok)
	}

	if !store.Rekey("net-1", "#lab", "#Lab", mapping) {
		t.Fatal("case rekey should rewrite the spelling")
	}
	if got := store.PlacesJSON("net-1"); !strings.Contains(got, `"target":"#Lab"`) {
		t.Fatalf("case rekey JSON = %s", got)
	}
	if !store.Rekey("net-1", "#Lab", "#omarchy", mapping) {
		t.Fatal("rekey onto an existing target should replace it")
	}
	replaced, ok := store.Place("net-1", "#omarchy", mapping)
	if !ok || replaced.Body != "x" {
		t.Fatalf("replaced place = %+v/%v", replaced, ok)
	}
	if _, ok := store.Place("net-1", "#lab", mapping); ok {
		t.Fatal("the old target must be gone after rekey")
	}

	store.Forget("net-1")
	if _, ok := NewScrollPlaceStore().Place("net-1", "#omarchy", mapping); ok {
		t.Fatal("forget must drop the group")
	}

	raw, err := os.ReadFile(filepath.Join(dir, "omairc", "omairc.conf"))
	if err != nil {
		t.Fatalf("config read: %v", err)
	}
	if strings.Contains(string(raw), "scrollPlaces") {
		t.Fatalf("forget must remove the scrollPlaces group:\n%s", raw)
	}
}

func TestScrollPlaceEphemeralSkipsDisk(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	store := NewScrollPlaceStore()
	store.SetEphemeral(true)
	store.Remember("net-1", "#omarchy", ScrollPlace{FollowEnd: true}, mapping)
	if _, err := os.Stat(filepath.Join(dir, "omairc", "omairc.conf")); !os.IsNotExist(err) {
		t.Fatalf("ephemeral remember must not create a config, err=%v", err)
	}
	if _, ok := store.Place("net-1", "#omarchy", mapping); !ok {
		t.Fatal("ephemeral remember must stay in memory")
	}
	store.SetEphemeral(true)
	if _, ok := store.Place("net-1", "#omarchy", mapping); ok {
		t.Fatal("SetEphemeral(true) must clear the cache")
	}
}

func TestScrollPlaceSkipsMalformed(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	settings := OpenSettings("")
	settings.SetValue(scrollPlacesGroup+"/net-1", scrollPlacesKey,
		`[{"target":"#omarchy"},{"follow":false,"ms":"nope","target":"#bad"},{"follow":true,"target":"#ok"}]`)
	settings.Sync()
	store := NewScrollPlaceStore()
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	if _, ok := store.Place("net-1", "#omarchy", mapping); ok {
		t.Fatal("a row without follow must be skipped")
	}
	if _, ok := store.Place("net-1", "#bad", mapping); ok {
		t.Fatal("a non-integer ms must drop the row")
	}
	place, ok := store.Place("net-1", "#ok", mapping)
	if !ok || !place.FollowEnd {
		t.Fatalf("valid row = %+v/%v", place, ok)
	}
}
