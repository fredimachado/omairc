// Package notify is the desktop-notification seam, mirroring
// Backend::notifyDesktop in src/backend.cpp. The platform notifier is chosen
// at build time; the neutral file only defines the interface and a fail-closed
// no-op so a caller always receives a usable Notifier.
package notify

// Activation identifies the conversation a notification belongs to; it is
// delivered when the user activates the notification (body click or its
// "Open" action).
type Activation struct {
	NetworkID string
	Target    string
	MsgID     string
}

// Notifier is the desktop-notification seam. Notify must never block the
// caller and must never panic when no desktop session is available.
type Notifier interface {
	Notify(summary, body, networkID, target, msgid string)
	Close()
}

// New returns the platform notifier. onActivated is called from a background
// goroutine when a notification is activated; it may be nil.
func New(onActivated func(Activation)) Notifier {
	return newPlatform(onActivated)
}

// noopNotifier is the fail-closed notifier used when the platform has no
// notification backend or the desktop bus is unavailable. It is shared by the
// per-platform stubs and by the Linux notifier when the session bus errors.
type noopNotifier struct{}

func (noopNotifier) Notify(string, string, string, string, string) {}

func (noopNotifier) Close() {}
