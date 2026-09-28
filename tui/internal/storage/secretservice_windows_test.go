//go:build windows

package storage

import (
	"testing"

	"github.com/danieljoos/wincred"
)

func TestWincredRoundTrip(t *testing.T) {
	backend := newWincredBackend()
	key := "omairc/v1/test:" + t.Name()

	if got := backend.remove(key); got.State != CredentialMissing {
		t.Fatalf("initial remove state = %v, want CredentialMissing", got.State)
	}

	if got := backend.read(key); got.State != CredentialMissing {
		t.Fatalf("initial read state = %v, want CredentialMissing", got.State)
	}

	if got := backend.write(key, "hunter2"); got.State != CredentialAvailable {
		t.Fatalf("write state = %v, want CredentialAvailable (%s)", got.State, got.Message)
	}
	t.Cleanup(func() { backend.remove(key) })

	if got := backend.read(key); got.State != CredentialAvailable || got.Password != "hunter2" {
		t.Fatalf("read = %+v, want available hunter2", got)
	}

	if got := backend.remove(key); got.State != CredentialAvailable {
		t.Fatalf("remove state = %v, want CredentialAvailable", got.State)
	}

	if got := backend.read(key); got.State != CredentialMissing {
		t.Fatalf("post-remove read state = %v, want CredentialMissing", got.State)
	}
}

func TestWincredCompatBareKeyRead(t *testing.T) {
	backend := newWincredBackend()
	key := "omairc/v1/compat:" + t.Name()

	cred := wincred.NewGenericCredential(key)
	cred.CredentialBlob = []byte("legacy")
	cred.Persist = wincred.PersistEnterprise
	if err := cred.Write(); err != nil {
		t.Fatalf("write bare entry: %v", err)
	}
	t.Cleanup(func() { _ = cred.Delete() })

	if got := backend.read(key); got.State != CredentialAvailable || got.Password != "legacy" {
		t.Fatalf("compat read = %+v, want available legacy", got)
	}
}
