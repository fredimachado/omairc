package storage

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

func scrollTestRoots(t *testing.T) (config, state string) {
	t.Helper()
	root := t.TempDir()
	config = filepath.Join(root, "config-1d06")
	state = filepath.Join(root, "state-1d06")
	if err := os.MkdirAll(config, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(state, 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("XDG_CONFIG_HOME", config)
	t.Setenv("XDG_STATE_HOME", state)
	t.Setenv("OMAIRC_DEV_HOME", "")
	return config, state
}

func TestScrollPlaceJSONBytesRekeyForget(t *testing.T) {
	_, state := scrollTestRoots(t)
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

	raw, err := os.ReadFile(filepath.Join(state, "omairc", ScrollPlacesFile))
	if err != nil {
		t.Fatalf("state read: %v", err)
	}
	if strings.Contains(string(raw), `"target":"#omarchy"`) {
		t.Fatalf("forget must remove the network's places:\n%s", raw)
	}
}

func TestScrollPlaceEphemeralSkipsDisk(t *testing.T) {
	config, state := scrollTestRoots(t)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	store := NewScrollPlaceStore()
	store.SetEphemeral(true)
	store.Remember("net-1", "#omarchy", ScrollPlace{FollowEnd: true}, mapping)
	if _, err := os.Stat(filepath.Join(config, "omairc", "omairc.conf")); !os.IsNotExist(err) {
		t.Fatalf("ephemeral remember must not create a config, err=%v", err)
	}
	if _, err := os.Stat(filepath.Join(state, "omairc", ScrollPlacesFile)); !os.IsNotExist(err) {
		t.Fatalf("ephemeral remember must not create scrollplaces.ini, err=%v", err)
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
	_, state := scrollTestRoots(t)
	settings := OpenSettings(filepath.Join(state, "omairc", ScrollPlacesFile))
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

func TestScrollPlaceSaveLeavesConfBytesAlone(t *testing.T) {
	config, state := scrollTestRoots(t)
	conf := filepath.Join(config, "omairc", "omairc.conf")
	if err := os.MkdirAll(filepath.Dir(conf), 0o700); err != nil {
		t.Fatal(err)
	}
	before := []byte("[preferences]\ndirects=true\n[scrollPlaces]\nnet1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#qa\\\"}]\"\n")
	if err := os.WriteFile(conf, before, 0o600); err != nil {
		t.Fatal(err)
	}
	time.Sleep(1100 * time.Millisecond)
	info, err := os.Stat(conf)
	if err != nil {
		t.Fatal(err)
	}
	mtime := info.ModTime()

	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	NewScrollPlaceStore().Remember("net1", "#qa", ScrollPlace{MsgID: "m-1"}, mapping)

	after, err := os.ReadFile(conf)
	if err != nil {
		t.Fatal(err)
	}
	if string(after) != string(before) {
		t.Fatalf("conf bytes changed:\nbefore=%q\nafter=%q", before, after)
	}
	info, err = os.Stat(conf)
	if err != nil {
		t.Fatal(err)
	}
	if !info.ModTime().Equal(mtime) {
		t.Fatalf("conf mtime changed: %v -> %v", mtime, info.ModTime())
	}
	statePath := filepath.Join(state, "omairc", ScrollPlacesFile)
	raw, err := os.ReadFile(statePath)
	if err != nil {
		t.Fatalf("state file missing: %v", err)
	}
	if !strings.Contains(string(raw), "m-1") {
		t.Fatalf("state file missing place: %s", raw)
	}
	if strings.Contains(string(after), "m-1") {
		t.Fatal("save must not write the new place into omairc.conf")
	}
}

func TestScrollPlaceLegacyConfFallbackOnce(t *testing.T) {
	config, state := scrollTestRoots(t)
	conf := filepath.Join(config, "omairc", "omairc.conf")
	if err := os.MkdirAll(filepath.Dir(conf), 0o700); err != nil {
		t.Fatal(err)
	}
	before := []byte("[scrollPlaces]\nnet1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#qa\\\"}]\"\n")
	if err := os.WriteFile(conf, before, 0o600); err != nil {
		t.Fatal(err)
	}
	statePath := filepath.Join(state, "omairc", ScrollPlacesFile)
	if _, err := os.Stat(statePath); !os.IsNotExist(err) {
		t.Fatalf("state file must start missing, err=%v", err)
	}

	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	place, ok := NewScrollPlaceStore().Place("net1", "#qa", mapping)
	if !ok || !place.FollowEnd {
		t.Fatalf("legacy place = %+v/%v", place, ok)
	}
	after, err := os.ReadFile(conf)
	if err != nil {
		t.Fatal(err)
	}
	if string(after) != string(before) {
		t.Fatalf("legacy read must leave conf untouched:\nbefore=%q\nafter=%q", before, after)
	}
	if _, err := os.Stat(statePath); !os.IsNotExist(err) {
		t.Fatalf("legacy read must not create the state file, err=%v", err)
	}

	NewScrollPlaceStore().Remember("net1", "#qa", ScrollPlace{FollowEnd: true}, mapping)
	after, err = os.ReadFile(conf)
	if err != nil {
		t.Fatal(err)
	}
	if string(after) != string(before) {
		t.Fatalf("save must leave legacy conf lines alone:\nbefore=%q\nafter=%q", before, after)
	}
	if _, err := os.Stat(statePath); err != nil {
		t.Fatalf("save must create the state file: %v", err)
	}
}

func TestScrollPlacePathHonoursXDGAndDevHome(t *testing.T) {
	_, state := scrollTestRoots(t)
	want := filepath.Join(state, "omairc", ScrollPlacesFile)
	if got := ScrollPlacesPath(); got != want {
		t.Fatalf("ScrollPlacesPath() = %q, want %q", got, want)
	}

	dev := t.TempDir()
	t.Setenv("OMAIRC_DEV_HOME", dev)
	wantDev := filepath.Join(dev, "state", ScrollPlacesFile)
	if got := ScrollPlacesPath(); got != wantDev {
		t.Fatalf("dev ScrollPlacesPath() = %q, want %q", got, wantDev)
	}
}

func TestScrollPlaceCrossClientFixture(t *testing.T) {
	_, state := scrollTestRoots(t)
	path := filepath.Join(state, "omairc", ScrollPlacesFile)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatal(err)
	}
	fixture := "[scrollPlaces]\nnet1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#qa\\\"},{\\\"follow\\\":false,\\\"msgid\\\":\\\"m-9\\\",\\\"target\\\":\\\"anna\\\"}]\"\n"
	if err := os.WriteFile(path, []byte(fixture), 0o600); err != nil {
		t.Fatal(err)
	}
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	store := NewScrollPlaceStore()
	qa, ok := store.Place("net1", "#qa", mapping)
	if !ok || !qa.FollowEnd {
		t.Fatalf("#qa = %+v/%v", qa, ok)
	}
	anna, ok := store.Place("net1", "anna", mapping)
	if !ok || anna.MsgID != "m-9" {
		t.Fatalf("anna = %+v/%v", anna, ok)
	}
}

func TestScrollPlaceSaveMustNotTargetConf(t *testing.T) {
	config, state := scrollTestRoots(t)
	conf := filepath.Join(config, "omairc", "omairc.conf")
	if err := os.MkdirAll(filepath.Dir(conf), 0o700); err != nil {
		t.Fatal(err)
	}
	before := []byte("[preferences]\nkeep=yes\n")
	if err := os.WriteFile(conf, before, 0o600); err != nil {
		t.Fatal(err)
	}
	time.Sleep(1100 * time.Millisecond)

	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	NewScrollPlaceStore().Remember("net1", "#qa", ScrollPlace{FollowEnd: true}, mapping)

	after, err := os.ReadFile(conf)
	if err != nil {
		t.Fatal(err)
	}
	if string(after) != string(before) {
		t.Fatalf("conf changed: %q -> %q", before, after)
	}
	if strings.Contains(string(after), "[scrollPlaces]") {
		t.Fatal("scroll place save must not add a [scrollPlaces] group to omairc.conf")
	}
	if _, err := os.Stat(filepath.Join(state, "omairc", ScrollPlacesFile)); err != nil {
		t.Fatalf("state file missing: %v", err)
	}
}
