// Package version holds the single build version for omairc-tui.
//
// Value is injected at link time from the repository version.pri through
// bin/version (see tui/bin/build). It must never be hardcoded anywhere else
// in Go source; bin/check-conventions enforces that.
package version

// Value is the omairc-tui build version. tui/bin/build injects it with
// `-ldflags "-X .../internal/version.Value=<version>"`, mirroring the Qt
// OMAIRC_BUILD_VERSION override so CI snapshot versions match.
var Value = "0.0.0-dev"
