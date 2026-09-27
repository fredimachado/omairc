package storage

import (
	"os"
	"reflect"
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// openDirectTempConfig points XDG_CONFIG_HOME at a fresh temp dir and returns
// the shared Settings path inside it.
func openDirectTempConfig(t *testing.T) string {
	t.Helper()
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	return ConfigPath()
}

func TestOpenDirectEmptyNetworkAndArgs(t *testing.T) {
	openDirectTempConfig(t)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	store := NewOpenDirectStore()

	if got := store.Targets(""); got != nil {
		t.Fatalf("Targets(\"\") = %v, want nil", got)
	}
	if store.Add("", "alice", mapping) {
		t.Fatal("Add with an empty networkID should be false")
	}
	if store.Add("net-1", "", mapping) {
		t.Fatal("Add with an empty target should be false")
	}
	if store.Remove("", "alice", mapping) {
		t.Fatal("Remove with an empty networkID should be false")
	}
	if store.Remove("net-1", "", mapping) {
		t.Fatal("Remove with an empty target should be false")
	}
	if store.Rekey("", "a", "b", mapping) {
		t.Fatal("Rekey with an empty networkID should be false")
	}
	if store.Rekey("net-1", "", "b", mapping) {
		t.Fatal("Rekey with an empty oldTarget should be false")
	}
	if store.Rekey("net-1", "a", "", mapping) {
		t.Fatal("Rekey with an empty newTarget should be false")
	}
	store.Forget("") // no-op, must not panic or write
}

func TestOpenDirectAddDuplicateAndRemove(t *testing.T) {
	openDirectTempConfig(t)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	store := NewOpenDirectStore()

	if !store.Add("net-1", "Alice", mapping) {
		t.Fatal("Add(Alice) should be true")
	}
	if !store.Add("net-1", "Bob", mapping) {
		t.Fatal("Add(Bob) should be true")
	}
	if store.Add("net-1", "alice", mapping) {
		t.Fatal("Add of a case-mapped duplicate should be false")
	}
	if got, want := store.Targets("net-1"), []string{"Alice", "Bob"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Targets = %v, want %v", got, want)
	}
	if !store.Remove("net-1", "ALICE", mapping) {
		t.Fatal("Remove(ALICE) should be true")
	}
	if got, want := store.Targets("net-1"), []string{"Bob"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("after Remove Targets = %v, want %v", got, want)
	}
	if store.Remove("net-1", "alice", mapping) {
		t.Fatal("Remove of an absent target should be false")
	}
}

func TestOpenDirectRemoveEveryCaseMappedMatch(t *testing.T) {
	openDirectTempConfig(t)
	mapping := irc.NewCaseMapping(irc.KindAscii)

	seed := OpenSettings("")
	seed.SetStringList("openDirects/net-1", "targets", []string{"alice", "ALICE", "alice", "bob"})
	if status := seed.Sync(); status != StatusWritten {
		t.Fatalf("seed Sync = %v, want StatusWritten", status)
	}

	store := NewOpenDirectStore()
	if !store.Remove("net-1", "Alice", mapping) {
		t.Fatal("Remove(Alice) should be true")
	}
	if got, want := store.Targets("net-1"), []string{"bob"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Targets = %v, want %v", got, want)
	}
}

func TestOpenDirectRekey(t *testing.T) {
	openDirectTempConfig(t)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)
	store := NewOpenDirectStore()

	if !store.Add("net-1", "Alice", mapping) {
		t.Fatal("Add(Alice) should be true")
	}
	// Case-mapped equal but byte-different: rewritten in place.
	if !store.Rekey("net-1", "Alice", "alice", mapping) {
		t.Fatal("Rekey(Alice->alice) should be true")
	}
	if got, want := store.Targets("net-1"), []string{"alice"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Targets = %v, want %v", got, want)
	}
	// Exact same spelling: no change.
	if store.Rekey("net-1", "alice", "alice", mapping) {
		t.Fatal("Rekey to the identical spelling should be false")
	}
	// Missing oldTarget: no change.
	if store.Rekey("net-1", "carol", "carol2", mapping) {
		t.Fatal("Rekey of a missing oldTarget should be false")
	}
	// Rename to a distinct target.
	if !store.Rekey("net-1", "alice", "Alicia", mapping) {
		t.Fatal("Rekey(alice->Alicia) should be true")
	}
	if got, want := store.Targets("net-1"), []string{"Alicia"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Targets = %v, want %v", got, want)
	}
	// Rename onto an existing case-mapped target: old drops, no duplicate.
	if !store.Add("net-1", "Bob", mapping) {
		t.Fatal("Add(Bob) should be true")
	}
	if !store.Rekey("net-1", "Alicia", "bob", mapping) {
		t.Fatal("Rekey(Alicia->bob) should be true")
	}
	if got, want := store.Targets("net-1"), []string{"Bob"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Targets = %v, want %v", got, want)
	}
}

