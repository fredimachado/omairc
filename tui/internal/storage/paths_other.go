//go:build !linux && !darwin

package storage

// TODO(phase 11+): resolve the real per-platform roots. Windows uses
// %LOCALAPPDATA% (and QSettings' NativeFormat registry path). Until then the
// roots are empty so ConfigPath and TranscriptRoot return "". The
// OMAIRC_TRANSCRIPT_ROOT override in paths.go still applies on every platform.
func GenericConfigRoot() string { return "" }

// GenericStateRoot is the Windows counterpart of the Linux XDG base state
// directory. It is unimplemented for now; see the TODO above.
func GenericStateRoot() string { return "" }
