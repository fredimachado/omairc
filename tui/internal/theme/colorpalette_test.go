package theme

import (
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

func TestFixedAccentIsTokyoNightBlue(t *testing.T) {
	if Hex(FixedAccent()) != "#7aa2f7" {
		t.Fatalf("FixedAccent = %s, want #7aa2f7", Hex(FixedAccent()))
	}
}
