package ui

// This file ports the URL policy and Ctrl+Shift+O link sheet tests: the
// isAllowedHttpUrl allowlist, the trailing-punctuation trim, span offsets,
// INVITE channel extraction, the newest-first link sheet ordering, and the
// openAllowedURL suppression latch.

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
)

func TestIsAllowedHTTPURL(t *testing.T) {
	cases := []struct {
		raw  string
		want bool
	}{
		{"https://example.com", true},
		{"http://example.com", true},
		{"HTTPS://example.com", true},
		{"https:///foo", true}, // hostname is not validated, mirroring the QML
		{"file:///etc/passwd", false},
		{"javascript:alert(1)", false},
		{"ftp://example.com", false},
		{"https://", false},
		{"", false},
		{"https://exa mple.com", false},
		{"https://exa\nmple.com", false},
		{"https://exa\rmple.com", false},
		{"example.com", false},
		{":https://x", false},
		{"https:/example.com", false},
	}
	for _, tc := range cases {
		if got := isAllowedHTTPURL(tc.raw); got != tc.want {
			t.Errorf("isAllowedHTTPURL(%q) = %v, want %v", tc.raw, got, tc.want)
		}
	}
}

func TestTrimHTTPURLMatch(t *testing.T) {
	cases := []struct {
		raw  string
		want string
	}{
		{"https://example.com.", "https://example.com"},
		{"https://example.com/path!", "https://example.com/path"},
		{"https://example.com/x).", "https://example.com/x"},
		{"https://example.com/x))", "https://example.com/x"},
		{"https://en.wikipedia.org/wiki/IRC_(protocol)", "https://en.wikipedia.org/wiki/IRC_(protocol)"},
		{"https://example.com/a]", "https://example.com/a"},
		{"https://example.com/a[0]", "https://example.com/a[0]"},
		{"https://example.com/plain", "https://example.com/plain"},
		{".,;:!?", ""},
	}
	for _, tc := range cases {
		if got := trimHTTPURLMatch(tc.raw); got != tc.want {
			t.Errorf("trimHTTPURLMatch(%q) = %q, want %q", tc.raw, got, tc.want)
		}
	}
}

func TestHTTPURLSpansAndAt(t *testing.T) {
	text := "see https://a.example/x, then https://b.example/y."
	spans := httpURLSpans(text)
	if len(spans) != 2 {
		t.Fatalf("httpURLSpans = %d spans, want 2: %+v", len(spans), spans)
	}
	if spans[0].url != "https://a.example/x" {
		t.Fatalf("spans[0].url = %q, want https://a.example/x", spans[0].url)
	}
	if spans[1].url != "https://b.example/y" {
		t.Fatalf("spans[1].url = %q, want https://b.example/y", spans[1].url)
	}
	if spans[0].start >= spans[1].start {
		t.Fatalf("spans must be in text order: %+v", spans)
	}
	if text[spans[0].start:spans[0].end] != "https://a.example/x" {
		t.Fatalf("span slice = %q", text[spans[0].start:spans[0].end])
	}

	if got := httpURLAt(text, spans[0].start); got != "https://a.example/x" {
		t.Fatalf("httpURLAt inside first span = %q", got)
	}
	if got := httpURLAt(text, spans[1].end-1); got != "https://b.example/y" {
		t.Fatalf("httpURLAt inside second span = %q", got)
	}
	if got := httpURLAt(text, spans[1].end); got != "" {
		t.Fatalf("httpURLAt on the trimmed period = %q, want empty", got)
	}
	if got := httpURLAt(text, 0); got != "" {
		t.Fatalf("httpURLAt outside all spans = %q, want empty", got)
	}
	if got := httpURLAt("open file:///etc/passwd now", 8); got != "" {
		t.Fatalf("httpURLAt on a disallowed scheme = %q, want empty", got)
	}
}

func TestInviteChannelAtAndAfter(t *testing.T) {
	text := "alice invited you to #chan"
	hash := strings.IndexByte(text, '#')
	if hash < 0 {
		t.Fatal("test fixture has no channel")
	}
	if got := inviteChannelAt(text, hash); got != "#chan" {
		t.Fatalf("inviteChannelAt inside channel = %q, want #chan", got)
	}
	if got := inviteChannelAt(text, 0); got != "" {
		t.Fatalf("inviteChannelAt outside channel = %q, want empty", got)
	}
	if got := inviteChannelAt(text, hash+len("#chan")); got != "" {
		t.Fatalf("inviteChannelAt past channel = %q, want empty", got)
	}
	if got := inviteChannelAt("no invite here", 3); got != "" {
		t.Fatalf("inviteChannelAt without marker = %q, want empty", got)
	}
	if got := inviteChannelAfter(text); got != "#chan" {
		t.Fatalf("inviteChannelAfter = %q, want #chan", got)
	}
	if got := inviteChannelAfter("no invite here"); got != "" {
		t.Fatalf("inviteChannelAfter without marker = %q, want empty", got)
	}
}

