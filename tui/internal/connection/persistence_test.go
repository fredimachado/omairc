package connection

import (
	"os"
	"path/filepath"
	"slices"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

// fullProfile is a complete profile that exercises every persisted field.
func fullProfile() NetworkProfile {
	return NetworkProfile{
		NetworkID:        "net-round-trip",
		Name:             "libera",
		Host:             "irc.libera.chat",
		Port:             7000,
		TLSEnabled:       true,
		ConnectOnStartup: true,
		Nick:             "omairc",
		Username:         "user",
		Realname:         "Real Name",
		Account:          "acct",
		BouncerNetwork:   "bnc",
		AutojoinChannels: []string{"#one", "#two"},
		AutojoinKeys:     map[string]string{"#one": "key"},
		IconColor:        3,
		AvatarURL:        "https://example.invalid/avatar.png",
	}
}

func TestApplyPersistsProfileRoundTrip(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	ctrl := controller.New()
	conn := New(ctrl, func() session.Transport { return session.NewLoopbackTransport() })
	conn.SetProfileStore(storage.NewProfileStore())
	conn.SetStoredProfiles([]NetworkProfile{fullProfile()})
	if !conn.Apply() {
		t.Fatal("Apply() = false")
	}

	loaded := storage.NewProfileStore().Profiles()
	if len(loaded) != 1 {
		t.Fatalf("Profiles() = %+v, want one", loaded)
	}
	got := loaded[0]
	want := fullProfile()
	if got.NetworkID != want.NetworkID || got.Name != want.Name || got.Host != want.Host ||
		got.Port != want.Port || got.TLSEnabled != want.TLSEnabled ||
		got.ConnectOnStartup != want.ConnectOnStartup || got.Nick != want.Nick ||
		got.Username != want.Username || got.Realname != want.Realname ||
		got.Account != want.Account || got.BouncerNetwork != want.BouncerNetwork ||
		got.IconColor != want.IconColor || got.AvatarURL != want.AvatarURL {
		t.Fatalf("round-tripped profile = %+v, want %+v", got, want)
	}
	if !slices.Equal(got.AutojoinChannels, want.AutojoinChannels) {
		t.Fatalf("AutojoinChannels = %v, want %v", got.AutojoinChannels, want.AutojoinChannels)
	}
	if got.AutojoinKeys["#one"] != "key" {
		t.Fatalf("AutojoinKeys = %v, want #one:key", got.AutojoinKeys)
	}
}

func TestProfileStoreSeedsStoredProfiles(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	store := storage.NewProfileStore()
	persisted := fullProfile()
	persisted.SecretSaved = true
	if status := store.Save(storageProfile(persisted)); status != storage.StatusWritten {
		t.Fatalf("Save() = %v, want Written", status)
	}

	ctrl := controller.New()
	conn := New(ctrl, nil)
	conn.SetProfileStore(store)
	if got := conn.SelectedNetworkID(); got != persisted.NetworkID {
		t.Fatalf("selected = %q, want %q", got, persisted.NetworkID)
	}
	if got := conn.Host(); got != persisted.Host {
		t.Fatalf("Host() = %q, want %q", got, persisted.Host)
	}
	if !conn.CanForgetPassword() {
		t.Fatal("a profile with SecretSaved must be forgettable after seeding")
	}
}

func TestPersistenceStatusRecordsReadOnlyFailure(t *testing.T) {
	if os.Geteuid() == 0 {
		t.Skip("root ignores permission bits")
	}
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	path := storage.ConfigPath()
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("[General]\nfoo=bar\n"), 0o444); err != nil {
		t.Fatal(err)
	}

	ctrl := controller.New()
	conn := New(ctrl, nil)
	conn.SetProfileStore(storage.NewProfileStore())
	conn.SetNick("omairc")
	if conn.Apply() {
		t.Fatal("Apply() with a read-only settings file = true")
	}
	if got := conn.PersistenceStatus(); got != "The settings file could not be written." {
		t.Fatalf("PersistenceStatus() = %q", got)
	}
}

