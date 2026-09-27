//go:build linux

package storage

import (
	"context"
	"errors"
	"sync"
	"time"

	"github.com/godbus/dbus/v5"
)

// This file implements the Secret Service credential backend over the session
// bus. It mirrors the intent of SecretServiceCredentialStore, which uses
// QtKeychain's ReadPasswordJob("omairc"): the service name is the attribute
// service=omairc on items in the user's collection. QtKeychain stores the key
// name in the username attribute, so a lookup here matches both attributes.
//
// Every call is synchronous and bounded by secretServiceCallTimeout so a hung
// or unresponsive secret service can never block the terminal indefinitely.

const (
	secretServiceBusName   = "org.freedesktop.secrets"
	secretServicePath      = dbus.ObjectPath("/org/freedesktop/secrets")
	secretServiceInterface = "org.freedesktop.Secret.Service"

	secretCollectionInterface = "org.freedesktop.Secret.Collection"
	secretItemInterface       = "org.freedesktop.Secret.Item"

	secretServiceAttributeService  = "service"
	secretServiceAttributeUsername = "username"
	secretServiceServiceValue      = "omairc"

	secretServiceLoginCollection = dbus.ObjectPath("/org/freedesktop/secrets/collection/login")
	secretServiceContentType     = "text/plain"

	secretServiceCallTimeout = 5 * time.Second
)

var (
	// errSecretServicePrompt means the service wants interactive authorization
	// that this synchronous client cannot provide.
	errSecretServicePrompt = errors.New("secret service: interactive authorization required")
	// errSecretServiceNoBus means there is no connect function to dial.
	errSecretServiceNoBus = errors.New("secret service: no session bus")
)

// secretServiceSecret is the Secret Service "Secret" struct, signature
// "(oayays)": the session object path, the algorithm parameters, the secret
// bytes, and the content type.
type secretServiceSecret struct {
	Session     dbus.ObjectPath
	Parameters  []byte
	Value       []byte
	ContentType string
}

// secretServiceBackend talks to org.freedesktop.secrets on the session bus.
// The connect function is a field so tests can substitute it; the production
// value is dbus.SessionBus.
type secretServiceBackend struct {
	connect func() (*dbus.Conn, error)

	mu         sync.Mutex
	conn       *dbus.Conn
	connectErr error
}

// newSecretServiceBackend builds the production backend.
func newSecretServiceBackend() *secretServiceBackend {
	return &secretServiceBackend{connect: dbus.SessionBus}
}

// connection returns the shared session-bus connection, dialing it once and
// caching the outcome. A failed dial is cached so a missing bus does not spawn
// an autolaunch attempt on every operation.
func (b *secretServiceBackend) connection() (*dbus.Conn, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if b.conn != nil && b.conn.Connected() {
		return b.conn, nil
	}
	if b.connectErr != nil {
		return nil, b.connectErr
	}
	if b.connect == nil {
		b.connectErr = errSecretServiceNoBus
		return nil, b.connectErr
	}
	conn, err := b.connect()
	if err != nil {
		b.connectErr = err
		return nil, err
	}
	b.conn = conn
	return conn, nil
}

// read returns the stored secret for keyName, or Missing when none exists.
func (b *secretServiceBackend) read(keyName string) CredentialResult {
	conn, err := b.connection()
	if err != nil {
		return credentialUnavailableResult()
	}
	ctx, cancel := context.WithTimeout(context.Background(), secretServiceCallTimeout)
	defer cancel()

	session, err := secretServiceOpenSession(ctx, conn)
	if err != nil {
		return secretServiceResultForError(err)
	}
	items, err := secretServiceSearchItems(ctx, conn, keyName)
	if err != nil {
		return secretServiceResultForError(err)
	}
	if len(items) == 0 {
		return CredentialResult{State: CredentialMissing}
	}
	unlocked, err := secretServiceUnlock(ctx, conn, items)
	if err != nil {
		return secretServiceResultForError(err)
	}
	var lastErr error
	for _, item := range unlocked {
		secret, err := secretServiceGetSecret(ctx, conn, item, session)
		if err != nil {
			lastErr = err
			continue
		}
		password := string(secret.Value)
		if password == "" {
			return CredentialResult{State: CredentialMissing}
		}
		return CredentialResult{State: CredentialAvailable, Password: password}
	}
	if lastErr != nil {
		return secretServiceResultForError(lastErr)
	}
	return CredentialResult{State: CredentialMissing}
}

