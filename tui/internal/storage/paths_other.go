//go:build !linux && !darwin && !windows

package storage

// GenericConfigRoot and GenericStateRoot are unimplemented on this platform.
// ConfigPath and TranscriptRoot return "" unless OMAIRC_TRANSCRIPT_ROOT is set.
func GenericConfigRoot() string { return "" }

// GenericStateRoot is the counterpart of the Linux XDG base state directory.
func GenericStateRoot() string { return "" }