func TestRecordApplyPersistenceSentences(t *testing.T) {
	conn, _, _ := newFixture(t)
	id := conn.SelectedNetworkID()

	conn.recordApplyPersistence(id, storage.StatusWritten, true)
	if got := conn.PersistenceStatus(); got != "" {
		t.Fatalf("accepted save sentence = %q, want empty", got)
	}
	conn.recordApplyPersistence(id, storage.StatusWritten, false)
	if got := conn.PersistenceStatus(); got != "These settings were saved successfully." {
		t.Fatalf("unaccepted save sentence = %q", got)
	}
	conn.recordApplyPersistence(id, storage.StatusWritten, true)
	if got := conn.PersistenceStatus(); got != "" {
		t.Fatalf("accepted save did not clear the sentence: %q", got)
	}
	conn.recordApplyPersistence(id, storage.StatusAccessError, false)
	if got := conn.PersistenceStatus(); got != "The settings file could not be written." {
		t.Fatalf("access-error sentence = %q", got)
	}
	conn.recordApplyPersistence(id, storage.StatusAccessError, true)
	if got := conn.PersistenceStatus(); got != "Connected using these settings. The settings file could not be written." {
		t.Fatalf("connected access-error sentence = %q", got)
	}
}

func TestActivateStartupStartsReadableProfiles(t *testing.T) {
	fake := newFakeCredentialStore()
	fake.secrets[storage.CredentialKeyName(storage.CredentialKey{
		NetworkID: "net-ok", Username: "omairc", Host: "irc.ok.example",
	})] = "stored"

	ctrl := controller.New()
	factory := &loopbackFactory{}
	conn := New(ctrl, factory.newTransport)

	readable := completeProfile("net-ok", "irc.ok.example")
	readable.ConnectOnStartup = true
	readable.SecretSaved = true
	missing := completeProfile("net-bad", "irc.bad.example")
	missing.ConnectOnStartup = true
	missing.SecretSaved = true
	idle := completeProfile("net-idle", "irc.idle.example")
	idle.ConnectOnStartup = false
	idle.SecretSaved = true

	conn.SetStoredProfiles([]NetworkProfile{readable, missing, idle})
	conn.SetCredentialStore(fake)

	if got := conn.ActivateStartup(); got != 1 {
		t.Fatalf("ActivateStartup() = %d, want 1", got)
	}
	if ctrl.Session("net-ok") == nil {
		t.Fatal("the readable startup profile did not start")
	}
	if ctrl.Session("net-bad") != nil {
		t.Fatal("a profile with a missing declared secret must be skipped")
	}
	if ctrl.Session("net-idle") != nil {
		t.Fatal("a profile without ConnectOnStartup must be skipped")
	}
}

func TestActivateStartupWithoutStoresIsSafe(t *testing.T) {
	conn, ctrl, _ := newFixture(t)
	open := completeProfile("net-open", "irc.open.example")
	open.ConnectOnStartup = true
	declared := completeProfile("net-declared", "irc.declared.example")
	declared.ConnectOnStartup = true
	declared.SecretSaved = true
	conn.SetStoredProfiles([]NetworkProfile{open, declared})

	// With no credential store there is nothing to read, so a profile that
	// declares a saved secret cannot be confirmed and is skipped, while one
	// that declares none connects with the held (empty) secrets.
	if got := conn.ActivateStartup(); got != 1 {
		t.Fatalf("ActivateStartup() = %d with no credential store, want 1", got)
	}
	if ctrl.Session("net-open") == nil {
		t.Fatal("a profile with no declared secret did not start")
	}
	if ctrl.Session("net-declared") != nil {
		t.Fatal("an unconfirmable declared secret must be skipped")
	}
}

func TestPreferencesDelegateToController(t *testing.T) {
	ctrl := controller.New()
	conn := New(ctrl, nil)
	if !conn.ReopenDirects() || !conn.ShowAvatars() || !conn.OpenAtUnread() {
		t.Fatal("preferences must default on")
	}
	conn.SetReopenDirects(false)
	if ctrl.ReopenDirects() {
		t.Fatal("SetReopenDirects(false) did not flip the controller preference")
	}
	if conn.ReopenDirects() {
		t.Fatal("ReopenDirects() did not read through to the controller")
	}
	conn.SetShowAvatars(false)
	if ctrl.ShowAvatars() {
		t.Fatal("SetShowAvatars(false) did not flip the controller preference")
	}
	conn.SetOpenAtUnread(false)
	if ctrl.OpenAtUnread() {
		t.Fatal("SetOpenAtUnread(false) did not flip the controller preference")
	}
}

