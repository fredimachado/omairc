package storage

import "strconv"

// CredentialState mirrors CredentialStore::State. A synchronous operation only
// ever reports Available, Missing, Unavailable, Error, or SessionOnly;
// Loading is kept for parity with the asynchronous Qt API.
type CredentialState int

const (
	// CredentialLoading means an asynchronous read is still in flight. It is
	// never returned by the synchronous API.
	CredentialLoading CredentialState = iota
	// CredentialAvailable means the secret exists and was read back.
	CredentialAvailable
	// CredentialMissing means no secret is stored for the key.
	CredentialMissing
	// CredentialUnavailable means no secret backend (session bus) is available.
	CredentialUnavailable
	// CredentialError means the backend failed; Message carries the reason.
	CredentialError
	// CredentialSessionOnly means the caller must keep the secret in memory.
	CredentialSessionOnly
)

// CredentialKey identifies one stored secret. It mirrors CredentialKey.
type CredentialKey struct {
	NetworkID string
	Username  string
	Host      string
	Purpose   string
}

// CredentialResult is the outcome of a synchronous credential operation.
// Message never contains the password.
type CredentialResult struct {
	State    CredentialState
	Password string
	Message  string
}

// CredentialStore reads, writes, and removes network secrets. Implementations
// are synchronous and safe for concurrent use by the connection layer.
type CredentialStore interface {
	Read(key CredentialKey) CredentialResult
	Write(key CredentialKey, password string) CredentialResult
	Remove(key CredentialKey) CredentialResult
}

// CredentialKeyName mirrors SecretServiceCredentialStore::keyName:
// "omairc/v1/<len>:<networkId>/<len>:<username>/<len>:<host>" and, when the
// purpose is non-empty, "/<len>:<purpose>". Lengths are UTF-8 byte counts.
func CredentialKeyName(key CredentialKey) string {
	encode := func(value string) string {
		return strconv.Itoa(len([]byte(value))) + ":" + value
	}
	name := "omairc/v1/" +
		encode(key.NetworkID) + "/" +
		encode(key.Username) + "/" +
		encode(key.Host)
	if key.Purpose != "" {
		name += "/" + encode(key.Purpose)
	}
	return name
}

// credentialUnavailableResult is the shared "no backend" outcome.
func credentialUnavailableResult() CredentialResult {
	return CredentialResult{State: CredentialUnavailable}
}

// credentialBackend is the platform secret transport behind a CredentialStore.
// The Linux build wires a Secret Service client; other platforms provide an
// unavailable stub. It is unexported so the DBus specifics stay out of the
// public surface.
type credentialBackend interface {
	read(keyName string) CredentialResult
	write(keyName, password string) CredentialResult
	remove(keyName string) CredentialResult
}

// credentialStore adapts a credentialBackend to the exported CredentialStore
// interface, resolving the key name first.
type credentialStore struct {
	backend credentialBackend
}

// Read resolves the key name and asks the backend for the stored secret.
func (s *credentialStore) Read(key CredentialKey) CredentialResult {
	return s.backend.read(CredentialKeyName(key))
}

// Write resolves the key name and stores the secret.
func (s *credentialStore) Write(key CredentialKey, password string) CredentialResult {
	return s.backend.write(CredentialKeyName(key), password)
}

// Remove resolves the key name and deletes the stored secret.
func (s *credentialStore) Remove(key CredentialKey) CredentialResult {
	return s.backend.remove(CredentialKeyName(key))
}
