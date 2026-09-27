//go:build windows

package openurl

// openPlatform is the fail-closed no-op stub. Windows URL opening lands later
// behind the same Open seam.
func openPlatform(string) error { return nil }
