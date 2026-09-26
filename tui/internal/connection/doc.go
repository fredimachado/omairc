// Package connection is the Go port of IrcNetworkProfile
// (src/irc/ircnetworkprofile.*) and IrcConnection (src/irc/ircconnection.*):
// the profile model plus the in-memory draft/selection, apply, discard, add,
// remove, and disconnect surface. It imports only internal/irc,
// internal/controller, and internal/session.
//
// Persistence (IrcProfileStore/QSettings) and credentials (CredentialStore,
// the OS keychain, and the saved-password flags) are deliberately absent; they
// are phase-11 seams. Passwords live in memory on the Connection for the life
// of the process.
package connection
