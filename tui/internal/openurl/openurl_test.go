package openurl

import (
	"errors"
	"testing"
)

// overridePlatformOpen swaps the package seam and returns the restore func so
// the test never spawns a real handler.
func overridePlatformOpen(t *testing.T) *string {
	t.Helper()
	var got string
	restore := platformOpen
	platformOpen = func(rawURL string) error {
		got = rawURL
		return nil
	}
	t.Cleanup(func() { platformOpen = restore })
	return &got
}

func TestOpenRejectsDisallowed(t *testing.T) {
	got := overridePlatformOpen(t)

	disallowed := []string{
		"file:///etc/passwd",
		"javascript:alert(1)",
		"ftp://example.com",
		"https://",
		"https://exa mple.com",
		"",
	}
	for _, rawURL := range disallowed {
		if err := Open(rawURL); !errors.Is(err, ErrDisallowed) {
			t.Errorf("Open(%q) = %v, want ErrDisallowed", rawURL, err)
		}
	}
	if *got != "" {
		t.Errorf("platformOpen called with %q for a disallowed URL", *got)
	}
}

func TestOpenAllowsWebURLs(t *testing.T) {
	for _, rawURL := range []string{
		"https://example.com",
		"http://example.com/path?q=1",
		"HTTPS://example.com",
	} {
		got := overridePlatformOpen(t)
		if err := Open(rawURL); err != nil {
			t.Fatalf("Open(%q) = %v, want nil", rawURL, err)
		}
		if *got != rawURL {
			t.Errorf("Open(%q): platformOpen got %q", rawURL, *got)
		}
	}
}
