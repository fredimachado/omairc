// Package storage owns omairc-tui's filesystem and keychain integration.
//
// It is the Go counterpart of the Qt files that persist state outside of a
// live session: IrcProfileStore, IrcOpenDirectStore, IrcPlaybackTimeStore,
// IrcConversationLog, and the QSettings-backed preferences. Both clients share
// one QSettings INI file (ConfigPath), so this package must read and write the
// same bytes QSettings does: nested groups written as a top-level [section]
// whose remainder path is backslash-escaped in the key names, strings escaped
// and quoted with the iniEscapedString rules, string lists joined with ", ",
// and @Invalid() for an empty list. settings.go implements that INI engine;
// paths.go, paths_linux.go, paths_darwin.go, and paths_other.go assemble the
// roots.
//
// Package storage may import os and github.com/fredimachado/omairc/tui/internal/irc
// (for shared types such as the case mapping). It must never import
// internal/controller, internal/connection, internal/session, or internal/ui:
// persistence sits below the session and the view.
//
// Later phase-11 files build on this foundation: profilestore.go,
// opendirect.go, playbacktime.go, conversationlog.go, credentialstore.go,
// secretservice.go, secretservice_darwin.go, and secretservice_other.go. The
// platform and INI
// foundation here is what they share, so it deliberately carries no
// protocol or view policy of its own.
package storage
