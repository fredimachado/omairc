//go:build darwin

package openurl

// openPlatform is the fail-closed no-op stub. macOS URL opening via `open`
// lands later behind the same Open seam.
func openPlatform(string) error { return nil }
