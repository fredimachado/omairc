// Package connection is the Go port of IrcNetworkProfile
// (src/irc/ircnetworkprofile.*) and IrcConnection (src/irc/ircconnection.*):
// the profile model plus the draft/selection, apply, discard, add, remove, and
// disconnect surface. Phase 11 adds the persisted profile store
// (internal/storage.ProfileStore), the credential backend
// (internal/storage.CredentialStore), the saved-password flags, startup
// activation, and the Connect sheet's persistence and credential sentences. It
// imports only internal/irc, internal/controller, internal/session, and
// internal/storage.
//
// The Go credential store is synchronous, so the C++ revision/obsolete-key
// state machine is deliberately dropped: Apply writes or removes the current
// key inline, and a rename removes the previous key and writes the new one in
// the same call. Held passwords still live in memory on the Connection.
package connection
