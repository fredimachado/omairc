//go:build !linux && !darwin && !windows

package openurl

// openPlatform is the fail-closed no-op stub on platforms without a URL
// handler. A real implementation lands later behind the same Open seam.
func openPlatform(string) error { return nil }
