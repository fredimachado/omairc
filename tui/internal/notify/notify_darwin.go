//go:build darwin

package notify

import (
	"fmt"
	"os"
	"os/exec"
	"strings"
)

const notifyAppName = "Omairc"

// darwinNotifier mirrors Backend::notifyDesktop through osascript. It fails
// closed: when osascript is unavailable it degrades to noopNotifier rather
// than panicking. display notification does not surface activation callbacks,
// so onActivated is never invoked on macOS.
type darwinNotifier struct{}

// newPlatform returns the macOS notifier when osascript is available, or the
// fail-closed no-op notifier otherwise.
func newPlatform(onActivated func(Activation)) Notifier {
	if _, err := exec.LookPath("osascript"); err != nil {
		return noopNotifier{}
	}
	if os.Getenv("OMAIRC_SKIP_NOTIFICATIONS") != "" {
		return noopNotifier{}
	}
	return darwinNotifier{}
}

// Notify mirrors Backend::notifyDesktop. It never blocks: osascript runs in
// its own goroutine and a failed invocation is silently ignored.
func (darwinNotifier) Notify(summary, body, networkID, target, msgid string) {
	script := fmt.Sprintf(
		"display notification %q with title %q",
		escapeAppleScript(body),
		escapeAppleScript(summary),
	)
	go func() {
		_ = exec.Command("osascript", "-e", script).Run()
	}()
}

func (darwinNotifier) Close() {}

// escapeAppleScript quotes a string for AppleScript double-quoted literals.
func escapeAppleScript(value string) string {
	return strings.NewReplacer("\\", "\\\\", "\"", "\\\"").Replace(value)
}