func TestOpenDirectListedDeduplicates(t *testing.T) {
	openDirectTempConfig(t)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)

	seed := OpenSettings("")
	seed.SetStringList("openDirects/net-1", "targets", []string{"alice", "ALICE", "bob", "Alice"})
	seed.Sync()

	store := NewOpenDirectStore()
	if got, want := store.Listed("net-1", mapping), []string{"alice", "bob"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Listed = %v, want %v", got, want)
	}
}

func TestOpenDirectPersistenceAndNaming(t *testing.T) {
	openDirectTempConfig(t)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)

	store := NewOpenDirectStore()
	store.Add("net-1", "Alice", mapping)
	store.Add("net-2", "Bob", mapping)

	fresh := NewOpenDirectStore()
	if got, want := fresh.Targets("net-1"), []string{"Alice"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("fresh Targets(net-1) = %v, want %v", got, want)
	}
	if got, want := fresh.Targets("net-2"), []string{"Bob"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("fresh Targets(net-2) = %v, want %v", got, want)
	}

	// The on-disk group/key layout is openDirects/<id> with targets.
	settings := OpenSettings("")
	if got, want := settings.StringList("openDirects/net-1", "targets"), []string{"Alice"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("stored StringList = %v, want %v", got, want)
	}
	if got, want := settings.ChildGroups("openDirects"), []string{"net-1", "net-2"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("ChildGroups(openDirects) = %v, want %v", got, want)
	}
	raw, err := os.ReadFile(ConfigPath())
	if err != nil {
		t.Fatalf("ReadFile(%q) = %v", ConfigPath(), err)
	}
	if !strings.Contains(string(raw), "[openDirects]") {
		t.Fatalf("config file missing [openDirects] section:\n%s", raw)
	}
}

func TestOpenDirectForget(t *testing.T) {
	openDirectTempConfig(t)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)

	store := NewOpenDirectStore()
	store.Add("net-1", "Alice", mapping)
	store.Forget("net-1")

	if got := store.Targets("net-1"); got != nil {
		t.Fatalf("Targets after Forget = %v, want nil", got)
	}
	fresh := NewOpenDirectStore()
	if got := fresh.Targets("net-1"); got != nil {
		t.Fatalf("fresh Targets after Forget = %v, want nil", got)
	}
	settings := OpenSettings("")
	if got := settings.StringList("openDirects/net-1", "targets"); got != nil {
		t.Fatalf("stored list after Forget = %v, want nil", got)
	}
}

func TestOpenDirectEphemeralNeverWrites(t *testing.T) {
	configPath := openDirectTempConfig(t)
	mapping := irc.NewCaseMapping(irc.KindRfc1459)

	store := NewOpenDirectStore()
	store.SetEphemeral(true)
	if !store.Add("net-1", "Alice", mapping) {
		t.Fatal("ephemeral Add should be true")
	}
	if got, want := store.Targets("net-1"), []string{"Alice"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("ephemeral Targets = %v, want %v", got, want)
	}

	// Nothing was written to disk.
	if _, err := os.Stat(configPath); !os.IsNotExist(err) {
		t.Fatalf("ephemeral store wrote %q (stat err = %v)", configPath, err)
	}
	fresh := NewOpenDirectStore()
	if got := fresh.Targets("net-1"); got != nil {
		t.Fatalf("disk-backed store saw ephemeral data: %v", got)
	}

	// Turning ephemeral on again clears the cache.
	store.SetEphemeral(true)
	if got := store.Targets("net-1"); got != nil {
		t.Fatalf("Targets after SetEphemeral(true) = %v, want nil", got)
	}
}
