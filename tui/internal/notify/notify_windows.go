//go:build windows

package notify

// newPlatform returns the fail-closed no-op notifier. Windows Toast
// notifications land later behind the same Notifier interface.
func newPlatform(func(Activation)) Notifier {
	return noopNotifier{}
}
