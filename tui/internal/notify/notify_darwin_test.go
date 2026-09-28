//go:build darwin

package notify

import (
	"os/exec"
	"testing"
)

func TestNewReturnsDarwinNotifierWhenOsascriptPresent(t *testing.T) {
	if _, err := exec.LookPath("osascript"); err != nil {
		t.Skip("osascript not available")
	}

	n := New(nil)
	if n == nil {
		t.Fatal("New(nil) returned nil")
	}
	n.Notify("summary", "body", "net", "#chan", "1")
	n.Close()
	n.Close()
}

func TestEscapeAppleScriptQuotesLiterals(t *testing.T) {
	got := escapeAppleScript(`say "hi" \ now`)
	want := `say \"hi\" \\ now`
	if got != want {
		t.Fatalf("escapeAppleScript = %q, want %q", got, want)
	}
}
