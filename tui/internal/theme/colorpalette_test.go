package theme

import (
	"slices"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

func TestParseColorPalette(t *testing.T) {
	id, ok := ParseColorPalette("vivid")
	if !ok || id != ColorPaletteVivid {
		t.Fatalf("ParseColorPalette(vivid) = %v/%v, want vivid", id, ok)
	}
	id, ok = ParseColorPalette("CLASSIC")
	if !ok || id != ColorPaletteClassic {
		t.Fatalf("ParseColorPalette(CLASSIC) = %v/%v, want classic", id, ok)
	}
	if _, ok := ParseColorPalette("loud"); ok {
		t.Fatal("ParseColorPalette(loud) ok, want false")
	}
	if irc.ColorPaletteToken(ColorPaletteClassic) != "classic" || irc.ColorPaletteToken(ColorPaletteVivid) != "vivid" {
		t.Fatalf("tokens = %q/%q", irc.ColorPaletteToken(ColorPaletteClassic), irc.ColorPaletteToken(ColorPaletteVivid))
	}
}

func TestSetColorPaletteAffectsDerive(t *testing.T) {
	prev := ActiveColorPalette()
	t.Cleanup(func() { SetColorPalette(prev) })

	SetColorPalette(ColorPaletteVivid)
	colors := Derive(Spec{Mode: ModeDark})
	want := PaletteColors(ColorPaletteVivid, true, colors.Accent)
	if !slices.Equal(colors.Palette, want) {
		t.Fatalf("vivid Palette = %v, want %v", colors.Palette, want)
	}

	SetColorPalette(ColorPaletteClassic)
	colors = Derive(Spec{Mode: ModeDark})
	want = PaletteColors(ColorPaletteClassic, true, colors.Accent)
	if !slices.Equal(colors.Palette, want) {
		t.Fatalf("classic Palette = %v, want %v", colors.Palette, want)
	}
}

func TestPaletteColorsClassicSlotZeroFollowsAccent(t *testing.T) {
	accent := mustHex("#112233")
	got := PaletteColors(ColorPaletteClassic, true, accent)
	if Hex(got[0]) != "#112233" {
		t.Fatalf("classic[0] = %s, want accent", Hex(got[0]))
	}
	if Hex(got[1]) != "#c099ff" {
		t.Fatalf("classic[1] = %s, want #c099ff", Hex(got[1]))
	}
}

func TestPaletteColorsVividIgnoresAccent(t *testing.T) {
	accent := mustHex("#112233")
	got := PaletteColors(ColorPaletteVivid, true, accent)
	if Hex(got[0]) != "#2f6ff5" {
		t.Fatalf("vivid[0] = %s, want #2f6ff5", Hex(got[0]))
	}
}
