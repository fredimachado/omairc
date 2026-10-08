package ui

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
)

func TestEllipsizeShortLineUnchanged(t *testing.T) {
	line := "short"
	got := ellipsizeLine(line, 10)
	if got != line {
		t.Fatalf("got %q, want unchanged %q", got, line)
	}
	if lipgloss.Width(got) != lipgloss.Width(line) {
		t.Fatalf("width changed: got %d want %d", lipgloss.Width(got), lipgloss.Width(line))
	}
}

func TestEllipsizeASCIIFitsBudget(t *testing.T) {
	const width = 8
	line := "long sidebar label"
	got := ellipsizeLine(line, width)
	if !strings.HasSuffix(got, ellipsisRune) {
		t.Fatalf("missing ellipsis: %q", got)
	}
	if strings.Contains(got, "...") {
		t.Fatalf("used ASCII dots instead of ellipsis: %q", got)
	}
	if lipgloss.Width(got) != width {
		t.Fatalf("width %d, want %d: %q", lipgloss.Width(got), width, got)
	}
}

func TestEllipsizeStyledKeepsEllipsisInStyle(t *testing.T) {
	styled := lipgloss.NewStyle().Foreground(lipgloss.Color("#ff0000")).Render("long label text")
	const width = 10
	got := ellipsizeLine(styled, width)
	if endsWithResetBareEllipsis(got) {
		t.Fatalf("ellipsis sits after a reset: %q", got)
	}
	if lipgloss.Width(got) != width {
		t.Fatalf("width %d, want %d: %q", lipgloss.Width(got), width, got)
	}

	sgr := "\x1b[31mRed\x1b[0mlonger text"
	got = ellipsizeLine(sgr, 4)
	if endsWithResetBareEllipsis(got) {
		t.Fatalf("SGR line left bare ellipsis after reset: %q", got)
	}
	if lipgloss.Width(got) != 4 {
		t.Fatalf("width %d, want 4: %q", lipgloss.Width(got), got)
	}
}

func TestEllipsizeWideGlyphDoesNotSplit(t *testing.T) {
	const width = 4
	line := "中文nick"
	got := ellipsizeLine(line, width)
	if lipgloss.Width(got) != width {
		t.Fatalf("width %d, want %d: %q", lipgloss.Width(got), width, got)
	}
	plain := ansiStripForTest(got)
	if strings.Contains(plain, "文") && strings.HasSuffix(plain, "文…") {
		t.Fatalf("split wide glyph across cells: %q", plain)
	}
	if !strings.HasSuffix(plain, ellipsisRune) {
		t.Fatalf("missing ellipsis: %q", plain)
	}
}

func TestEllipsizeNonPositiveWidthEmpty(t *testing.T) {
	line := "anything"
	if got := ellipsizeLine(line, 0); got != "" {
		t.Fatalf("width 0: got %q", got)
	}
	if got := ellipsizeLine(line, -3); got != "" {
		t.Fatalf("width -3: got %q", got)
	}
}

func TestEllipsizeANSIDoesNotConsumeBudget(t *testing.T) {
	const width = 6
	line := "\x1b[31mred\x1b[0m tail"
	got := ellipsizeLine(line, width)
	if lipgloss.Width(got) != width {
		t.Fatalf("width %d, want %d: %q", lipgloss.Width(got), width, got)
	}
	if !strings.Contains(got, "…") {
		t.Fatalf("missing ellipsis: %q", got)
	}
	// Visible text should be five letters plus ellipsis, not counting escapes.
	plain := ansiStripForTest(got)
	if lipgloss.Width(plain) != width {
		t.Fatalf("plain width %d, want %d: %q", lipgloss.Width(plain), width, plain)
	}
}

func TestEllipsizeWidthOne(t *testing.T) {
	got := ellipsizeLine("hello", 1)
	if got != ellipsisRune {
		t.Fatalf("got %q, want ellipsis only", got)
	}
	if lipgloss.Width(got) != 1 {
		t.Fatalf("width %d, want 1", lipgloss.Width(got))
	}
}

// ansiStripForTest drops escape sequences for readable assertions.
func ansiStripForTest(s string) string {
	var b strings.Builder
	for i := 0; i < len(s); {
		if s[i] == '\x1b' && i+1 < len(s) && s[i+1] == '[' {
			end := strings.IndexByte(s[i+2:], 'm')
			if end < 0 {
				b.WriteByte(s[i])
				i++
				continue
			}
			i += end + 3
			continue
		}
		b.WriteByte(s[i])
		i++
	}
	return b.String()
}
