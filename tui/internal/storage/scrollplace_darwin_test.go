//go:build darwin

package storage

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

func TestScrollPlaceLiveDarwinSeedsBothConfNames(t *testing.T) {
	config, state := scrollTestRoots(t)
	dir := filepath.Join(config, "omairc")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		t.Fatal(err)
	}
	ini := filepath.Join(dir, "omairc.ini")
	conf := filepath.Join(dir, "omairc.conf")
	if err := os.WriteFile(ini, []byte("[scrollPlaces]\n"+
		"net_ini\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#from-ini\\\"}]\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(conf, []byte("[scrollPlaces]\n"+
		"net_conf\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#from-conf\\\"}]\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	mapping := irc.NewCaseMapping(irc.KindRfc1459)

	NewScrollPlaceStore().Remember("net_ini", "#from-ini", ScrollPlace{FollowEnd: true}, mapping)

	fresh := NewScrollPlaceStore()
	if place, ok := fresh.Place("net_conf", "#from-conf", mapping); !ok || !place.FollowEnd {
		t.Fatalf("live darwin gate must seed omairc.conf-only networks: %+v/%v", place, ok)
	}
	if _, err := os.Stat(filepath.Join(state, "omairc", ScrollPlacesFile)); err != nil {
		t.Fatalf("state file missing: %v", err)
	}
}
