//go:build darwin

package storage

import (
	"context"
	"errors"
	"os/exec"
	"testing"
)

func TestKeychainBackendReadWriteRemove(t *testing.T) {
	store := map[string]string{}
	backend := &keychainBackend{
		runner: func(ctx context.Context, name string, args ...string) ([]byte, int, error) {
			if name != keychainSecurityBin {
				t.Fatalf("unexpected command %q", name)
			}
			switch args[0] {
			case "find-generic-password":
				key := args[4]
				if password, ok := store[key]; ok {
					return []byte(password), 0, nil
				}
				return nil, keychainExitNotFound, nil
			case "add-generic-password":
				key := args[4]
				password := args[6]
				store[key] = password
				return nil, 0, nil
			case "delete-generic-password":
				key := args[4]
				if _, ok := store[key]; !ok {
					return nil, keychainExitNotFound, nil
				}
				delete(store, key)
				return nil, 0, nil
			default:
				t.Fatalf("unexpected security subcommand %q", args[0])
				return nil, 1, nil
			}
		},
	}

	keyName := CredentialKeyName(CredentialKey{
		NetworkID: "freenode",
		Username:  "alice",
		Host:      "chat.freenode.net",
	})

	if got := backend.read(keyName); got.State != CredentialMissing {
		t.Fatalf("read missing = %+v, want CredentialMissing", got)
	}

	if got := backend.write(keyName, "hunter2"); got.State != CredentialAvailable {
		t.Fatalf("write = %+v, want CredentialAvailable", got)
	}
	if got := backend.read(keyName); got.State != CredentialAvailable || got.Password != "hunter2" {
		t.Fatalf("read stored = %+v, want available hunter2", got)
	}

	if got := backend.remove(keyName); got.State != CredentialAvailable {
		t.Fatalf("remove = %+v, want CredentialAvailable", got)
	}
	if got := backend.read(keyName); got.State != CredentialMissing {
		t.Fatalf("read after remove = %+v, want CredentialMissing", got)
	}
	if got := backend.remove(keyName); got.State != CredentialMissing {
		t.Fatalf("remove missing = %+v, want CredentialMissing", got)
	}
}

func TestKeychainBackendUnavailableWithoutSecurity(t *testing.T) {
	backend := &keychainBackend{
		runner: func(context.Context, string, ...string) ([]byte, int, error) {
			return nil, 0, exec.ErrNotFound
		},
	}
	keyName := CredentialKeyName(CredentialKey{NetworkID: "freenode"})

	for name, got := range map[string]CredentialResult{
		"read":   backend.read(keyName),
		"write":  backend.write(keyName, "secret"),
		"remove": backend.remove(keyName),
	} {
		if got.State != CredentialUnavailable {
			t.Errorf("%s state = %v, want CredentialUnavailable", name, got.State)
		}
	}
}

func TestKeychainResultForErrorMapsNotFound(t *testing.T) {
	got := keychainResultForError(errKeychainNotFound)
	if got.State != CredentialMissing {
		t.Fatalf("state = %v, want CredentialMissing", got.State)
	}
	got = keychainResultForError(exec.ErrNotFound)
	if got.State != CredentialUnavailable {
		t.Fatalf("state = %v, want CredentialUnavailable", got.State)
	}
	if !errors.Is(exec.ErrNotFound, exec.ErrNotFound) {
		t.Fatal("sanity check failed")
	}
}
