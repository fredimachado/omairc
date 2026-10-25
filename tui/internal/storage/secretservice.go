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
// bus. It mirrors the dual-attribute contract in poc/.../secretattrs.rs and
// the Linux Rust store: a read finds both the items QtKeychain wrote for the
// Qt client (user/server) and the ones the TUI wrote (service/username), a
// write stores one item both of them can read, and a delete removes both sets.
//
// Every call is synchronous and bounded by secretServiceCallTimeout so a hung
// or unresponsive secret service can never block the terminal indefinitely.

const (
	secretServiceBusName   = "org.freedesktop.secrets"
	secretServicePath      = dbus.ObjectPath("/org/freedesktop/secrets")
	secretServiceInterface = "org.freedesktop.Secret.Service"

	secretCollectionInterface = "org.freedesktop.Secret.Collection"
	secretItemInterface       = "org.freedesktop.Secret.Item"
	secretPropertiesInterface = "org.freedesktop.DBus.Properties"

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
// Both attribute sets are searched; a dual-set item this client wrote wins
// over older single-set copies.
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
	items, err := secretServiceSearchAllItems(ctx, conn, keyName)
	if err != nil {
		return secretServiceResultForError(err)
	}
	if len(items) == 0 {
		return CredentialResult{State: CredentialMissing}
	}
	unlocked, _, err := secretServiceUnlock(ctx, conn, items)
	if err != nil {
		return secretServiceResultForError(err)
	}

	type candidate struct {
		written  bool
		password string
	}
	var candidates []candidate
	var lastErr error
	for _, item := range unlocked {
		attributes, err := secretServiceGetAttributes(ctx, conn, item)
		if err != nil {
			lastErr = err
			continue
		}
		secret, err := secretServiceGetSecret(ctx, conn, item, session)
		if err != nil {
			lastErr = err
			continue
		}
		password := secretAttrsDecodeSecret(secret.Value, attributes)
		if password == "" {
			continue
		}
		candidates = append(candidates, candidate{
			written:  secretAttrsIsWritten(attributes, keyName),
			password: password,
		})
	}
	if len(candidates) == 0 {
		if lastErr != nil {
			return secretServiceResultForError(lastErr)
		}
		return CredentialResult{State: CredentialMissing}
	}
	// Prefer the dual-set item; keep search order inside each group.
	chosen := candidates[0]
	for _, c := range candidates[1:] {
		if c.written && !chosen.written {
			chosen = c
		}
	}
	return CredentialResult{State: CredentialAvailable, Password: chosen.password}
}

// write stores password for keyName in one dual-set item every client can
// read, then drops older single-set copies so none of them is read stale.
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
	collection, err := secretServiceDefaultCollection(ctx, conn)
	if err != nil {
		return secretServiceResultForError(err)
	}
	if err := secretServiceCreateItem(ctx, conn, collection, session, keyName, password); err != nil {
		return secretServiceResultForError(err)
	}

	items, err := secretServiceSearchAllItems(ctx, conn, keyName)
	if err != nil {
		return secretServiceResultForError(err)
	}
	if len(items) == 0 {
		return CredentialResult{State: CredentialAvailable}
	}
	unlocked, prompt, err := secretServiceUnlock(ctx, conn, items)
	if err != nil {
		return secretServiceResultForError(err)
	}
	if secretServiceUnlockIncomplete(secretServicePathStrings(items), secretServicePathStrings(unlocked), string(prompt)) {
		return secretServiceResultForError(errSecretServicePrompt)
	}
	for _, item := range unlocked {
		attributes, err := secretServiceGetAttributes(ctx, conn, item)
		if err != nil {
			return secretServiceResultForError(err)
		}
		if secretAttrsIsWritten(attributes, keyName) {
			continue
		}
		if err := secretServiceDeleteItem(ctx, conn, item); err != nil {
			return secretServiceResultForError(err)
		}
	}
	return CredentialResult{State: CredentialAvailable}
}

