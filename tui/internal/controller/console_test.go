package controller

import (
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

func TestConsoleText(t *testing.T) {
	c := New()
	if got := c.ConsoleText("omarchy"); got != nil {
		t.Fatalf("ConsoleText(empty) = %v, want nil", got)
	}

	now := time.Date(2026, 9, 12, 12, 0, 0, 0, time.UTC)
	c.Console().Append(irc.Lifecycle("omarchy", irc.LogSeverityInfo, "test", "first line", now))
	c.Console().Append(irc.Lifecycle("omarchy", irc.LogSeverityInfo, "test", "second line", now))

	got := c.ConsoleText("omarchy")
	want := []string{"first line", "second line"}
	if len(got) != len(want) {
		t.Fatalf("ConsoleText len = %d, want %d (%v)", len(got), len(want), got)
	}
	for index := range want {
		if got[index] != want[index] {
			t.Fatalf("ConsoleText[%d] = %q, want %q", index, got[index], want[index])
		}
	}
}
