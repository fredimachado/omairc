package storage

import (
	"bytes"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

// profileStoreWriteIni writes a hand-authored ini at the config path implied by
// the test's XDG_CONFIG_HOME and returns that path.
func profileStoreWriteIni(t *testing.T, contents string) string {
	t.Helper()
	path := ConfigPath()
	if path == "" {
		t.Fatal("ConfigPath() is empty")
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatalf("MkdirAll(%q): %v", filepath.Dir(path), err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o600); err != nil {
		t.Fatalf("WriteFile(%q): %v", path, err)
	}
	return path
}

func profileStoreReadFile(t *testing.T, path string) []byte {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile(%q): %v", path, err)
	}
	return raw
}

func profileStoreLineContaining(t *testing.T, raw, needle string) string {
	t.Helper()
	for _, line := range strings.Split(raw, "\n") {
		if strings.Contains(line, needle) {
			return line
		}
	}
	t.Fatalf("no line containing %q in:\n%s", needle, raw)
	return ""
}

func TestProfileStoreRoundTrip(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	want := Profile{
		NetworkID:        "libera",
		Name:             "Libera Chat",
		Host:             "irc.libera.chat",
		Port:             7000,
		TLSEnabled:       true,
		ConnectOnStartup: true,
		SecretSaved:      true,
		NickServSaved:    true,
		Nick:             "fredi",
		Username:         "fredi",
		Realname:         "Fredi Machado",
		Account:          "fredi",
		BouncerNetwork:   "libera",
		AutojoinChannels: []string{"#a", "#b"},
		AutojoinKeys:     map[string]string{"#a": "key-a", "#b": "key-b"},
		IconColor:        3,
		AvatarURL:        "https://example.com/avatar.png",
	}

	store := NewProfileStore()
	if status := store.Save(want); status != StatusWritten {
		t.Fatalf("Save() = %v, want StatusWritten", status)
	}

	got := store.Profiles()
	if len(got) != 1 {
		t.Fatalf("Profiles() len = %d, want 1 (%#v)", len(got), got)
	}
	if !reflect.DeepEqual(got[0], want) {
		t.Errorf("Profiles()[0] =\n%#v\nwant\n%#v", got[0], want)
	}
}

func TestProfileStoreDefaults(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	profileStoreWriteIni(t, "[networks]\nplain\\host=irc.example.org\n")

	got := NewProfileStore().Profiles()
	if len(got) != 1 {
		t.Fatalf("Profiles() len = %d, want 1 (%#v)", len(got), got)
	}
	profile := got[0]
	if profile.NetworkID != "plain" {
		t.Errorf("NetworkID = %q, want %q", profile.NetworkID, "plain")
	}
	if profile.Host != "irc.example.org" {
		t.Errorf("Host = %q, want %q", profile.Host, "irc.example.org")
	}
	if profile.Name != profile.Host {
		t.Errorf("Name = %q, want the host %q", profile.Name, profile.Host)
	}
	if profile.Port != 6697 {
		t.Errorf("Port = %d, want 6697", profile.Port)
	}
	if !profile.TLSEnabled {
		t.Error("TLSEnabled = false, want true")
	}
	if profile.ConnectOnStartup {
		t.Error("ConnectOnStartup = true, want false")
	}
	if profile.SecretSaved {
		t.Error("SecretSaved = true, want false")
	}
	if profile.NickServSaved {
		t.Error("NickServSaved = true, want false")
	}
	if profile.IconColor != NoIconColor {
		t.Errorf("IconColor = %d, want %d", profile.IconColor, NoIconColor)
	}
	if len(profile.AutojoinChannels) != 0 {
		t.Errorf("AutojoinChannels = %v, want empty", profile.AutojoinChannels)
	}
	if len(profile.AutojoinKeys) != 0 {
		t.Errorf("AutojoinKeys = %v, want empty", profile.AutojoinKeys)
	}
}

func TestProfileStoreLoadAutojoinKeys(t *testing.T) {
	t.Run("length mismatch yields no keys", func(t *testing.T) {
		got := profileStoreLoadAutojoinKeys([]string{"#a"}, []string{"#a"}, []string{"k", "k2"})
		if len(got) != 0 {
			t.Errorf("keys = %v, want empty", got)
		}
	})

	t.Run("case insensitive match", func(t *testing.T) {
		got := profileStoreLoadAutojoinKeys([]string{"#Chan"}, []string{"#chan"}, []string{"secret"})
		want := map[string]string{"#Chan": "secret"}
		if !reflect.DeepEqual(got, want) {
			t.Errorf("keys = %v, want %v", got, want)
		}
	})

	t.Run("empty name and value skipped", func(t *testing.T) {
		got := profileStoreLoadAutojoinKeys([]string{"#a"}, []string{"", "#a"}, []string{"k", ""})
		if len(got) != 0 {
			t.Errorf("keys = %v, want empty", got)
		}
	})

	t.Run("first matching index wins", func(t *testing.T) {
		got := profileStoreLoadAutojoinKeys([]string{"#a"}, []string{"#a", "#a"}, []string{"first", "second"})
		want := map[string]string{"#a": "first"}
		if !reflect.DeepEqual(got, want) {
			t.Errorf("keys = %v, want %v", got, want)
		}
	})
}

func TestProfileStoreRemove(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	store := NewProfileStore()
	if status := store.Save(Profile{NetworkID: "libera", Host: "irc.libera.chat"}); status != StatusWritten {
		t.Fatalf("Save(libera) = %v, want StatusWritten", status)
	}
	if status := store.Save(Profile{NetworkID: "oftc", Host: "irc.oftc.net"}); status != StatusWritten {
		t.Fatalf("Save(oftc) = %v, want StatusWritten", status)
	}

	if status := store.Remove("libera"); status != StatusWritten {
		t.Fatalf("Remove(libera) = %v, want StatusWritten", status)
	}
	got := store.Profiles()
	if len(got) != 1 || got[0].NetworkID != "oftc" {
		t.Fatalf("Profiles() after remove = %#v, want only oftc", got)
	}

	if status := store.Remove("missing"); status != StatusAbsent {
		t.Errorf("Remove(missing) = %v, want StatusAbsent", status)
	}
	if status := store.Remove(""); status != StatusAbsent {
		t.Errorf("Remove(\"\") = %v, want StatusAbsent", status)
	}
}

func TestProfileStoreRemoveMissingFile(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	if status := NewProfileStore().Remove("libera"); status != StatusAbsent {
		t.Errorf("Remove on missing file = %v, want StatusAbsent", status)
	}
	if _, err := os.Stat(ConfigPath()); !os.IsNotExist(err) {
		t.Errorf("Remove created the config file: Stat err = %v", err)
	}
}

func TestProfileStoreFailClosedMalformed(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	path := profileStoreWriteIni(t,
		"[networks]\nlibera\\host=irc.libera.chat\nnot a key line\n")
	before := profileStoreReadFile(t, path)

	status := NewProfileStore().Save(Profile{NetworkID: "libera", Host: "irc.libera.chat"})
	if status != StatusFormatError {
		t.Fatalf("Save on malformed file = %v, want StatusFormatError", status)
	}
	if after := profileStoreReadFile(t, path); !bytes.Equal(before, after) {
		t.Errorf("malformed file was rewritten:\nbefore %q\nafter  %q", before, after)
	}
}

func TestProfileStoreFailClosedReadOnly(t *testing.T) {
	if os.Geteuid() == 0 {
		t.Skip("running as root: permission bits are not enforced")
	}
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	path := profileStoreWriteIni(t, "[networks]\nlibera\\host=irc.libera.chat\n")
	if err := os.Chmod(path, 0o444); err != nil {
		t.Fatalf("Chmod(%q): %v", path, err)
	}
	before := profileStoreReadFile(t, path)

	status := NewProfileStore().Save(Profile{NetworkID: "libera", Host: "other.example.org"})
	if status != StatusAccessError {
		t.Fatalf("Save on read-only file = %v, want StatusAccessError", status)
	}
	if after := profileStoreReadFile(t, path); !bytes.Equal(before, after) {
		t.Errorf("read-only file was rewritten:\nbefore %q\nafter  %q", before, after)
	}
}

func TestProfileStoreIconColorOutOfRange(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	profileStoreWriteIni(t, strings.Join([]string{
		"[networks]",
		"high\\host=high.example.org",
		"high\\iconColor=9",
		"low\\host=low.example.org",
		"low\\iconColor=-1",
		"junk\\host=junk.example.org",
		"junk\\iconColor=abc",
		"ok\\host=ok.example.org",
		"ok\\iconColor=4",
	}, "\n")+"\n")

	byID := make(map[string]int)
	for _, profile := range NewProfileStore().Profiles() {
		byID[profile.NetworkID] = profile.IconColor
	}
	want := map[string]int{
		"high": NoIconColor,
		"low":  NoIconColor,
		"junk": NoIconColor,
		"ok":   4,
	}
	if !reflect.DeepEqual(byID, want) {
		t.Errorf("icon colors = %v, want %v", byID, want)
	}
}

func TestProfileStoreAutojoinKeyOrder(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	store := NewProfileStore()
	profile := Profile{
		NetworkID:        "libera",
		Host:             "irc.libera.chat",
		AutojoinChannels: []string{"#z", "#a", "#m"},
		AutojoinKeys:     map[string]string{"#z": "kz", "#a": "ka", "#m": "km"},
	}
	if status := store.Save(profile); status != StatusWritten {
		t.Fatalf("Save() = %v, want StatusWritten", status)
	}

	raw := string(profileStoreReadFile(t, ConfigPath()))
	channels := profileStoreLineContaining(t, raw, "autojoinKeyChannels")
	if want := `libera\autojoinKeyChannels=#a, #m, #z`; channels != want {
		t.Errorf("channels line = %q, want %q", channels, want)
	}
	values := profileStoreLineContaining(t, raw, "autojoinKeyValues")
	if want := `libera\autojoinKeyValues=ka, km, kz`; values != want {
		t.Errorf("values line = %q, want %q", values, want)
	}
}
