//go:build !linux && !darwin && !windows

package notify

// newPlatform returns the fail-closed no-op notifier on platforms without a
// notification backend. A real implementation lands later behind the same
// Notifier interface.
func newPlatform(func(Activation)) Notifier {
	return noopNotifier{}
}
