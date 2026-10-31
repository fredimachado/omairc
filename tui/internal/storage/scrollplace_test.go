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

func writeTwoNetworkLegacyConf(t *testing.T, conf string) []byte {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(conf), 0o700); err != nil {
		t.Fatal(err)
	}
	body := []byte("[scrollPlaces]\n" +
		"net1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#qa\\\"},{\\\"follow\\\":true,\\\"target\\\":\\\"#qb\\\"}]\"\n" +
		"net2\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#dev\\\"}]\"\n")
	if err := os.WriteFile(conf, body, 0o600); err != nil {
		t.Fatal(err)
	}
	return body
}

func TestScrollPlaceFirstSaveKeepsOtherNetworksFromLegacy(t *testing.T) {
	config, state := scrollTestRoots(t)
	conf := filepath.Join(config, "omairc", "omairc.conf")
	before := writeTwoNetworkLegacyConf(t, conf)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)

	NewScrollPlaceStore().Remember("net1", "#qa", ScrollPlace{MsgID: "m-qa"}, mapping)

	after, err := os.ReadFile(conf)
	if err != nil {
		t.Fatal(err)
	}
	if string(after) != string(before) {
		t.Fatalf("conf changed:\nbefore=%q\nafter=%q", before, after)
	}
	fresh := NewScrollPlaceStore()
	qa, ok := fresh.Place("net1", "#qa", mapping)
	if !ok || qa.MsgID != "m-qa" {
		t.Fatalf("net1#qa = %+v/%v", qa, ok)
	}
	dev, ok := fresh.Place("net2", "#dev", mapping)
	if !ok || !dev.FollowEnd {
		t.Fatalf("net2#dev must survive the first save: %+v/%v", dev, ok)
	}
	if _, err := os.Stat(filepath.Join(state, "omairc", ScrollPlacesFile)); err != nil {
		t.Fatalf("state file missing: %v", err)
	}
}

func TestScrollPlaceForgetSeedsLegacyNetworks(t *testing.T) {
	config, state := scrollTestRoots(t)
	conf := filepath.Join(config, "omairc", "omairc.conf")
	before := writeTwoNetworkLegacyConf(t, conf)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)

	store := NewScrollPlaceStore()
	store.Forget("net1")

	after, err := os.ReadFile(conf)
	if err != nil {
		t.Fatal(err)
	}
	if string(after) != string(before) {
		t.Fatalf("conf changed:\nbefore=%q\nafter=%q", before, after)
	}
	if _, err := os.Stat(filepath.Join(state, "omairc", ScrollPlacesFile)); err != nil {
		t.Fatalf("forget must create the state file: %v", err)
	}
	fresh := NewScrollPlaceStore()
	if _, ok := fresh.Place("net1", "#qa", mapping); ok {
		t.Fatal("forgotten net1 must be gone")
	}
	dev, ok := fresh.Place("net2", "#dev", mapping)
	if !ok || !dev.FollowEnd {
		t.Fatalf("net2#dev must survive forget: %+v/%v", dev, ok)
	}
}

func TestScrollPlaceForgetOnlyLegacyNetworkWritesEmptyState(t *testing.T) {
	config, state := scrollTestRoots(t)
	conf := filepath.Join(config, "omairc", "omairc.conf")
	if err := os.MkdirAll(filepath.Dir(conf), 0o700); err != nil {
		t.Fatal(err)
	}
	before := []byte("[scrollPlaces]\nnet1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#qa\\\"}]\"\n")
	if err := os.WriteFile(conf, before, 0o600); err != nil {
		t.Fatal(err)
	}
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	statePath := filepath.Join(state, "omairc", ScrollPlacesFile)

	NewScrollPlaceStore().Forget("net1")

	after, err := os.ReadFile(conf)
	if err != nil {
		t.Fatal(err)
	}
	if string(after) != string(before) {
		t.Fatalf("conf changed:\nbefore=%q\nafter=%q", before, after)
	}
	if _, err := os.Stat(statePath); err != nil {
		t.Fatalf("forgetting the only legacy network must create an empty state file: %v", err)
	}
	if _, ok := NewScrollPlaceStore().Place("net1", "#qa", mapping); ok {
		t.Fatal("forgotten sole legacy network must stay gone after reload")
	}
}

