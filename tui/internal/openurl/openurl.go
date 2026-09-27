// Package openurl is the external-URL seam: it opens a URL through the OS
// handler, mirroring the presentation-layer link sheet. The platform opener is
// chosen at build time; the neutral file owns the interface plus a fail-closed
// allowlist so a stray non-web string can never reach the OS handler.
package openurl

import (
	"errors"
	"strings"
	"unicode"
)

// ErrDisallowed is returned when rawURL is not an allowed external web URL.
var ErrDisallowed = errors.New("openurl: disallowed url")

// platformOpen opens rawURL through the OS handler. It is an unexported
// package-level seam so tests can record the URL instead of spawning a real
// handler; production code always runs the per-platform openPlatform. Test-only.
var platformOpen = openPlatform

// Open opens rawURL through the OS handler. It fails closed: unless rawURL is
// an allowed external web URL it returns ErrDisallowed and never reaches the
// platform opener.
func Open(rawURL string) error {
	if !allowed(rawURL) {
		return ErrDisallowed
	}
	return platformOpen(rawURL)
}

// allowed mirrors isAllowedHttpUrl in src/OmaircWindow.qml: scheme http or
// https (case-insensitive) followed by "://", a non-empty remainder, and no
// whitespace anywhere. It is a defence-in-depth guard for the OS handler, not
// the link sheet's source of truth, so it stays self-contained and does not
// import internal/irc.
func allowed(rawURL string) bool {
	lower := strings.ToLower(rawURL)
	var rest string
	switch {
	case strings.HasPrefix(lower, "http://"):
		rest = rawURL[len("http://"):]
	case strings.HasPrefix(lower, "https://"):
		rest = rawURL[len("https://"):]
	default:
		return false
	}
	if rest == "" {
		return false
	}
	return !strings.ContainsFunc(rawURL, unicode.IsSpace)
}