func TestAppendLinkURLsFromTextOrder(t *testing.T) {
	text := "a https://one.example/x b https://two.example/y"
	matches := appendLinkURLsFromText(nil, text, 3, "")
	if len(matches) != 2 {
		t.Fatalf("appendLinkURLsFromText = %d matches, want 2: %+v", len(matches), matches)
	}
	if matches[0].value != "https://two.example/y" || matches[1].value != "https://one.example/x" {
		t.Fatalf("last-in-text first order = %+v, want two then one", matches)
	}
	if matches[0].row != 3 || matches[1].row != 3 {
		t.Fatalf("rows = %d,%d, want 3,3", matches[0].row, matches[1].row)
	}

	filtered := appendLinkURLsFromText(nil, text, 3, "one")
	if len(filtered) != 1 || filtered[0].value != "https://one.example/x" {
		t.Fatalf("query filter = %+v, want only the one.example URL", filtered)
	}

	// A span that trims away or is disallowed contributes nothing.
	none := appendLinkURLsFromText(nil, "nothing to see", 0, "")
	if len(none) != 0 {
		t.Fatalf("no URL text produced %+v", none)
	}
}

func TestLinkSheetListsNewestFirst(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("keep me")
	m.saveDraft()
	draft := m.composer.Value()

	if !m.ctrl.SendMessage("see https://a.example/x and https://b.example/y") {
		t.Fatal("SendMessage refused the URL message")
	}

	m = press(t, m, ctrlShiftKey('o'))
	if !m.linkVisible() {
		t.Fatal("Ctrl+Shift+O must open the link sheet")
	}
	matches := m.linkMatches()
	if len(matches) != 2 {
		t.Fatalf("linkMatches = %d, want 2: %+v", len(matches), matches)
	}
	if matches[0].value != "https://b.example/y" {
		t.Fatalf("matches[0] = %q, want the b URL (last-in-text first)", matches[0].value)
	}
	if matches[1].value != "https://a.example/x" {
		t.Fatalf("matches[1] = %q, want the a URL", matches[1].value)
	}
	if matches[0].row < 0 || matches[1].row < 0 {
		t.Fatalf("link rows must be valid: %+v", matches)
	}
	if matches[0].row != matches[1].row {
		t.Fatalf("both URLs share a row: %+v", matches)
	}

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.linkVisible() {
		t.Fatal("Escape must close the link sheet")
	}
	if got := m.composer.Value(); got != draft {
		t.Fatalf("composer draft = %q, want unchanged %q", got, draft)
	}
	if !m.composer.Focused() {
		t.Fatal("closing the sheet must return focus to the composer")
	}

	// Reopen and filter down to the b URL, then activate it with the
	// suppression latch so no OS handler runs.
	m = press(t, m, ctrlShiftKey('o'))
	m = press(t, m, tea.KeyPressMsg{Code: 'b', Text: "b"})
	m = press(t, m, tea.KeyPressMsg{Code: '.', Text: "."})
	m = press(t, m, tea.KeyPressMsg{Code: 'e', Text: "e"})
	matches = m.linkMatches()
	if len(matches) != 1 || matches[0].value != "https://b.example/y" {
		t.Fatalf("filtered linkMatches = %+v, want only the b URL", matches)
	}
	m.suppressExternalOpen = true
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.linkVisible() {
		t.Fatal("Enter must close the link sheet")
	}
	if m.lastOpenedURL != "https://b.example/y" {
		t.Fatalf("lastOpenedURL = %q, want the b URL", m.lastOpenedURL)
	}
}

func TestOpenAllowedURLSuppression(t *testing.T) {
	m := seededModel(t)

	if m.openAllowedURL("javascript:alert(1)") {
		t.Fatal("openAllowedURL must reject a disallowed scheme")
	}
	if m.lastOpenedURL != "" {
		t.Fatalf("lastOpenedURL = %q, want empty after a rejected URL", m.lastOpenedURL)
	}

	// The suppression latch keeps the test from launching a real OS handler
	// while still exercising the allowlist.
	m.suppressExternalOpen = true
	if !m.openAllowedURL("https://example.com") {
		t.Fatal("openAllowedURL must allow https")
	}
	if m.lastOpenedURL != "https://example.com" {
		t.Fatalf("lastOpenedURL = %q, want https://example.com", m.lastOpenedURL)
	}
}