// write stores password for keyName, updating an existing item when one
// matches and creating one in the default collection otherwise.
func (b *secretServiceBackend) write(keyName, password string) CredentialResult {
	if password == "" {
		return CredentialResult{State: CredentialError, Message: "secret service: empty password"}
	}
	conn, err := b.connection()
	if err != nil {
		return credentialUnavailableResult()
	}
	ctx, cancel := context.WithTimeout(context.Background(), secretServiceCallTimeout)
	defer cancel()

	session, err := secretServiceOpenSession(ctx, conn)
	if err != nil {
		return secretServiceResultForError(err)
	}
	items, err := secretServiceSearchItems(ctx, conn, keyName)
	if err != nil {
		return secretServiceResultForError(err)
	}
	if len(items) > 0 {
		unlocked, err := secretServiceUnlock(ctx, conn, items)
		if err != nil {
			return secretServiceResultForError(err)
		}
		if len(unlocked) == 0 {
			return secretServiceResultForError(errSecretServicePrompt)
		}
		for _, item := range unlocked {
			if err := secretServiceSetSecret(ctx, conn, item, session, password); err != nil {
				return secretServiceResultForError(err)
			}
		}
		return CredentialResult{State: CredentialAvailable}
	}

	collection, err := secretServiceDefaultCollection(ctx, conn)
	if err != nil {
		return secretServiceResultForError(err)
	}
	if err := secretServiceCreateItem(ctx, conn, collection, session, keyName, password); err != nil {
		return secretServiceResultForError(err)
	}
	return CredentialResult{State: CredentialAvailable}
}

// remove deletes every item matching keyName. Deleting nothing succeeds and
// reports Missing, mirroring the Qt store's delete-of-nothing path.
func (b *secretServiceBackend) remove(keyName string) CredentialResult {
	conn, err := b.connection()
	if err != nil {
		return credentialUnavailableResult()
	}
	ctx, cancel := context.WithTimeout(context.Background(), secretServiceCallTimeout)
	defer cancel()

	items, err := secretServiceSearchItems(ctx, conn, keyName)
	if err != nil {
		return secretServiceResultForError(err)
	}
	if len(items) == 0 {
		return CredentialResult{State: CredentialMissing}
	}
	unlocked, err := secretServiceUnlock(ctx, conn, items)
	if err != nil {
		return secretServiceResultForError(err)
	}
	if len(unlocked) == 0 {
		return secretServiceResultForError(errSecretServicePrompt)
	}
	for _, item := range unlocked {
		if err := secretServiceDeleteItem(ctx, conn, item); err != nil {
			return secretServiceResultForError(err)
		}
	}
	return CredentialResult{State: CredentialAvailable}
}

// secretServiceAttributes is the QtKeychain-compatible search/label attribute
// set: service=omairc plus the key name in username.
func secretServiceAttributes(keyName string) map[string]string {
	return map[string]string{
		secretServiceAttributeService:  secretServiceServiceValue,
		secretServiceAttributeUsername: keyName,
	}
}

// secretServiceOpenSession opens a plain (unencrypted) session and returns its
// object path.
func secretServiceOpenSession(ctx context.Context, conn *dbus.Conn) (dbus.ObjectPath, error) {
	obj := conn.Object(secretServiceBusName, secretServicePath)
	call := obj.CallWithContext(ctx, secretServiceInterface+".OpenSession", 0, "plain", dbus.MakeVariant(""))
	if call.Err != nil {
		return "", call.Err
	}
	var output dbus.Variant
	var session dbus.ObjectPath
	if err := call.Store(&output, &session); err != nil {
		return "", err
	}
	return session, nil
}

// secretServiceSearchItems returns every item whose attributes match keyName,
// whether the service reports it unlocked or locked.
func secretServiceSearchItems(ctx context.Context, conn *dbus.Conn, keyName string) ([]dbus.ObjectPath, error) {
	obj := conn.Object(secretServiceBusName, secretServicePath)
	call := obj.CallWithContext(ctx, secretServiceInterface+".SearchItems", 0, secretServiceAttributes(keyName))
	if call.Err != nil {
		return nil, call.Err
	}
	var unlocked, locked []dbus.ObjectPath
	if err := call.Store(&unlocked, &locked); err != nil {
		return nil, err
	}
	return append(unlocked, locked...), nil
}

// secretServiceUnlock unlocks items and returns the paths that are now usable.
// A non-empty prompt path with nothing unlocked means the service wants
// interaction, which fails closed as an error.
func secretServiceUnlock(ctx context.Context, conn *dbus.Conn, items []dbus.ObjectPath) ([]dbus.ObjectPath, error) {
	if len(items) == 0 {
		return nil, nil
	}
	obj := conn.Object(secretServiceBusName, secretServicePath)
	call := obj.CallWithContext(ctx, secretServiceInterface+".Unlock", 0, items)
	if call.Err != nil {
		return nil, call.Err
	}
	var unlocked []dbus.ObjectPath
	var prompt dbus.ObjectPath
	if err := call.Store(&unlocked, &prompt); err != nil {
		return nil, err
	}
	if len(unlocked) == 0 && prompt != "" && prompt != "/" {
		return nil, errSecretServicePrompt
	}
	return unlocked, nil
}

// secretServiceGetSecret reads one item's secret through session.
func secretServiceGetSecret(ctx context.Context, conn *dbus.Conn, item, session dbus.ObjectPath) (secretServiceSecret, error) {
	obj := conn.Object(secretServiceBusName, item)
	call := obj.CallWithContext(ctx, secretItemInterface+".GetSecret", 0, session)
	if call.Err != nil {
		return secretServiceSecret{}, call.Err
	}
	var secret secretServiceSecret
	if err := call.Store(&secret); err != nil {
		return secretServiceSecret{}, err
	}
	return secret, nil
}

