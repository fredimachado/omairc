//go:build live

package live

import (
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// DefaultTimeout bounds every wait. It matches `client/src/live/wait.rs`.
const DefaultTimeout = 20 * time.Second

// WaitUntil polls predicate until it returns true or timeout elapses, then
// gives it one last look.
func WaitUntil(predicate func() bool, timeout time.Duration) bool {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		if predicate() {
			return true
		}
		time.Sleep(20 * time.Millisecond)
	}
	return predicate()
}

// MessageHasCommand reports whether any message carries command.
func MessageHasCommand(messages []irc.Message, command string) bool {
	return MessageCommandCount(messages, command) > 0
}

// MessageCommandCount counts the messages that carry command.
func MessageCommandCount(messages []irc.Message, command string) int {
	count := 0
	for _, message := range messages {
		if strings.EqualFold(message.Command, command) {
			count++
		}
	}
	return count
}
