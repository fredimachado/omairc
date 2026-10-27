package ui

import (
	"image/color"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/theme"
)

// TestConfiguredPaletteReachesNickColors proves the configured colorPalette
// reaches Styles.NickPalette, classic slot 0 stays #7aa2f7, and a live theme
// swap (accent + light mode) does not move nick hues.
func TestConfiguredPaletteReachesNickColors(t *testing.T) {
	prev := theme.ActiveColorPalette()
	t.Cleanup(func() { theme.SetColorPalette(prev) })

	theme.SetColorPalette(theme.ColorPaletteVivid)
	vivid := defaultStyles()
	wantVivid := theme.PaletteColors(theme.ColorPaletteVivid, true, theme.Fallback().Accent)
	index := nickPaletteIndex("mira")
	if got, want := theme.Hex(vivid.nickColor("mira")), theme.Hex(wantVivid[index]); got != want {
		t.Fatalf("vivid nickColor(mira) = %s, want slot %d %s", got, index, want)
	}

	theme.SetColorPalette(theme.ColorPaletteClassic)
	classic := New(nil, nil)
	if got := theme.Hex(classic.styles.NickPalette[0]); got != "#7aa2f7" {
		t.Fatalf("classic NickPalette[0] = %s, want #7aa2f7", got)
	}
	wantClassic := theme.PaletteColors(theme.ColorPaletteClassic, true, theme.Fallback().Accent)
	if got, want := theme.Hex(classic.styles.nickColor("mira")), theme.Hex(wantClassic[index]); got != want {
		t.Fatalf("classic nickColor(mira) = %s, want %s", got, want)
	}

	before := theme.Hex(classic.styles.nickColor("mira"))
	classic.SetTheme(theme.Derive(theme.Spec{
		Background:    theme.Fallback().Background,
		Foreground:    theme.Fallback().Foreground,
		Accent:        color.RGBA{R: 0xff, G: 0x00, B: 0x44, A: 0xff},
		Selection:     theme.Fallback().Selection,
		Mode:          theme.ModeLight,
		HasBackground: true,
		HasForeground: true,
		HasAccent:     true,
		HasSelection:  true,
		HasMode:       true,
	}))
	after := theme.Hex(classic.styles.nickColor("mira"))
	if before != after {
		t.Fatalf("nickColor moved with theme: before %s after %s", before, after)
	}
	if got := theme.Hex(classic.styles.NickPalette[0]); got != "#7aa2f7" {
		t.Fatalf("classic NickPalette[0] after SetTheme = %s, want #7aa2f7", got)
	}
}
