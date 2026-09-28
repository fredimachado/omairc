//go:build darwin

package openurl

import (
	"errors"
	"os/exec"
	"testing"
)

func TestOpenPlatformStartsOpen(t *testing.T) {
	if _, err := exec.LookPath("open"); err != nil {
		t.Skip("open not available")
	}

	restore := platformOpen
	var got string
	platformOpen = func(rawURL string) error {
		got = rawURL
		return nil
	}
	t.Cleanup(func() { platformOpen = restore })

	if err := Open("https://example.com"); err != nil {
		t.Fatalf("Open() = %v, want nil", err)
	}
	if got != "https://example.com" {
		t.Fatalf("platformOpen got %q", got)
	}
}

func TestOpenPlatformRejectsDisallowedOnDarwin(t *testing.T) {
	restore := platformOpen
	platformOpen = func(string) error { return nil }
	t.Cleanup(func() { platformOpen = restore })

	if err := Open("file:///etc/passwd"); !errors.Is(err, ErrDisallowed) {
		t.Fatalf("Open() = %v, want ErrDisallowed", err)
	}
}