// remove deletes every item matching keyName under either attribute set.
// Deleting nothing succeeds and reports Missing, mirroring the Qt store's
// delete-of-nothing path.
func (b *secretServiceBackend) remove(keyName string) CredentialResult {
	conn, err := b.connection()
	if err != nil {
		return credentialUnavailableResult()
	}
	ctx, cancel := context.WithTimeout(context.Background(), secretServiceCallTimeout)
	defer cancel()

	items, err := secretServiceSearchAllItems(ctx, conn, keyName)
	if err != nil {
		return secretServiceResultForError(err)
	}
	if len(items) == 0 {
		return CredentialResult{State: CredentialMissing}
	}
	unlocked, prompt, err := secretServiceUnlock(ctx, conn, items)
	if err != nil {
		return secretServiceResultForError(err)
	}
	if secretServiceUnlockIncomplete(secretServicePathStrings(items), secretServicePathStrings(unlocked), string(prompt)) {
		return secretServiceResultForError(errSecretServicePrompt)
	}
	for _, item := range unlocked {
		if err := secretServiceDeleteItem(ctx, conn, item); err != nil {
			return secretServiceResultForError(err)
		}
	}
	return CredentialResult{State: CredentialAvailable}
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

// secretServiceSearchAllItems returns every unique item matching either
// attribute set for keyName, whether the service reports it unlocked or locked.
func secretServiceSearchAllItems(ctx context.Context, conn *dbus.Conn, keyName string) ([]dbus.ObjectPath, error) {
	seen := make(map[dbus.ObjectPath]struct{})
	var items []dbus.ObjectPath
	for _, attributes := range secretAttrsLookups(keyName) {
		found, err := secretServiceSearchItems(ctx, conn, attributes)
		if err != nil {
			return nil, err
		}
		for _, item := range found {
			if _, ok := seen[item]; ok {
				continue
			}
			seen[item] = struct{}{}
			items = append(items, item)
		}
	}
	return items, nil
}

// secretServiceSearchItems returns every item whose attributes match the given
// set, whether the service reports it unlocked or locked.
func secretServiceSearchItems(ctx context.Context, conn *dbus.Conn, attributes map[string]string) ([]dbus.ObjectPath, error) {
	obj := conn.Object(secretServiceBusName, secretServicePath)
	call := obj.CallWithContext(ctx, secretServiceInterface+".SearchItems", 0, attributes)
	if call.Err != nil {
		return nil, call.Err
	}
	var unlocked, locked []dbus.ObjectPath
	if err := call.Store(&unlocked, &locked); err != nil {
		return nil, err
	}
	return append(unlocked, locked...), nil
}

// secretServiceUnlock unlocks items and returns the paths that are now usable
// plus any prompt path. A non-empty prompt with nothing unlocked fails closed.
// Callers that must touch every searched item (write cleanup, remove) also
// check secretServiceUnlockIncomplete so a partial unlock cannot report success.
func secretServiceUnlock(ctx context.Context, conn *dbus.Conn, items []dbus.ObjectPath) ([]dbus.ObjectPath, dbus.ObjectPath, error) {
	if len(items) == 0 {
		return nil, "/", nil
	}
	obj := conn.Object(secretServiceBusName, secretServicePath)
	call := obj.CallWithContext(ctx, secretServiceInterface+".Unlock", 0, items)
	if call.Err != nil {
		return nil, "", call.Err
	}
	var unlocked []dbus.ObjectPath
	var prompt dbus.ObjectPath
	if err := call.Store(&unlocked, &prompt); err != nil {
		return nil, "", err
	}
	if len(unlocked) == 0 && prompt != "" && prompt != "/" {
		return nil, prompt, errSecretServicePrompt
	}
	return unlocked, prompt, nil
}

// secretServicePathStrings converts object paths for the incomplete-unlock check.
func secretServicePathStrings(paths []dbus.ObjectPath) []string {
	out := make([]string, len(paths))
	for i, path := range paths {
		out[i] = string(path)
	}
	return out
}

// secretServiceGetAttributes reads one item's Attributes property.
func secretServiceGetAttributes(ctx context.Context, conn *dbus.Conn, item dbus.ObjectPath) (map[string]string, error) {
	obj := conn.Object(secretServiceBusName, item)
	call := obj.CallWithContext(ctx, secretPropertiesInterface+".Get", 0, secretItemInterface, "Attributes")
	if call.Err != nil {
		return nil, call.Err
	}
	var value dbus.Variant
	if err := call.Store(&value); err != nil {
		return nil, err
	}
	switch attributes := value.Value().(type) {
	case map[string]string:
		return attributes, nil
	case map[string]any:
		out := make(map[string]string, len(attributes))
		for name, raw := range attributes {
			text, ok := raw.(string)
			if !ok {
				continue
			}
			out[name] = text
		}
		return out, nil
	default:
		return nil, errors.New("secret service: unexpected Attributes type")
	}
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

// secretServiceCreateItem creates (or replaces) a dual-set item in collection.
// The attributes match secretAttrsWritten so QtKeychain and the TUI both find it.
func secretServiceCreateItem(ctx context.Context, conn *dbus.Conn, collection, session dbus.ObjectPath, keyName, password string) error {
	obj := conn.Object(secretServiceBusName, collection)
	properties := map[string]dbus.Variant{
		secretItemInterface + ".Label":      dbus.MakeVariant(keyName),
		secretItemInterface + ".Attributes": dbus.MakeVariant(secretAttrsWritten(keyName)),
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
