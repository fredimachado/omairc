package storage

import (
	"runtime"
	"testing"
)

func TestCredentialKeyName(t *testing.T) {
	cases := []struct {
		name string
		key  CredentialKey
		want string
	}{
		{
			name: "plain",
			key:  CredentialKey{NetworkID: "freenode", Username: "alice", Host: "chat.freenode.net"},
			want: "omairc/v1/8:freenode/5:alice/17:chat.freenode.net",
		},
		{
			name: "purpose",
			key:  CredentialKey{NetworkID: "freenode", Username: "alice", Host: "chat.freenode.net", Purpose: "sasl"},
			want: "omairc/v1/8:freenode/5:alice/17:chat.freenode.net/4:sasl",
		},
		{
			name: "empty fields",
			key:  CredentialKey{},
			want: "omairc/v1/0:/0:/0:",
		},
		{
			name: "non-ascii uses byte length",
			key:  CredentialKey{NetworkID: "é", Username: "ü", Host: "host"},
			want: "omairc/v1/2:é/2:ü/4:host",
		},
		{
			name: "slashes survive in the value",
			key:  CredentialKey{NetworkID: "net", Username: "u", Host: "h", Purpose: "a/b"},
			want: "omairc/v1/3:net/1:u/1:h/3:a/b",
		},
	}
	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			if got := CredentialKeyName(testCase.key); got != testCase.want {
				t.Fatalf("CredentialKeyName = %q, want %q", got, testCase.want)
			}
		})
	}
}

// credentialTestBackend is a fake credentialBackend that records the key names
// the adapter derived and returns a canned result.
type credentialTestBackend struct {
	readKey   string
	writeKey  string
	writePass string
	removeKey string
	result    CredentialResult
}

func (b *credentialTestBackend) read(keyName string) CredentialResult {
	b.readKey = keyName
	return b.result
}

func (b *credentialTestBackend) write(keyName, password string) CredentialResult {
	b.writeKey = keyName
	b.writePass = password
	return b.result
}

func (b *credentialTestBackend) remove(keyName string) CredentialResult {
	b.removeKey = keyName
	return b.result
}

func TestCredentialStoreRoutesThroughBackend(t *testing.T) {
	key := CredentialKey{NetworkID: "freenode", Username: "alice", Host: "chat.freenode.net", Purpose: "sasl"}
	backend := &credentialTestBackend{result: CredentialResult{State: CredentialMissing}}
	store := &credentialStore{backend: backend}

	if got := store.Read(key); got.State != CredentialMissing {
		t.Fatalf("Read state = %v, want CredentialMissing", got.State)
	}
	if backend.readKey != CredentialKeyName(key) {
		t.Errorf("Read key name = %q, want %q", backend.readKey, CredentialKeyName(key))
	}

	backend.result = CredentialResult{State: CredentialAvailable, Password: "hunter2"}
	if got := store.Read(key); got.State != CredentialAvailable || got.Password != "hunter2" {
		t.Fatalf("Read = %+v, want available hunter2", got)
	}

	if got := store.Write(key, "swordfish"); got.State != CredentialAvailable {
		t.Fatalf("Write state = %v, want CredentialAvailable", got.State)
	}
	if backend.writeKey != CredentialKeyName(key) || backend.writePass != "swordfish" {
		t.Errorf("Write saw key=%q password=%q", backend.writeKey, backend.writePass)
	}

	if got := store.Remove(key); got.State != CredentialAvailable {
		t.Fatalf("Remove state = %v, want CredentialAvailable", got.State)
	}
	if backend.removeKey != CredentialKeyName(key) {
		t.Errorf("Remove key name = %q, want %q", backend.removeKey, CredentialKeyName(key))
	}
}

func TestCredentialStoreUnavailableStateIsNotAnError(t *testing.T) {
	backend := &credentialTestBackend{result: CredentialResult{State: CredentialUnavailable}}
	store := &credentialStore{backend: backend}
	key := CredentialKey{NetworkID: "freenode", Username: "alice", Host: "chat.freenode.net"}

	for name, got := range map[string]CredentialResult{
		"read":   store.Read(key),
		"write":  store.Write(key, "hunter2"),
		"remove": store.Remove(key),
	} {
		if got.State != CredentialUnavailable {
			t.Errorf("%s state = %v, want CredentialUnavailable", name, got.State)
		}
		if got.Message != "" {
			t.Errorf("%s message = %q, want empty", name, got.Message)
		}
		if got.Password != "" {
			t.Errorf("%s password must never be returned", name)
		}
	}
}

func TestNewCredentialStoreUnavailableWithoutSessionBus(t *testing.T) {
	if runtime.GOOS != "linux" {
		t.Skip("Secret Service is Linux-only")
	}
	// Clear the session bus address so a headless run cannot reach a secret
	// service. godbus may still discover a bus under /run/user/<uid>/bus, so a
	// busy developer desktop can legitimately skip this assertion.
	t.Setenv("DBUS_SESSION_BUS_ADDRESS", "")
	t.Setenv("DISPLAY", "")

	key := CredentialKey{NetworkID: "freenode", Username: "alice", Host: "chat.freenode.net"}
	result := NewCredentialStore().Read(key)
	if result.State != CredentialUnavailable {
		t.Skipf("a secret service answered %v without a cleared session bus; skipping", result.State)
	}
	if result.Password != "" {
		t.Fatalf("unavailable read returned a password")
	}
}
