package irc

import "testing"

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
	if ColorPaletteToken(ColorPaletteClassic) != "classic" || ColorPaletteToken(ColorPaletteVivid) != "vivid" {
		t.Fatalf("tokens = %q/%q", ColorPaletteToken(ColorPaletteClassic), ColorPaletteToken(ColorPaletteVivid))
	}
}

func TestNextColorPaletteCycles(t *testing.T) {
	if got := NextColorPalette(ColorPaletteClassic); got != ColorPaletteVivid {
		t.Fatalf("NextColorPalette(classic) = %v, want vivid", got)
	}
	if got := NextColorPalette(ColorPaletteVivid); got != ColorPaletteClassic {
		t.Fatalf("NextColorPalette(vivid) = %v, want classic", got)
	}
}
