package theme

import (
	"slices"
	"testing"
)

// TestFallbackMatchesStyleGoLiterals pins every color Fallback shares with the
// hard-coded palette in internal/ui/style.go. If one of these changes, either
// this package or the Qt client moved and the two must be reconciled.
func TestFallbackMatchesStyleGoLiterals(t *testing.T) {
	SetColorPalette(ColorPaletteClassic)
	colors := Fallback()
	assertHex(t, "Background", colors.Background, "#1a1b26")
	assertHex(t, "AvatarPage", colors.AvatarPage, "#1a1b26")
	assertHex(t, "Accent", colors.Accent, "#7aa2f7")
	assertHex(t, "TextMuted", colors.TextMuted, "#6c7086")
	assertHex(t, "TextDim", colors.TextDim, "#565f89")
	assertHex(t, "Good", colors.Good, "#9ece6a")
	assertHex(t, "Warning", colors.Warning, "#e0af68")
	assertHex(t, "Danger", colors.Danger, "#f7768e")
	assertHex(t, "Mention", colors.Mention, "#f7768e")
	assertHex(t, "Unread", colors.Unread, "#e0af68")
	assertHex(t, "Page", colors.Page, "#1a1b26")

	if colors.NickAvatarMix != 0.23 {
		t.Fatalf("NickAvatarMix = %v, want 0.23", colors.NickAvatarMix)
	}
	want := PaletteColors(ColorPaletteClassic, true, colors.Accent)
	if !slices.Equal(colors.Palette, want) {
		t.Fatalf("Palette = %v, want classic dark %v", colors.Palette, want)
	}
}
