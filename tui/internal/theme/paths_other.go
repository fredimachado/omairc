//go:build !linux

package theme

// colorsPath has no defined location off Linux: the Omarchy theme tree is an
// Omarchy concern. Callers fall back to Fallback().
func colorsPath() string { return "" }

// watchDirs is empty off Linux, so the watcher never observes a change.
func watchDirs() []string { return nil }
