package ui

// Slice 3: mIRC FG/BG paint through renderMessageBody. Asserts Lip Gloss SGR
// markers against the desktop MIRC_PALETTE RGB values, preference gating,
// style isolation, wrapping, and the action/notice/mention call sites.

import (
	"strings"
	"testing"
	"time"

	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// redSGR is palette index 4 (#FF0000) as a truecolor foreground sequence.
const redSGR = "\x1b[38;2;255;0;0m"

func TestRenderMessageBodyColorOnlyPaintsWhenColorsOn(t *testing.T) {
	m := seededModel(t)
	if !m.ctrl.PrefColorsEnabled() {
		t.Fatal("seeded model must default colors on")
	}

	got := m.renderMessageBody("\x034red", lipgloss.NewStyle())
	want := lipgloss.NewStyle().Foreground(lipgloss.Color("#FF0000")).Render("red")
	if got != want {
		t.Fatalf("color-only body = %q, want %q", got, want)
	}
	if !strings.Contains(got, redSGR) {
		t.Fatalf("color-only body missing red SGR: %q", got)
	}
	if strings.Contains(got, "\x03") {
		t.Fatalf("rendered body must not keep the mIRC color byte: %q", got)
	}
}

func TestRenderMessageBodyColorsOffKeepsEmphasisStripsPaint(t *testing.T) {
	m := seededModel(t)
	m.ctrl.SetShowIrcColors(false)

	got := m.renderMessageBody("\x02\x0304bold red\x0f plain", lipgloss.NewStyle())
	if strings.Contains(got, redSGR) || strings.Contains(got, "38;2;255;0;0") {
		t.Fatalf("colors off must not paint FG: %q", got)
	}
	boldPlain := lipgloss.NewStyle().Bold(true).Render("bold red")
	if !strings.Contains(got, boldPlain) {
		t.Fatalf("colors off must still bold: got %q, want substring %q", got, boldPlain)
	}
	if !strings.Contains(got, " plain") {
		t.Fatalf("colors off body missing trailing plain text: %q", got)
	}

	colorOnly := m.renderMessageBody("\x034red", lipgloss.NewStyle())
	wantPlain := lipgloss.NewStyle().Render("red")
	if colorOnly != wantPlain {
		t.Fatalf("color-only with colors off = %q, want plain %q", colorOnly, wantPlain)
	}
}

func TestRenderMessageBodyStyleIsolation(t *testing.T) {
	m := seededModel(t)

	got := m.renderMessageBody("\x0304red\x03 plain", lipgloss.NewStyle())
	redPart := lipgloss.NewStyle().Foreground(lipgloss.Color("#FF0000")).Render("red")
	plainPart := lipgloss.NewStyle().Render(" plain")
	if got != redPart+plainPart {
		t.Fatalf("isolation = %q, want %q + %q", got, redPart, plainPart)
	}
	// The plain run is a separate Render, so red SGR cannot open after "red".
	if strings.Count(got, redSGR) != 1 {
		t.Fatalf("red SGR should appear once (no leak into plain run): %q", got)
	}

	base := lipgloss.NewStyle().Foreground(lipgloss.Color("#00FC00"))
	withBase := m.renderMessageBody("\x0304red\x03 keep", base)
	keepPart := base.Render(" keep")
	if !strings.HasSuffix(withBase, keepPart) {
		t.Fatalf("unset color must preserve base style: got %q, want suffix %q", withBase, keepPart)
	}
}

func TestRenderMessageBodyForegroundAndBackground(t *testing.T) {
	m := seededModel(t)

	got := m.renderMessageBody("\x0304,02x", lipgloss.NewStyle())
	want := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#FF0000")).
		Background(lipgloss.Color("#00007F")).
		Render("x")
	if got != want {
		t.Fatalf("fg+bg = %q, want %q", got, want)
	}
	// Lip Gloss may combine FG+BG into one SGR; assert the channel fragments.
	if !strings.Contains(got, "38;2;255;0;0") {
		t.Fatalf("missing red FG in %q", got)
	}
	if !strings.Contains(got, "48;2;0;0;127") {
		t.Fatalf("missing navy BG in %q", got)
	}
}

func TestRenderMessageBodyHexRGB(t *testing.T) {
	m := seededModel(t)

	got := m.renderMessageBody("\x04FF0080pink", lipgloss.NewStyle())
	want := lipgloss.NewStyle().Foreground(lipgloss.Color("#FF0080")).Render("pink")
	if got != want {
		t.Fatalf("hex RGB = %q, want %q", got, want)
	}
}

func TestRenderMessageBodyWrappingKeepsColorMarkers(t *testing.T) {
	m := seededModel(t)
	m.width = 40

	// Long red body so joinWrappedBody wraps under the byline column.
	body := "\x0304" + strings.Repeat("redword ", 20)
	msg := controller.MessageSnapshot{
		Author: "anna",
		Kind:   "message",
		Body:   body,
		Time:   time.Date(2026, 9, 27, 12, 0, 0, 0, time.UTC),
	}
	row := m.chatRow(msg, false, len("anna"))
	if !strings.Contains(row, "38;2;255;0;0") {
		t.Fatalf("wrapped chat row lost red FG: %q", row)
	}
	if strings.Contains(row, "\x03") {
		t.Fatalf("wrapped row kept mIRC color byte: %q", row)
	}
	plain := plainText(row)
	if !strings.Contains(plain, "redword") {
		t.Fatalf("wrapped plain text missing body: %q", plain)
	}
}

func TestRenderMessageBodyActionNoticeMentionKeepColors(t *testing.T) {
	m := seededModel(t)
	at := time.Date(2026, 9, 27, 12, 0, 0, 0, time.UTC)
	body := "\x0304painted"

	action := m.messageRow(0, controller.MessageSnapshot{
		Author: "anna", Kind: "action", Body: body, Time: at,
	})
	if !strings.Contains(action, "38;2;255;0;0") {
		t.Fatalf("action row missing red FG: %q", action)
	}

	notice := m.messageRow(0, controller.MessageSnapshot{
		Author: "anna", Kind: "notice", Body: body, Time: at,
	})
	if !strings.Contains(notice, "38;2;255;0;0") {
		t.Fatalf("notice row missing red FG: %q", notice)
	}

	mention := m.messageRow(0, controller.MessageSnapshot{
		Author: "anna", Kind: "message", Body: body, Time: at, Mentioned: true,
	})
	if !strings.Contains(mention, "38;2;255;0;0") {
		t.Fatalf("mention row missing red FG: %q", mention)
	}
}

func TestRenderMessageBodyAllSixteenPaletteEntries(t *testing.T) {
	m := seededModel(t)
	palette := []string{
		"#FFFFFF", "#000000", "#00007F", "#009300", "#FF0000", "#7F0000", "#9C009C", "#FC7F00",
		"#FFFF00", "#00FC00", "#009393", "#00FFFF", "#0000FC", "#FF00FF", "#7F7F7F", "#D2D2D2",
	}
	for index, hex := range palette {
		input := "\x03" + itoa2(index) + "x"
		got := m.renderMessageBody(input, lipgloss.NewStyle())
		want := lipgloss.NewStyle().Foreground(lipgloss.Color(hex)).Render("x")
		if got != want {
			t.Fatalf("palette[%d] = %q, want %q", index, got, want)
		}
	}
}

func itoa2(n int) string {
	if n < 10 {
		return string(rune('0' + n))
	}
	return string([]byte{byte('0' + n/10), byte('0' + n%10)})
}
