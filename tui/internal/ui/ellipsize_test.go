package ui

import (
	"strings"
	"testing"
	"time"

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
	ellipsis := strings.LastIndex(got, ellipsisRune)
	if ellipsis < 0 {
		t.Fatalf("missing ellipsis: %q", got)
	}
	if !strings.HasSuffix(got[:ellipsis], "\x1b[31m") {
		t.Fatalf("ellipsis not in active SGR, want suffix \\x1b[31m before …: %q", got)
	}
	if strings.Count(got, ellipsisRune) != 1 {
		t.Fatalf("want one visible ellipsis, got %q", got)
	}

	twoSpan := "\x1b[31mRed\x1b[0m\x1b[32mGreener text\x1b[0m"
	got = ellipsizeLine(twoSpan, 4)
	if endsWithResetBareEllipsis(got) {
		t.Fatalf("two-span line left bare ellipsis after reset: %q", got)
	}
	if lipgloss.Width(got) != 4 {
		t.Fatalf("width %d, want 4: %q", lipgloss.Width(got), got)
	}
	if strings.Count(got, ellipsisRune) != 1 {
		t.Fatalf("want one visible ellipsis, got %q", got)
	}

	brightBG := "\x1b[101mRed\x1b[0mLONGER TEXT\x1b[0m"
	got = ellipsizeLine(brightBG, 4)
	if endsWithResetBareEllipsis(got) {
		t.Fatalf("bright background left bare ellipsis after reset: %q", got)
	}
	if lipgloss.Width(got) != 4 {
		t.Fatalf("width %d, want 4: %q", lipgloss.Width(got), got)
	}
	ellipsis = strings.LastIndex(got, ellipsisRune)
	if !strings.Contains(got[:ellipsis], "\x1b[101m") {
		t.Fatalf("ellipsis not inside bright background SGR: %q", got)
	}
	if strings.HasSuffix(got[:ellipsis], "\x1b[0m") {
		t.Fatalf("ellipsis sits after reset: %q", got)
	}

	prefixStyled := lipgloss.NewStyle().Foreground(lipgloss.Color("#ff0000")).Render("Red") + "longer text"
	got = ellipsizeLine(prefixStyled, 4)
	if endsWithResetBareEllipsis(got) {
		t.Fatalf("lipgloss prefix+plain tail left bare ellipsis after reset: %q", got)
	}
	if lipgloss.Width(got) != 4 {
		t.Fatalf("width %d, want 4: %q", lipgloss.Width(got), got)
	}
	ellipsis = strings.LastIndex(got, ellipsisRune)
	if ellipsis < 0 {
		t.Fatalf("missing ellipsis: %q", got)
	}
	if !strings.Contains(got[:ellipsis], "38;2;") {
		t.Fatalf("ellipsis not in active truecolor SGR: %q", got)
	}
	if strings.HasSuffix(got[:ellipsis], "\x1b[m") || strings.HasSuffix(got[:ellipsis], "\x1b[0m") {
		t.Fatalf("ellipsis sits after reset: %q", got)
	}

	truecolor := "\x1b[38;2;255;0;0mRed\x1b[0mlonger text"
	got = ellipsizeLine(truecolor, 4)
	if endsWithResetBareEllipsis(got) {
		t.Fatalf("truecolor SGR left bare ellipsis after reset: %q", got)
	}
	if lipgloss.Width(got) != 4 {
		t.Fatalf("width %d, want 4: %q", lipgloss.Width(got), got)
	}
	ellipsis = strings.LastIndex(got, ellipsisRune)
	if !strings.Contains(got[:ellipsis], "\x1b[38;2;255;0;0m") {
		t.Fatalf("ellipsis not in truecolor SGR, want \\x1b[38;2;255;0;0m before …: %q", got)
	}

	resetThenBright := "\x1b[0;101mRed\x1b[0mLONGER TEXT\x1b[0m"
	got = ellipsizeLine(resetThenBright, 4)
	if endsWithResetBareEllipsis(got) {
		t.Fatalf("0;101 SGR left bare ellipsis after reset: %q", got)
	}
	if lipgloss.Width(got) != 4 {
		t.Fatalf("width %d, want 4: %q", lipgloss.Width(got), got)
	}
	ellipsis = strings.LastIndex(got, ellipsisRune)
	if !strings.Contains(got[:ellipsis], "\x1b[0;101m") && !strings.Contains(got[:ellipsis], "\x1b[101m") {
		t.Fatalf("ellipsis not inside 0;101 bright background SGR: %q", got)
	}

	for _, reset := range []string{"\x1b[00m", "\x1b[0;m", "\x1b[0;0m"} {
		line := "\x1b[31mRed" + reset + "longer"
		got = ellipsizeLine(line, 4)
		if endsWithResetBareEllipsis(got) {
			t.Fatalf("reset spelling %q left bare ellipsis: %q", reset, got)
		}
		if lipgloss.Width(got) != 4 {
			t.Fatalf("reset %q width %d, want 4: %q", reset, lipgloss.Width(got), got)
		}
		ellipsis = strings.LastIndex(got, ellipsisRune)
		if !strings.HasSuffix(got[:ellipsis], "\x1b[31m") {
			t.Fatalf("reset %q: ellipsis not in active SGR: %q", reset, got)
		}
	}

	blackFG := lipgloss.NewStyle().Foreground(lipgloss.Color("#000000")).Render("Red") + "longer text"
	got = ellipsizeLine(blackFG, 4)
	if endsWithResetBareEllipsis(got) {
		t.Fatalf("black foreground left bare ellipsis after reset: %q", got)
	}
	blueBG := lipgloss.NewStyle().Background(lipgloss.Color("#0000ff")).Render("Red") + "longer text"
	got = ellipsizeLine(blueBG, 4)
	if endsWithResetBareEllipsis(got) {
		t.Fatalf("blue background left bare ellipsis after reset: %q", got)
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

func TestEllipsizeOSCWithEllipsisInURLDoesNotHang(t *testing.T) {
	const width = 4
	line := "中文nick" + "\x1b]8;;https://example.com/trail…more\x1b\\"
	done := make(chan string, 1)
	go func() {
		done <- ellipsizeLine(line, width)
	}()
	select {
	case got := <-done:
		if lipgloss.Width(got) != width {
			t.Fatalf("width %d, want %d: %q", lipgloss.Width(got), width, got)
		}
		visible := got
		if idx := strings.Index(visible, "\x1b]"); idx >= 0 {
			visible = visible[:idx]
		}
		if strings.Count(visible, ellipsisRune) != 1 {
			t.Fatalf("want one visible ellipsis before copied escapes, got %q", got)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("ellipsizeLine hung on OSC URL containing an ellipsis rune")
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
