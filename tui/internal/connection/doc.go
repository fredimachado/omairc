// Package connection owns the Connect sheet's draft and roster in Go, and
// reconciles Apply with the live session. The draft/selection, apply, discard
// add, remove, and disconnect surface mirrors the sheet model in
// `client/src/model/connect.rs` (that file does not connect). Persistence goes
// through internal/storage.ProfileStore and CredentialStore, including
// saved-password flags, startup activation, and the Connect sheet's
// persistence and credential sentences. It imports only internal/irc,
// internal/controller, internal/session, and internal/storage.
// The Go credential store is synchronous, so the C++ revision/obsolete-key
// state machine is deliberately dropped: Apply writes or removes the current
// key inline, and a rename removes the previous key and writes the new one in
// the same call. Held passwords still live in memory on the Connection.
package connection