func TestScrollPlaceSeedReadsBothMacOSConfNames(t *testing.T) {
	config, state := scrollTestRoots(t)
	dir := filepath.Join(config, "omairc")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		t.Fatal(err)
	}
	ini := filepath.Join(dir, "omairc.ini")
	conf := filepath.Join(dir, "omairc.conf")
	if err := os.WriteFile(ini, []byte("[scrollPlaces]\n"+
		"net_ini\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#from-ini\\\"}]\"\n"+
		"net_both\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#ini-wins\\\"}]\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(conf, []byte("[scrollPlaces]\n"+
		"net_conf\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#from-conf\\\"}]\"\n"+
		"net_both\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#conf-loses\\\"}]\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	mapping := irc.NewCaseMapping(irc.KindRfc1459)

	NewScrollPlaceStore().Remember("net_ini", "#from-ini", ScrollPlace{FollowEnd: true}, mapping)

	fresh := NewScrollPlaceStore()
	if place, ok := fresh.Place("net_conf", "#from-conf", mapping); !ok || !place.FollowEnd {
		t.Fatalf("places only in omairc.conf must be seeded: %+v/%v", place, ok)
	}
	if place, ok := fresh.Place("net_both", "#ini-wins", mapping); !ok || !place.FollowEnd {
		t.Fatalf("omairc.ini must win per network: %+v/%v", place, ok)
	}
	if _, ok := fresh.Place("net_both", "#conf-loses", mapping); ok {
		t.Fatal("conf must not override ini for the same network")
	}
	if _, err := os.Stat(filepath.Join(state, "omairc", ScrollPlacesFile)); err != nil {
		t.Fatalf("state file missing: %v", err)
	}

	// Places only in omairc.ini must seed when the TUI's omairc.conf has none.
	config2, state2 := scrollTestRoots(t)
	dir2 := filepath.Join(config2, "omairc")
	if err := os.MkdirAll(dir2, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir2, "omairc.ini"), []byte(
		"[scrollPlaces]\nnet_ini_only\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#ini-only\\\"}]\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir2, "omairc.conf"), []byte("[preferences]\ndirects=true\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	NewScrollPlaceStore().Remember("other", "#x", ScrollPlace{FollowEnd: true}, mapping)
	only, ok := NewScrollPlaceStore().Place("net_ini_only", "#ini-only", mapping)
	if !ok || !only.FollowEnd {
		t.Fatalf("places only in omairc.ini must seed: %+v/%v", only, ok)
	}
	if _, err := os.Stat(filepath.Join(state2, "omairc", ScrollPlacesFile)); err != nil {
		t.Fatalf("state file missing: %v", err)
	}
}

func TestScrollPlaceUnreadableLegacyConfRefusesSave(t *testing.T) {
	config, state := scrollTestRoots(t)
	conf := filepath.Join(config, "omairc", "omairc.conf")
	if err := os.MkdirAll(filepath.Dir(conf), 0o700); err != nil {
		t.Fatal(err)
	}
	body := []byte("[scrollPlaces]\n" +
		"net1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#qa\\\"}]\"\n" +
		"net2\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#dev\\\"}]\"\n")
	if err := os.WriteFile(conf, body, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(conf, 0o000); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chmod(conf, 0o600) })
	if _, err := os.ReadFile(conf); err == nil {
		t.Skip("running as root: chmod 000 leaves the file readable")
	}

	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	store := NewScrollPlaceStore()
	store.Remember("net1", "#qa", ScrollPlace{FollowEnd: true}, mapping)
	if _, ok := store.Place("net1", "#qa", mapping); !ok {
		t.Fatal("refused save must keep places in memory")
	}
	statePath := filepath.Join(state, "omairc", ScrollPlacesFile)
	if _, err := os.Stat(statePath); !os.IsNotExist(err) {
		t.Fatalf("unreadable legacy conf must not create an unseeded state file, err=%v", err)
	}
}

func TestScrollPlaceSeedRunsOnlyWhenStateMissing(t *testing.T) {
	config, state := scrollTestRoots(t)
	conf := filepath.Join(config, "omairc", "omairc.conf")
	if err := os.MkdirAll(filepath.Dir(conf), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(conf, []byte(
		"[scrollPlaces]\nnet2\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#dev\\\"}]\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	statePath := filepath.Join(state, "omairc", ScrollPlacesFile)
	if err := os.MkdirAll(filepath.Dir(statePath), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(statePath, []byte(
		"[scrollPlaces]\nnet1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#qa\\\"}]\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	NewScrollPlaceStore().Remember("net1", "#qa", ScrollPlace{MsgID: "m-1"}, mapping)

	fresh := NewScrollPlaceStore()
	place, ok := fresh.Place("net1", "#qa", mapping)
	if !ok || place.MsgID != "m-1" {
		t.Fatalf("net1 = %+v/%v", place, ok)
	}
	if _, ok := fresh.Place("net2", "#dev", mapping); ok {
		t.Fatal("existing state file must not re-seed net2 from conf")
	}
}
