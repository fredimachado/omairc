//go:build windows

package storage

import (
	"errors"

	"github.com/danieljoos/wincred"
)

// windowsCredentialService mirrors QtKeychain's ReadPasswordJob("omairc").
const windowsCredentialService = "omairc"

// wincredBackend stores secrets in the Windows Credential Manager using the
// same target naming as QtKeychain 0.17+: key@service, with a read fallback to
// the bare key for older entries.
type wincredBackend struct{}

func newWincredBackend() *wincredBackend { return &wincredBackend{} }

// windowsCredentialTarget mirrors QtKeychain's targetName(service, key).
func windowsCredentialTarget(keyName string) string {
	if keyName == "" {
		return windowsCredentialService
	}
	return keyName + "@" + windowsCredentialService
}

func (b *wincredBackend) read(keyName string) CredentialResult {
	password, found, err := readWindowsCredential(keyName)
	if err != nil {
		return CredentialResult{State: CredentialError, Message: err.Error()}
	}
	if !found || password == "" {
		return CredentialResult{State: CredentialMissing}
	}
	return CredentialResult{State: CredentialAvailable, Password: password}
}

func (b *wincredBackend) write(keyName, password string) CredentialResult {
	if password == "" {
		return CredentialResult{State: CredentialError, Message: "credential manager: empty password"}
	}
	if err := writeWindowsCredential(keyName, password); err != nil {
		return CredentialResult{State: CredentialError, Message: err.Error()}
	}
	return CredentialResult{State: CredentialAvailable}
}

func (b *wincredBackend) remove(keyName string) CredentialResult {
	removed, err := removeWindowsCredential(keyName)
	if err != nil {
		return CredentialResult{State: CredentialError, Message: err.Error()}
	}
	if !removed {
		return CredentialResult{State: CredentialMissing}
	}
	return CredentialResult{State: CredentialAvailable}
}

func readWindowsCredential(keyName string) (password string, found bool, err error) {
	for _, target := range []string{windowsCredentialTarget(keyName), keyName} {
		cred, readErr := wincred.GetGenericCredential(target)
		if readErr != nil {
			if errors.Is(readErr, wincred.ErrElementNotFound) {
				continue
			}
			return "", false, readErr
		}
		if cred == nil || len(cred.CredentialBlob) == 0 {
			continue
		}
		return string(cred.CredentialBlob), true, nil
	}
	return "", false, nil
}

func writeWindowsCredential(keyName, password string) error {
	cred := wincred.NewGenericCredential(windowsCredentialTarget(keyName))
	cred.CredentialBlob = []byte(password)
	cred.Persist = wincred.PersistEnterprise
	return cred.Write()
}

func removeWindowsCredential(keyName string) (bool, error) {
	removed := false
	for _, target := range []string{windowsCredentialTarget(keyName), keyName} {
		cred := wincred.NewGenericCredential(target)
		if err := cred.Delete(); err != nil {
			if errors.Is(err, wincred.ErrElementNotFound) {
				continue
			}
			return removed, err
		}
		removed = true
	}
	return removed, nil
}

// NewCredentialStore returns the Windows Credential Manager store.
func NewCredentialStore() CredentialStore {
	return &credentialStore{backend: newWincredBackend()}
}
