//go:build darwin

package notify

// newPlatform returns the fail-closed no-op notifier. macOS notifications via
// osascript land later behind the same Notifier interface.
func newPlatform(func(Activation)) Notifier {
	return noopNotifier{}
}
