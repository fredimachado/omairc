//go:build unix && !linux && !android && !darwin && !ios

package avatar

import (
	"errors"
	"syscall"
)

func installSIGTSTPDefault() error {
	return errors.New("SIGTSTP default disposition is not installed on this OS")
}

func unblockSIGTSTP() error { return nil }

func stopProcess() error {
	return syscall.Kill(syscall.Getpid(), syscall.SIGTSTP)
}
