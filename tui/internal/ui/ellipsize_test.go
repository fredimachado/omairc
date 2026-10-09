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

func TestEllipsizeWideGraphemeAfterResetKeepsStyle(t *testing.T) {
	line := lipgloss.NewStyle().Foreground(lipgloss.Color("#ff0000")).Render("AB") + "😀hello"
	got := ellipsizeLine(line, 4)
	if lipgloss.Width(got) != 4 {
		t.Fatalf("width %d, want 4: %q", lipgloss.Width(got), got)
	}
	if strings.Count(got, ellipsisRune) != 1 {
		t.Fatalf("want one visible ellipsis, got %q", got)
	}
	ellipsis := strings.LastIndex(got, ellipsisRune)
	before := strings.TrimRight(got[:ellipsis], " ")
	if !strings.HasSuffix(before, "\x1b[38;2;255;0;0m") {
		t.Fatalf("ellipsis not inside truecolor SGR: %q", got)
	}
	if strings.HasSuffix(before, "\x1b[m") || strings.HasSuffix(before, "\x1b[0m") {
		t.Fatalf("ellipsis sits after reset: %q", got)
	}

	empty := "\x1b[38;2;255;0;0m\x1b[m😀hello"
	got = ellipsizeLine(empty, 2)
	if lipgloss.Width(got) != 2 {
		t.Fatalf("empty span width %d, want 2: %q", lipgloss.Width(got), got)
	}
	if strings.Count(got, ellipsisRune) != 1 {
		t.Fatalf("empty span want one ellipsis, got %q", got)
	}
	if strings.Count(got, "\x1b[38;2;255;0;0m") != 1 {
		t.Fatalf("empty span reopened truecolor: %q", got)
	}
	ellipsis = strings.LastIndex(got, ellipsisRune)
	before = strings.TrimRight(got[:ellipsis], " ")
	if !strings.HasSuffix(before, "\x1b[m") && !strings.HasSuffix(before, "\x1b[0m") {
		t.Fatalf("empty span should stay bare: %q", got)
	}
}

func TestEllipsizeZeroWidthTailAfterResetKeepsStyle(t *testing.T) {
	osc := "\x1b[38;2;255;0;0mAB\x1b[m\x1b]8;;https://example.com/trail…more\x1b\\😀hello"
	done := make(chan string, 1)
	go func() {
		done <- ellipsizeLine(osc, 4)
	}()
	var got string
	select {
	case got = <-done:
	case <-time.After(3 * time.Second):
		t.Fatal("ellipsizeLine hung on a reset followed by an OSC URL")
	}
	if lipgloss.Width(got) != 4 {
		t.Fatalf("OSC tail width %d, want 4: %q", lipgloss.Width(got), got)
	}
	if !strings.Contains(got, "https://example.com/trail…more") {
		t.Fatalf("OSC URL dropped: %q", got)
	}
	ellipsis := strings.LastIndex(got, ellipsisRune)
	before := strings.TrimRight(got[:ellipsis], " ")
	if !strings.HasSuffix(before, "\x1b[38;2;255;0;0m") {
		t.Fatalf("OSC tail left ellipsis outside truecolor: %q", got)
	}

	zwsp := "\x1b[31mAB\x1b[0m\u200b😀hello"
	got = ellipsizeLine(zwsp, 4)
	if lipgloss.Width(got) != 4 {
		t.Fatalf("ZWSP tail width %d, want 4: %q", lipgloss.Width(got), got)
	}
	if strings.Count(got, ellipsisRune) != 1 {
		t.Fatalf("ZWSP tail want one ellipsis, got %q", got)
	}
	ellipsis = strings.LastIndex(got, ellipsisRune)
	before = strings.TrimRight(got[:ellipsis], " ")
	if !strings.HasSuffix(before, "\x1b[31m") {
		t.Fatalf("ZWSP tail left ellipsis outside SGR: %q", got)
	}

	vs := lipgloss.NewStyle().Foreground(lipgloss.Color("#ff0000")).Render("1") + "\uFE0FXXXX"
	got = ellipsizeLine(vs, 2)
	if lipgloss.Width(got) != 2 {
		t.Fatalf("variation selector width %d, want 2: %q", lipgloss.Width(got), got)
	}
	if strings.Count(got, ellipsisRune) != 1 {
		t.Fatalf("variation selector want one ellipsis, got %q", got)
	}
	if !strings.Contains(got, "\uFE0F") {
		t.Fatalf("variation selector dropped: %q", got)
	}
	ellipsis = strings.LastIndex(got, ellipsisRune)
	before = strings.TrimRight(got[:ellipsis], " ")
	if !strings.HasSuffix(before, "\x1b[38;2;255;0;0m") {
		t.Fatalf("variation selector left ellipsis outside truecolor: %q", got)
	}
}

