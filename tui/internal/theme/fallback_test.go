package theme

import (
	"image/color"
	"slices"
	"testing"
)

// TestFallbackMatchesStyleGoLiterals pins every color Fallback shares with the
// hard-coded palette in internal/ui/style.go. If one of these changes, either
// this package or the Qt client moved and the two must be reconciled.
func TestFallbackMatchesStyleGoLiterals(t *testing.T) {
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
	want := []color.Color{
		mustHex("#7aa2f7"),
		mustHex("#c099ff"),
		mustHex("#7fc8a9"),
		mustHex("#efb366"),
		mustHex("#ed8f9d"),
		mustHex("#9ece6a"),
		mustHex("#7dcfff"),
		mustHex("#ddd06e"),
		mustHex("#ea76cb"),
		mustHex("#7c7cf0"),
	}
	if !slices.Equal(colors.NickPalette, want) {
		t.Fatalf("NickPalette = %v, want %v", colors.NickPalette, want)
	}
}
