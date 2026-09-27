package connection

import (
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

// fakeCredentialStore is a small in-memory storage.CredentialStore. It records
// every write and remove key so a test can assert the exact CredentialKey the
// connection addressed, and it can be flipped Unavailable to exercise the
// session-only overlay.
type fakeCredentialStore struct {
	secrets     map[string]string
	unavailable bool
	writes      []storage.CredentialKey
	removes     []storage.CredentialKey
}

func newFakeCredentialStore() *fakeCredentialStore {
	return &fakeCredentialStore{secrets: make(map[string]string)}
}

func (f *fakeCredentialStore) Read(key storage.CredentialKey) storage.CredentialResult {
	if f.unavailable {
		return storage.CredentialResult{State: storage.CredentialUnavailable}
	}
	if password, ok := f.secrets[storage.CredentialKeyName(key)]; ok {
		return storage.CredentialResult{State: storage.CredentialAvailable, Password: password}
	}
	return storage.CredentialResult{State: storage.CredentialMissing}
}

func (f *fakeCredentialStore) Write(key storage.CredentialKey, password string) storage.CredentialResult {
	if f.unavailable {
		return storage.CredentialResult{State: storage.CredentialUnavailable}
	}
	f.writes = append(f.writes, key)
	f.secrets[storage.CredentialKeyName(key)] = password
	return storage.CredentialResult{State: storage.CredentialAvailable}
}

func (f *fakeCredentialStore) Remove(key storage.CredentialKey) storage.CredentialResult {
	if f.unavailable {
		return storage.CredentialResult{State: storage.CredentialUnavailable}
	}
	f.removes = append(f.removes, key)
	name := storage.CredentialKeyName(key)
	if _, ok := f.secrets[name]; !ok {
		return storage.CredentialResult{State: storage.CredentialMissing}
	}
	delete(f.secrets, name)
	return storage.CredentialResult{State: storage.CredentialAvailable}
}

func TestApplyWritesEditedPasswordWithCredentialKey(t *testing.T) {
	fake := newFakeCredentialStore()
	conn, _, _ := newFixture(t)
	conn.SetCredentialStore(fake)
	conn.SetNick("omairc")
	conn.SetUsername("user")
	conn.SetPassword("hunter2")

	if !conn.Apply() {
		t.Fatal("Apply() = false")
	}
	id := conn.SelectedNetworkID()
	want := storage.CredentialKey{NetworkID: id, Username: "user", Host: "irc.libera.chat"}
	if len(fake.writes) != 1 || fake.writes[0] != want {
		t.Fatalf("writes = %+v, want [%+v]", fake.writes, want)
	}
	if !conn.draft.SecretSaved {
		t.Fatal("draft.SecretSaved = false after a successful write")
	}
	if stored, ok := conn.storedProfile(id); !ok || !stored.SecretSaved {
		t.Fatalf("stored.SecretSaved = %v, want true", stored.SecretSaved)
	}
	if fake.secrets[storage.CredentialKeyName(want)] != "hunter2" {
		t.Fatal("the fake did not receive the password")
	}
}

func TestApplyWritesEditedNickServWithPurpose(t *testing.T) {
	fake := newFakeCredentialStore()
	conn, _, _ := newFixture(t)
	conn.SetCredentialStore(fake)
	conn.SetNick("omairc")
	conn.SetNickServPassword("nssecret")

	if !conn.Apply() {
		t.Fatal("Apply() = false")
	}
	id := conn.SelectedNetworkID()
	want := storage.CredentialKey{NetworkID: id, Username: "omairc", Host: "irc.libera.chat", Purpose: "nickserv"}
	if len(fake.writes) != 1 || fake.writes[0] != want {
		t.Fatalf("writes = %+v, want [%+v]", fake.writes, want)
	}
	if !conn.draft.NickServSaved {
		t.Fatal("draft.NickServSaved = false after a successful write")
	}
}

func TestApplyRemovesClearedPassword(t *testing.T) {
	fake := newFakeCredentialStore()
	conn, _, _ := newFixture(t)
	conn.SetCredentialStore(fake)
	conn.SetNick("omairc")
	conn.SetPassword("hunter2")
	if !conn.Apply() {
		t.Fatal("first Apply() = false")
	}
	id := conn.SelectedNetworkID()
	key := storage.CredentialKey{NetworkID: id, Username: "omairc", Host: "irc.libera.chat"}

	// With a stored secret present the empty value is accepted and clears it.
	conn.SetPassword("")
	if !conn.Apply() {
		t.Fatal("second Apply() = false")
	}
	if conn.draft.SecretSaved {
		t.Fatal("draft.SecretSaved = true after clearing")
	}
	if len(fake.removes) != 1 || fake.removes[0] != key {
		t.Fatalf("removes = %+v, want [%+v]", fake.removes, key)
	}
	if _, ok := fake.secrets[storage.CredentialKeyName(key)]; ok {
		t.Fatal("the fake still holds the removed password")
	}
}

func TestSetPasswordRefusesEmptyWhenNoStoredSecret(t *testing.T) {
	conn, _, _ := newFixture(t)
	conn.SetCredentialStore(newFakeCredentialStore())
	conn.SetNick("omairc")
	conn.SetPassword("")
	if conn.PasswordSet() {
		t.Fatal("SetPassword(\"\") with no stored or held secret set a password")
	}
	if secret := conn.passwordSecret(conn.SelectedNetworkID()); secret.edited {
		t.Fatal("a refused clear must not mark the secret edited")
	}
}

func TestForgetPasswordRemovesKeyAndFlag(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	fake := newFakeCredentialStore()
	ctrl := controller.New()
	conn := New(ctrl, func() session.Transport { return session.NewLoopbackTransport() })
	conn.SetProfileStore(storage.NewProfileStore())
	conn.SetCredentialStore(fake)
	conn.SetNick("omairc")
	conn.SetPassword("hunter2")
	if !conn.Apply() {
		t.Fatal("Apply() = false")
	}
	id := conn.SelectedNetworkID()

	if !conn.CanForgetPassword() {
		t.Fatal("CanForgetPassword() = false after a write")
	}
	conn.ForgetPassword()
	if conn.PasswordSet() {
		t.Fatal("PasswordSet() = true after ForgetPassword")
	}
	if conn.CanForgetPassword() {
		t.Fatal("CanForgetPassword() = true after ForgetPassword")
	}
	if len(fake.removes) != 1 {
		t.Fatalf("removes = %d, want 1", len(fake.removes))
	}
	loaded := storage.NewProfileStore().Profiles()
	if len(loaded) != 1 || loaded[0].NetworkID != id {
		t.Fatalf("Profiles() = %+v", loaded)
	}
	if loaded[0].SecretSaved {
		t.Fatal("SecretSaved = true on disk after ForgetPassword")
	}
}

func TestForgetNickServRemovesKeyAndFlag(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	fake := newFakeCredentialStore()
	ctrl := controller.New()
	conn := New(ctrl, func() session.Transport { return session.NewLoopbackTransport() })
	conn.SetProfileStore(storage.NewProfileStore())
	conn.SetCredentialStore(fake)
	conn.SetNick("omairc")
	conn.SetNickServPassword("nssecret")
	if !conn.Apply() {
		t.Fatal("Apply() = false")
	}
	if !conn.CanForgetNickServ() {
		t.Fatal("CanForgetNickServ() = false after a write")
	}
	conn.ForgetNickServ()
	if conn.NickServSet() {
		t.Fatal("NickServSet() = true after ForgetNickServ")
	}
	if len(fake.removes) != 1 || fake.removes[0].Purpose != "nickserv" {
		t.Fatalf("removes = %+v, want one nickserv key", fake.removes)
	}
	loaded := storage.NewProfileStore().Profiles()
	if len(loaded) != 1 || loaded[0].NickServSaved {
		t.Fatalf("NickServSaved = true on disk after ForgetNickServ: %+v", loaded)
	}
}

func TestCredentialsUnavailableOverlaysSessionOnly(t *testing.T) {
	fake := newFakeCredentialStore()
	fake.unavailable = true
	conn, _, _ := newFixture(t)
	profile := completeProfile("net-1", "irc.example")
	profile.SecretSaved = true
	conn.SetStoredProfiles([]NetworkProfile{profile})
	conn.SetCredentialStore(fake)

	if got := conn.CredentialState(); got != storage.CredentialUnavailable {
		t.Fatalf("CredentialState() = %v, want Unavailable", got)
	}
	conn.SetPassword("hunter2")
	if got := conn.CredentialState(); got != storage.CredentialSessionOnly {
		t.Fatalf("CredentialState() = %v, want SessionOnly", got)
	}
	if got := conn.CredentialStatus(); got != "secure storage unavailable; password is session-only" {
		t.Fatalf("CredentialStatus() = %q", got)
	}
}

func TestCredentialStatusCombinedSessionOnlySentence(t *testing.T) {
	fake := newFakeCredentialStore()
	fake.unavailable = true
	conn, _, _ := newFixture(t)
	profile := completeProfile("net-1", "irc.example")
	profile.SecretSaved = true
	profile.NickServSaved = true
	conn.SetStoredProfiles([]NetworkProfile{profile})
	conn.SetCredentialStore(fake)

	conn.SetPassword("hunter2")
	conn.SetNickServPassword("nssecret")
	want := "secure storage unavailable; password and NickServ are session-only"
	if got := conn.CredentialStatus(); got != want {
		t.Fatalf("CredentialStatus() = %q, want %q", got, want)
	}
}

func TestCredentialStatusEditedAndSavedSentences(t *testing.T) {
	fake := newFakeCredentialStore()
	conn, _, _ := newFixture(t)
	conn.SetCredentialStore(fake)
	conn.SetNick("omairc")

	conn.SetPassword("hunter2")
	if got := conn.CredentialStatus(); got != "password changed; apply to save securely" {
		t.Fatalf("edited status = %q", got)
	}
	if !conn.Apply() {
		t.Fatal("Apply() = false")
	}
	if got := conn.CredentialStatus(); got != "password saved securely" {
		t.Fatalf("saved status = %q", got)
	}
}

func TestSetCredentialStoreReadsDeclaredSecrets(t *testing.T) {
	fake := newFakeCredentialStore()
	profile := completeProfile("net-1", "irc.example")
	profile.Username = "user"
	fake.secrets[storage.CredentialKeyName(storage.CredentialKey{
		NetworkID: "net-1", Username: "user", Host: "irc.example",
	})] = "stored"
	profile.SecretSaved = true

	conn, _, _ := newFixture(t)
	conn.SetStoredProfiles([]NetworkProfile{profile})
	conn.SetCredentialStore(fake)

	if !conn.PasswordSet() {
		t.Fatal("PasswordSet() = false after a readable stored secret")
	}
	if got := conn.CredentialState(); got != storage.CredentialAvailable {
		t.Fatalf("CredentialState() = %v, want Available", got)
	}
	if got := conn.CredentialStatus(); got != "password saved securely" {
		t.Fatalf("CredentialStatus() = %q", got)
	}
}
