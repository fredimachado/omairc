//go:build !linux

package storage

// This file provides the non-Linux credential store. There is no Secret
// Service outside Linux, so every operation fails closed as unavailable. The
// macOS Keychain and Windows Credential Manager backends land in a later
// phase.
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

// NewCredentialStore returns the platform store: the Secret Service client on
// Linux, or the unavailable stub elsewhere.
func NewCredentialStore() CredentialStore {
	return unavailableCredentialStore{}
}