// secretServiceSetSecret replaces one item's secret through session.
func secretServiceSetSecret(ctx context.Context, conn *dbus.Conn, item, session dbus.ObjectPath, password string) error {
	obj := conn.Object(secretServiceBusName, item)
	secret := secretServiceSecret{
		Session:     session,
		Parameters:  []byte{},
		Value:       []byte(password),
		ContentType: secretServiceContentType,
	}
	return obj.CallWithContext(ctx, secretItemInterface+".SetSecret", 0, secret).Err
}

// secretServiceDeleteItem deletes one item. A non-empty prompt path means the
// service wants interaction, which fails closed.
func secretServiceDeleteItem(ctx context.Context, conn *dbus.Conn, item dbus.ObjectPath) error {
	obj := conn.Object(secretServiceBusName, item)
	call := obj.CallWithContext(ctx, secretItemInterface+".Delete", 0)
	if call.Err != nil {
		return call.Err
	}
	if len(call.Body) == 0 {
		return nil
	}
	var prompt dbus.ObjectPath
	if err := call.Store(&prompt); err != nil {
		return err
	}
	if prompt != "" && prompt != "/" {
		return errSecretServicePrompt
	}
	return nil
}

// secretServiceCreateItem creates a new item in collection. The item label and
// attributes mirror what QtKeychain writes.
func secretServiceCreateItem(ctx context.Context, conn *dbus.Conn, collection, session dbus.ObjectPath, keyName, password string) error {
	obj := conn.Object(secretServiceBusName, collection)
	properties := map[string]dbus.Variant{
		secretItemInterface + ".Label":      dbus.MakeVariant(keyName),
		secretItemInterface + ".Attributes": dbus.MakeVariant(secretServiceAttributes(keyName)),
	}
	secret := secretServiceSecret{
		Session:     session,
		Parameters:  []byte{},
		Value:       []byte(password),
		ContentType: secretServiceContentType,
	}
	call := obj.CallWithContext(ctx, secretCollectionInterface+".CreateItem", 0, properties, secret, true)
	if call.Err != nil {
		return call.Err
	}
	var item dbus.ObjectPath
	var prompt dbus.ObjectPath
	if err := call.Store(&item, &prompt); err != nil {
		return err
	}
	if item == "" || item == "/" {
		return errSecretServicePrompt
	}
	return nil
}

// secretServiceDefaultCollection resolves the default collection alias,
// falling back to the login collection when no alias is configured.
func secretServiceDefaultCollection(ctx context.Context, conn *dbus.Conn) (dbus.ObjectPath, error) {
	obj := conn.Object(secretServiceBusName, secretServicePath)
	call := obj.CallWithContext(ctx, secretServiceInterface+".ReadAlias", 0, "default")
	if call.Err == nil {
		var path dbus.ObjectPath
		if err := call.Store(&path); err == nil && path != "" && path != "/" {
			return path, nil
		}
	}
	return secretServiceLoginCollection, nil
}

// secretServiceResultForError maps a backend failure to a CredentialResult. A
// missing name or transport is Unavailable; everything else is Error with the
// error text. The password is never part of the message.
func secretServiceResultForError(err error) CredentialResult {
	switch {
	case err == nil:
		return CredentialResult{State: CredentialError}
	case errors.Is(err, errSecretServiceNoBus):
		return credentialUnavailableResult()
	case errors.Is(err, errSecretServicePrompt):
		return CredentialResult{State: CredentialError, Message: err.Error()}
	}
	if name, ok := secretServiceErrorName(err); ok && secretServiceNameIsUnavailable(name) {
		return credentialUnavailableResult()
	}
	return CredentialResult{State: CredentialError, Message: err.Error()}
}

// secretServiceErrorName extracts the DBus error name from a value or pointer
// dbus.Error.
func secretServiceErrorName(err error) (string, bool) {
	var value dbus.Error
	if errors.As(err, &value) {
		return value.Name, true
	}
	var pointer *dbus.Error
	if errors.As(err, &pointer) && pointer != nil {
		return pointer.Name, true
	}
	return "", false
}

// secretServiceNameIsUnavailable reports whether a DBus error name means the
// secret service itself is not available (its name could not be resolved),
// rather than that a call to a live service failed.
func secretServiceNameIsUnavailable(name string) bool {
	switch name {
	case "org.freedesktop.DBus.Error.ServiceUnknown",
		"org.freedesktop.DBus.Error.NameHasNoOwner",
		"org.freedesktop.DBus.Error.Spawn.ExecFailed",
		"org.freedesktop.DBus.Error.Spawn.ChildExited",
		"org.freedesktop.DBus.Error.Spawn.Failed":
		return true
	}
	return false
}

// NewCredentialStore returns the Linux Secret Service store.
func NewCredentialStore() CredentialStore {
	return &credentialStore{backend: newSecretServiceBackend()}
}