func TestPreferencesFireDraftChanged(t *testing.T) {
	ctrl := controller.New()
	conn := New(ctrl, nil)
	calls := 0
	conn.OnDraftChanged = func() { calls++ }
	conn.SetReopenDirects(false)
	if calls == 0 {
		t.Fatal("SetReopenDirects did not fire OnDraftChanged")
	}
}

func TestPersistAvatarAndAutojoinSurviveReload(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	ctrl := controller.New()
	conn := New(ctrl, nil)
	conn.SetProfileStore(storage.NewProfileStore())
	conn.SetStoredProfiles([]NetworkProfile{completeProfile("net-1", "irc.example")})

	conn.PersistAvatarURL("net-1", "https://example.invalid/avatar.png")
	conn.PersistAutojoin("net-1", []string{"#one", "#two"}, map[string]string{"#one": "key"})

	if got := conn.StoredAvatarURL("net-1"); got != "https://example.invalid/avatar.png" {
		t.Fatalf("StoredAvatarURL() = %q", got)
	}
	loaded := storage.NewProfileStore().Profiles()
	if len(loaded) != 1 {
		t.Fatalf("Profiles() = %+v, want one", loaded)
	}
	if loaded[0].AvatarURL != "https://example.invalid/avatar.png" {
		t.Fatalf("reloaded AvatarURL = %q", loaded[0].AvatarURL)
	}
	if !slices.Equal(loaded[0].AutojoinChannels, []string{"#one", "#two"}) {
		t.Fatalf("reloaded AutojoinChannels = %v", loaded[0].AutojoinChannels)
	}
	if loaded[0].AutojoinKeys["#one"] != "key" {
		t.Fatalf("reloaded AutojoinKeys = %v", loaded[0].AutojoinKeys)
	}
}

func TestPersistAvatarAndAutojoinRecordBackgroundFailure(t *testing.T) {
	if os.Geteuid() == 0 {
		t.Skip("root ignores permission bits")
	}
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	path := storage.ConfigPath()
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("[General]\nfoo=bar\n"), 0o444); err != nil {
		t.Fatal(err)
	}

	ctrl := controller.New()
	conn := New(ctrl, nil)
	conn.SetProfileStore(storage.NewProfileStore())
	conn.SetStoredProfiles([]NetworkProfile{completeProfile("net-1", "irc.example")})
	conn.PersistAvatarURL("net-1", "https://example.invalid/avatar.png")
	if got := conn.PersistenceStatus(); got != "The settings file could not be written." {
		t.Fatalf("background failure sentence = %q", got)
	}
}

func TestPersistenceAndCredentialCallbacksFire(t *testing.T) {
	fake := newFakeCredentialStore()
	conn, _, _ := newFixture(t)
	conn.SetCredentialStore(fake)
	conn.SetNick("omairc")

	persistenceCalls, credentialCalls := 0, 0
	conn.OnPersistenceChanged = func() { persistenceCalls++ }
	conn.OnCredentialChanged = func() { credentialCalls++ }

	conn.assignPersistenceStatus(conn.SelectedNetworkID(), "boom")
	if persistenceCalls == 0 {
		t.Fatal("assignPersistenceStatus did not fire OnPersistenceChanged")
	}
	conn.SetPassword("hunter2")
	if credentialCalls == 0 {
		t.Fatal("SetPassword did not fire OnCredentialChanged")
	}
}

func TestEphemeralSkipsDiskAndBackend(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	ctrl := controller.New()
	conn := New(ctrl, nil)
	conn.SetProfileStore(storage.NewProfileStore())
	fake := newFakeCredentialStore()
	conn.SetCredentialStore(fake)

	conn.SetEphemeral(true)
	conn.SetNick("omairc")
	conn.SetPassword("hunter2")
	conn.Apply()

	if len(fake.writes) != 0 || len(fake.removes) != 0 {
		t.Fatalf("ephemeral Apply touched the credential store: writes=%v removes=%v", fake.writes, fake.removes)
	}
	if _, err := os.Stat(storage.ConfigPath()); !os.IsNotExist(err) {
		t.Fatalf("ephemeral Apply wrote %s (err=%v)", storage.ConfigPath(), err)
	}
	if conn.CredentialState() != storage.CredentialSessionOnly {
		t.Fatalf("CredentialState() = %v, want SessionOnly", conn.CredentialState())
	}
}
