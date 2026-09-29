// Package version holds the single build version for omairc-tui.
//
// Value is injected at link time from the repository version.pri through
// bin/version (see tui/bin/build and tui/bin/package). It must never be
// hardcoded anywhere else in Go source; bin/check-conventions enforces that.
//
// go install does not run those scripts, so Value stays the sentinel below.
// Only this file may call debug.ReadBuildInfo: when the sentinel remains,
// Main.Version is the module version. A release tag v1.0.4 (the git tag
// tui/v1.0.4) displays as 1.0.4, matching version.pri. A pseudo-version is
// shown as-is. This file does not parse version.pri.
package version

import (
	"regexp"
	"runtime/debug"
	"strings"
)

// sentinel is the link-time placeholder. tui/bin/build and tui/bin/package
// replace it. It stays only in this file.
const sentinel = "0.0.0-dev"

// pseudoVersionRE matches a Go pseudo-version, including the form produced
// for an untagged commit (v0.0.0-yyyymmddhhmmss-commit).
var pseudoVersionRE = regexp.MustCompile(`^v[0-9]+\.(0\.0-|\d+\.\d+-([^+]*\.)?0\.)\d{14}-[A-Za-z0-9]+(\+incompatible)?$`)

// Value is the omairc-tui build version. Link flags win. When they are
// absent, init fills Value from the module build info.
var Value = sentinel

func init() {
	if Value != sentinel {
		return
	}
	info, ok := debug.ReadBuildInfo()
	if !ok {
		return
	}
	Value = resolveValue(Value, info.Main.Version)
}

func resolveValue(linkValue, moduleVersion string) string {
	if linkValue != sentinel {
		return linkValue
	}
	displayed := displayModuleVersion(moduleVersion)
	if displayed == "" {
		return linkValue
	}
	return displayed
}

func displayModuleVersion(moduleVersion string) string {
	if moduleVersion == "" || moduleVersion == "(devel)" {
		return ""
	}
	if pseudoVersionRE.MatchString(moduleVersion) {
		return moduleVersion
	}
	return strings.TrimPrefix(moduleVersion, "v")
}
