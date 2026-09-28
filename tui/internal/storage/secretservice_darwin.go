//go:build darwin

package storage

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os/exec"
	"strings"
	"time"
)

// This file implements the macOS Keychain credential backend through the
// security CLI. It mirrors QtKeychain's ReadPasswordJob("omairc"): the
// service name is omairc and the key name is stored in the account attribute.
//
// Every call is synchronous and bounded by keychainCallTimeout so a hung
// security subprocess can never block the terminal indefinitely.

const (
	keychainServiceName   = "omairc"
	keychainCallTimeout   = 5 * time.Second
	keychainExitNotFound  = 44
	keychainSecurityBin   = "security"
)

var (
	errKeychainNotFound = errors.New("keychain: item not found")
	errKeychainEmpty    = errors.New("keychain: empty password")
)

type keychainRunner func(ctx context.Context, name string, args ...string) (stdout []byte, exitCode int, err error)

// keychainBackend talks to the login keychain through /usr/bin/security. The
// runner field is a seam so tests can substitute a fake subprocess.
type keychainBackend struct {
	runner keychainRunner
}

func newKeychainBackend() *keychainBackend {
	return &keychainBackend{runner: runSecurityCommand}
}

func (b *keychainBackend) read(keyName string) CredentialResult {
	password, err := b.findPassword(keyName)
	if err != nil {
		return keychainResultForError(err)
	}
	if password == "" {
		return CredentialResult{State: CredentialMissing}
	}
	return CredentialResult{State: CredentialAvailable, Password: password}
}

func (b *keychainBackend) write(keyName, password string) CredentialResult {
	if password == "" {
		return CredentialResult{State: CredentialError, Message: "keychain: empty password"}
	}
	ctx, cancel := context.WithTimeout(context.Background(), keychainCallTimeout)
	defer cancel()

	_, exitCode, err := b.runner(ctx, keychainSecurityBin,
		"add-generic-password",
		"-s", keychainServiceName,
		"-a", keyName,
		"-w", password,
		"-U",
	)
	if err != nil {
		return keychainResultForError(err)
	}
	if exitCode != 0 {
		return keychainResultForExitCode(exitCode, "keychain: write failed")
	}
	return CredentialResult{State: CredentialAvailable}
}

func (b *keychainBackend) remove(keyName string) CredentialResult {
	ctx, cancel := context.WithTimeout(context.Background(), keychainCallTimeout)
	defer cancel()

	_, exitCode, err := b.runner(ctx, keychainSecurityBin,
		"delete-generic-password",
		"-s", keychainServiceName,
		"-a", keyName,
	)
	if err != nil {
		return keychainResultForError(err)
	}
	switch exitCode {
	case 0:
		return CredentialResult{State: CredentialAvailable}
	case keychainExitNotFound:
		return CredentialResult{State: CredentialMissing}
	default:
		return keychainResultForExitCode(exitCode, "keychain: delete failed")
	}
}

func (b *keychainBackend) findPassword(keyName string) (string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), keychainCallTimeout)
	defer cancel()

	stdout, exitCode, err := b.runner(ctx, keychainSecurityBin,
		"find-generic-password",
		"-s", keychainServiceName,
		"-a", keyName,
		"-w",
	)
	if err != nil {
		return "", err
	}
	switch exitCode {
	case 0:
		return strings.TrimSpace(string(stdout)), nil
	case keychainExitNotFound:
		return "", errKeychainNotFound
	default:
		return "", fmt.Errorf("keychain: read failed with exit status %d", exitCode)
	}
}

func runSecurityCommand(ctx context.Context, name string, args ...string) ([]byte, int, error) {
	cmd := exec.CommandContext(ctx, name, args...)
	var stdout bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = nil
	if err := cmd.Run(); err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			return stdout.Bytes(), exitErr.ExitCode(), nil
		}
		if errors.Is(err, exec.ErrNotFound) {
			return nil, 0, err
		}
		return nil, 0, err
	}
	return stdout.Bytes(), 0, nil
}

func keychainResultForError(err error) CredentialResult {
	switch {
	case err == nil:
		return CredentialResult{State: CredentialError}
	case errors.Is(err, errKeychainNotFound):
		return CredentialResult{State: CredentialMissing}
	case errors.Is(err, exec.ErrNotFound):
		return credentialUnavailableResult()
	}
	return CredentialResult{State: CredentialError, Message: err.Error()}
}

func keychainResultForExitCode(exitCode int, message string) CredentialResult {
	if exitCode == keychainExitNotFound {
		return CredentialResult{State: CredentialMissing}
	}
	return CredentialResult{State: CredentialError, Message: fmt.Sprintf("%s (exit %d)", message, exitCode)}
}

// NewCredentialStore returns the macOS Keychain store.
func NewCredentialStore() CredentialStore {
	return &credentialStore{backend: newKeychainBackend()}
}
