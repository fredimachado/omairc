package notify

import "testing"

// TestNewReturnsUsableNotifier proves the fail-closed contract: New always
// returns a non-nil Notifier, and Notify/Close never panic even when no
// desktop session is available. CI/Xvfb has no session bus, so on Linux this
// exercises the no-op path; the test does not require a live bus.
func TestNewReturnsUsableNotifier(t *testing.T) {
	n := New(nil)
	if n == nil {
		t.Fatal("New(nil) returned a nil Notifier")
	}
	n.Notify("a", "b", "", "", "")
	n.Close()

	activated := make(chan Activation, 1)
	n = New(func(a Activation) { activated <- a })
	if n == nil {
		t.Fatal("New(fn) returned a nil Notifier")
	}
	n.Notify("summary", "body", "", "", "")
	n.Close()
	n.Close() // Close must be idempotent.
}