func TestEllipsizeSourceEllipsisStaysSingle(t *testing.T) {
	got := ellipsizeLine("…hello", 2)
	if lipgloss.Width(got) != 2 {
		t.Fatalf("width %d, want 2: %q", lipgloss.Width(got), got)
	}
	if strings.Count(got, ellipsisRune) != 1 || !strings.HasSuffix(got, ellipsisRune) {
		t.Fatalf("want one trailing ellipsis, got %q", got)
	}

	got = ellipsizeLine("a…bXXXX", 4)
	if lipgloss.Width(got) != 4 {
		t.Fatalf("width %d, want 4: %q", lipgloss.Width(got), got)
	}
	if strings.Count(got, ellipsisRune) != 1 || !strings.HasSuffix(got, ellipsisRune) {
		t.Fatalf("want one trailing ellipsis, got %q", got)
	}

	got = ellipsizeLine("hello…world", 7)
	if lipgloss.Width(got) != 7 {
		t.Fatalf("width %d, want 7: %q", lipgloss.Width(got), got)
	}
	if strings.Count(got, ellipsisRune) != 1 || !strings.HasSuffix(got, ellipsisRune) {
		t.Fatalf("want one trailing ellipsis, got %q", got)
	}

	if got := ellipsizeLine("…hi", 3); got != "…hi" {
		t.Fatalf("fitting ellipsis changed: %q", got)
	}

	if got := ellipsizeLine("…\uFE0F", 2); got != "…\uFE0F" {
		t.Fatalf("fitting VS ellipsis changed: %q", got)
	}

	got = ellipsizeLine("…\uFE0Fhello", 3)
	if lipgloss.Width(got) != 3 {
		t.Fatalf("VS ellipsis width %d, want 3: %q", lipgloss.Width(got), got)
	}
	if strings.Count(got, ellipsisRune) != 1 || !strings.HasSuffix(got, ellipsisRune) {
		t.Fatalf("VS ellipsis want one trailing marker, got %q", got)
	}
	if strings.Contains(got, "\uFE0F") {
		t.Fatalf("VS ellipsis cluster split or kept: %q", got)
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

func TestEllipsizeASCIILeadWideClusterStaysInBudget(t *testing.T) {
	for _, lead := range []string{"1\uFE0F", "1️⃣", "#\uFE0F\u20E3"} {
		line := lead + "XXXX"
		got := ellipsizeLine(line, 2)
		if lipgloss.Width(got) != 2 {
			t.Fatalf("%q width %d, want 2: %q", line, lipgloss.Width(got), got)
		}
		if strings.Count(got, ellipsisRune) != 1 {
			t.Fatalf("%q want one ellipsis, got %q", line, got)
		}
		if strings.Contains(got, lead) || strings.Contains(got, lead[:1]) {
			t.Fatalf("%q split wide cluster: %q", line, got)
		}
		if got := ellipsizeLine(lead, 2); got != lead {
			t.Fatalf("fitting cluster changed: got %q want %q", got, lead)
		}
	}

	got := ellipsizeLine("1\uFE0FXXXX", 3)
	if lipgloss.Width(got) != 3 {
		t.Fatalf("fitting VS width %d, want 3: %q", lipgloss.Width(got), got)
	}
	if strings.Count(got, ellipsisRune) != 1 {
		t.Fatalf("fitting VS want one ellipsis, got %q", got)
	}
	if !strings.Contains(got, "1\uFE0F") || strings.Contains(got, "X") {
		t.Fatalf("fitting VS cluster not kept whole: %q", got)
	}

	got = ellipsizeLine("1️⃣XXXX", 3)
	if lipgloss.Width(got) != 3 {
		t.Fatalf("fitting keycap width %d, want 3: %q", lipgloss.Width(got), got)
	}
	if strings.Count(got, ellipsisRune) != 1 {
		t.Fatalf("fitting keycap want one ellipsis, got %q", got)
	}
	if !strings.Contains(got, "1️⃣") || strings.Contains(got, "X") {
		t.Fatalf("fitting keycap not kept whole: %q", got)
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

func TestEllipsizeNewlinePaddingReachesBudget(t *testing.T) {
	cases := []struct {
		line  string
		width int
		want  string
	}{
		{"abc\ndefghijkl", 4, "abc\n   …"},
		{"abc\ndefghijkl", 5, "abc\nd   …"},
		{"中\n文nick", 4, "中\n   …"},
	}
	for _, tc := range cases {
		got := ellipsizeLine(tc.line, tc.width)
		if got != tc.want {
			t.Fatalf("ellipsizeLine(%q, %d) = %q, want %q", tc.line, tc.width, got, tc.want)
		}
		if lipgloss.Width(got) != tc.width {
			t.Fatalf("ellipsizeLine(%q, %d) width %d, want %d: %q", tc.line, tc.width, lipgloss.Width(got), tc.width, got)
		}
		if strings.Count(got, ellipsisRune) != 1 {
			t.Fatalf("ellipsizeLine(%q, %d) want one ellipsis, got %q", tc.line, tc.width, got)
		}
	}
}

func TestEllipsizeOSCNewlineStaysOnBudget(t *testing.T) {
	line := "\x1b]8;;https://ex.com/a\nb\x1b\\helloworld"
	got := ellipsizeLine(line, 10)
	want := "\x1b]8;;https://ex.com/ab\x1b\\helloworld"
	if got != want {
		t.Fatalf("width 10: got %q, want %q", got, want)
	}
	if lipgloss.Width(got) != 10 {
		t.Fatalf("width 10: lipgloss.Width %d, want 10: %q", lipgloss.Width(got), got)
	}
	if strings.Contains(got, "\n") || strings.Contains(got, ellipsisRune) {
		t.Fatalf("width 10: newline or ellipsis leaked: %q", got)
	}

	got = ellipsizeLine(line, 1)
	want = "\x1b]8;;https://ex.com/ab\x1b\\…"
	if got != want {
		t.Fatalf("width 1: got %q, want %q", got, want)
	}
	if lipgloss.Width(got) != 1 {
		t.Fatalf("width 1: lipgloss.Width %d, want 1: %q", lipgloss.Width(got), got)
	}
	if strings.Count(got, ellipsisRune) != 1 || strings.Contains(got, "\n") {
		t.Fatalf("width 1: want one ellipsis and no newline, got %q", got)
	}

	line = "ab\x1b]8;;u\nrl\x1b\\cdefghij"
	got = ellipsizeLine(line, 1)
	want = "…\x1b]8;;url\x1b\\"
	if got != want {
		t.Fatalf("embedded OSC width 1: got %q, want %q", got, want)
	}
	if lipgloss.Width(got) != 1 {
		t.Fatalf("embedded OSC width 1: lipgloss.Width %d, want 1: %q", lipgloss.Width(got), got)
	}
	if strings.Count(got, ellipsisRune) != 1 || strings.Contains(got, "\n") {
		t.Fatalf("embedded OSC width 1: want one ellipsis and no newline, got %q", got)
	}
}

func TestEllipsizeControlNewlineStaysOnBudget(t *testing.T) {
	cases := []struct {
		name  string
		line  string
		width int
		want  string
	}{
		{"torn CSI", "\x1b[31\nmHELLO", 5, "\x1b[31mHELLO"},
		{"torn truecolor", "\x1b[38;2;255\n;0;0mHELLO", 5, "\x1b[38;2;255;0;0mHELLO"},
		{"torn truecolor width one", "\x1b[38;2;255\n;0;0mHELLO", 1, "\x1b[38;2;255;0;0m…"},
		{"DCS payload", "\x1bPpayload\nhelloworld\x1b\\", 5, "\x1bPpayloadhelloworld\x1b\\"},
		{"DCS then text", "hi\x1bPpay\nload\x1b\\XXXX", 4, "hi\x1bPpayload\x1b\\X…"},
		{"SOS then text", "\x1bXpay\nload\x1b\\XXXX", 3, "\x1bXpayload\x1b\\XX…"},
		{"PM then text", "\x1b^pay\nload\x1b\\XXXX", 3, "\x1b^payload\x1b\\XX…"},
		{"APC then text", "\x1b_pay\nload\x1b\\XXXX", 3, "\x1b_payload\x1b\\XX…"},
		{"SOS payload", "\x1bXpayload\nhelloworld\x1b\\", 5, "\x1bXpayloadhelloworld\x1b\\"},
		{"PM payload", "\x1b^payload\nhelloworld\x1b\\", 5, "\x1b^payloadhelloworld\x1b\\"},
		{"APC payload", "\x1b_payload\nhelloworld\x1b\\", 5, "\x1b_payloadhelloworld\x1b\\"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := ellipsizeLine(tc.line, tc.width)
			if got != tc.want {
				t.Fatalf("got %q, want %q", got, tc.want)
			}
			if strings.Contains(got, "\n") {
				t.Fatalf("newline inside a sequence leaked: %q", got)
			}
			if lipgloss.Width(got) > tc.width {
				t.Fatalf("width %d wider than budget %d: %q", lipgloss.Width(got), tc.width, got)
			}
			if strings.Count(got, ellipsisRune) > 0 && lipgloss.Width(got) != tc.width {
				t.Fatalf("ellipsis width %d, want %d: %q", lipgloss.Width(got), tc.width, got)
			}
		})
	}

	// A ground-state break still pads on the ellipsis line.
	got := ellipsizeLine("abc\ndefghijkl", 4)
	if got != "abc\n   …" {
		t.Fatalf("ground newline: got %q, want %q", got, "abc\n   …")
	}
	if lipgloss.Width(got) != 4 || !strings.Contains(got, "\n") {
		t.Fatalf("ground newline width or break: width %d %q", lipgloss.Width(got), got)
	}
}

func TestEllipsizeTailSGRDoesNotStyleEllipsis(t *testing.T) {
	cases := []struct {
		name string
		line string
		want string
		// kept is the SGR of the last positive-width cell, immediately before
		// the ellipsis (pad spaces trimmed). tail is SGR that must sit after it.
		kept string
		tail string
	}{
		{
			name: "reset then green",
			line: "\x1b[31mRed\x1b[0m\x1b[32mGreener text\x1b[0m",
			want: "\x1b[31mRed\x1b[31m…\x1b[0m\x1b[32m\x1b[0m",
			kept: "\x1b[31m",
			tail: "\x1b[32m",
		},
		{
			name: "green without reset",
			line: "\x1b[31mRed\x1b[32mGreener text",
			want: "\x1b[31mRed\x1b[31m…\x1b[32m",
			kept: "\x1b[31m",
			tail: "\x1b[32m",
		},
		{
			name: "bold off",
			line: "\x1b[1;31mRed\x1b[22mlonger text",
			want: "\x1b[1;31mRed\x1b[1;31m…\x1b[22m",
			kept: "\x1b[1;31m",
			tail: "\x1b[22m",
		},
		{
			name: "underline off",
			line: "\x1b[4;31mRed\x1b[24mlonger",
			want: "\x1b[4;31mRed\x1b[4;31m…\x1b[24m",
			kept: "\x1b[4;31m",
			tail: "\x1b[24m",
		},
		{
			name: "foreground off",
			line: "\x1b[31;1mRed\x1b[39mlonger",
			want: "\x1b[31;1mRed\x1b[31;1m…\x1b[39m",
			kept: "\x1b[31;1m",
			tail: "\x1b[39m",
		},
		{
			name: "wide glyph pad",
			line: "\x1b[31mAB\x1b[32m文nick",
			want: "\x1b[31mAB\x1b[31m …\x1b[32m",
			kept: "\x1b[31m",
			tail: "\x1b[32m",
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := ellipsizeLine(tc.line, 4)
			if got != tc.want {
				t.Fatalf("got %q, want %q", got, tc.want)
			}
			if lipgloss.Width(got) != 4 {
				t.Fatalf("width %d, want 4: %q", lipgloss.Width(got), got)
			}
			if strings.Count(got, ellipsisRune) != 1 {
				t.Fatalf("want one ellipsis, got %q", got)
			}
			ellipsis := strings.LastIndex(got, ellipsisRune)
			before := strings.TrimRight(got[:ellipsis], " ")
			if !strings.HasSuffix(before, tc.kept) {
				t.Fatalf("ellipsis not inside kept SGR %q: %q", tc.kept, got)
			}
			if strings.Contains(got[:ellipsis], tc.tail) {
				t.Fatalf("tail SGR %q wraps the ellipsis: %q", tc.tail, got)
			}
			if strings.LastIndex(got, tc.tail) < ellipsis {
				t.Fatalf("tail SGR %q not after the ellipsis: %q", tc.tail, got)
			}
			if tc.name == "wide glyph pad" {
				if strings.Contains(got, "文") {
					t.Fatalf("wide glyph split or kept past the budget: %q", got)
				}
				if got[ellipsis-1] != ' ' {
					t.Fatalf("pad space not inside kept SGR: %q", got)
				}
			}
		})
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

func TestEllipsizeJoinedSequenceFitsWithoutEllipsis(t *testing.T) {
	cases := []struct {
		name      string
		line      string
		width     int
		want      string
		wantWidth int
	}{
		{
			name:      "torn CSI",
			line:      "\x1b[31\nmHELLO",
			width:     6,
			want:      "\x1b[31mHELLO",
			wantWidth: 5,
		},
		{
			name:      "torn DCS",
			line:      "\x1bPpayload\nhelloworld\x1b\\",
			width:     10,
			want:      "\x1bPpayloadhelloworld\x1b\\",
			wantWidth: 0,
		},
		{
			name:      "ground break and torn OSC",
			line:      "ab\ncd\x1b]8;;u\nWXYZ\x1b\\z",
			width:     3,
			want:      "ab\ncd\x1b]8;;uWXYZ\x1b\\z",
			wantWidth: 3,
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := ellipsizeLine(tc.line, tc.width)
			if got != tc.want {
				t.Fatalf("got %q, want %q", got, tc.want)
			}
			if strings.Contains(got, ellipsisRune) {
				t.Fatalf("fitting joined line grew an ellipsis: %q", got)
			}
			if lipgloss.Width(got) != tc.wantWidth {
				t.Fatalf("width %d, want %d: %q", lipgloss.Width(got), tc.wantWidth, got)
			}
		})
	}
}

func TestEllipsizeTailSGRBeforeZeroWidthResets(t *testing.T) {
	cases := []struct {
		name      string
		line      string
		want      string
		wantStyle string
	}{
		{
			name:      "green OSC before wide glyph",
			line:      "AB\x1b[32m\x1b]8;;https://example.com\x1b\\文nick",
			want:      "AB\x1b[32m\x1b]8;;https://example.com\x1b\\\x1b[0m…",
			wantStyle: "",
		},
		{
			name:      "underline before ZWSP",
			line:      "\x1b[31mAB\x1b[4m\u200b文nick",
			want:      "\x1b[31mAB\x1b[4m\u200b\x1b[0m\x1b[31m…",
			wantStyle: "\x1b[31m",
		},
		{
			name:      "bold before ZWSP",
			line:      "\x1b[31mAB\x1b[1m\u200b文nick",
			want:      "\x1b[31mAB\x1b[1m\u200b\x1b[0m\x1b[31m…",
			wantStyle: "\x1b[31m",
		},
		{
			name:      "background before ZWSP",
			line:      "\x1b[31mAB\x1b[45m\u200b文nick",
			want:      "\x1b[31mAB\x1b[45m\u200b\x1b[0m\x1b[31m…",
			wantStyle: "\x1b[31m",
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := ellipsizeLine(tc.line, 3)
			if got != tc.want {
				t.Fatalf("got %q, want %q", got, tc.want)
			}
			if lipgloss.Width(got) != 3 {
				t.Fatalf("width %d, want 3: %q", lipgloss.Width(got), got)
			}
			if strings.Count(got, ellipsisRune) != 1 {
				t.Fatalf("want one ellipsis, got %q", got)
			}
			if strings.Contains(got, "文") {
				t.Fatalf("wide glyph kept past the budget: %q", got)
			}
			ellipsis := strings.LastIndex(got, ellipsisRune)
			if activeSGRReplay(got[:ellipsis]) != tc.wantStyle {
				t.Fatalf("ellipsis SGR %q, want %q: %q", activeSGRReplay(got[:ellipsis]), tc.wantStyle, got)
			}
			if tc.name == "green OSC before wide glyph" && !strings.Contains(got, "https://example.com") {
				t.Fatalf("OSC URL dropped: %q", got)
			}
		})
	}
}

func TestEllipsizeNonSGRFinalDoesNotSwallowM(t *testing.T) {
	cases := []struct {
		name string
		line string
		want string
	}{
		{
			name: "EL then printable m",
			line: "\x1b[0Kmo\x1b[32mr\x1b[0mlonger text",
			want: "\x1b[0Kmo…\x1b[32m\x1b[0m",
		},
		{
			name: "CUU then printable m",
			line: "A\x1b[1Am\x1b[0mlongertext",
			want: "A\x1b[1Am…\x1b[0m",
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := ellipsizeLine(tc.line, 3)
			if got != tc.want {
				t.Fatalf("got %q, want %q", got, tc.want)
			}
			if lipgloss.Width(got) != 3 {
				t.Fatalf("width %d, want 3: %q", lipgloss.Width(got), got)
			}
			if strings.Count(got, ellipsisRune) != 1 {
				t.Fatalf("want one ellipsis, got %q", got)
			}
			ellipsis := strings.LastIndex(got, ellipsisRune)
			if activeSGRReplay(got[:ellipsis]) != "" {
				t.Fatalf("ellipsis picked up a swallowed CSI: %q", got)
			}
		})
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
