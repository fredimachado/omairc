//go:build !linux && !darwin

package storage

// This file provides the credential store on platforms without a secret
// backend. Every operation fails closed as unavailable.
type unavailableCredentialStore struct{}

// Read reports that no credential backend is available.
func (unavailableCredentialStore) Read(CredentialKey) CredentialResult {
	return CredentialResult{State: CredentialUnavailable}
}

// Write reports that no credential backend is available.
func (unavailableCredentialStore) Write(CredentialKey, string) CredentialResult {
	return CredentialResult{State: CredentialUnavailable}
}

// Remove reports that no credential backend is available.
func (unavailableCredentialStore) Remove(CredentialKey) CredentialResult {
	return CredentialResult{State: CredentialUnavailable}
}

// NewCredentialStore returns the unavailable stub on platforms without a
// credential backend.
func NewCredentialStore() CredentialStore {
	return unavailableCredentialStore{}
}
