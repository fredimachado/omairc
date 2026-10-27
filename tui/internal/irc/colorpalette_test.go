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

func TestNextColorPaletteCyclesTheCatalog(t *testing.T) {
	catalog := ColorPalettes()
	if len(catalog) == 0 {
		t.Fatal("ColorPalettes is empty")
	}
	start := catalog[0]
	id := start
	seen := make(map[ColorPaletteID]bool, len(catalog))
	for range catalog {
		seen[id] = true
		id = NextColorPalette(id)
	}
	if id != start {
		t.Fatalf("after %d steps got %v, want %v", len(catalog), id, start)
	}
	for _, want := range catalog {
		if !seen[want] {
			t.Fatalf("cycle missed %v; seen %v", want, seen)
		}
	}
}
