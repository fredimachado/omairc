package ui

import (
	"image/color"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/theme"
)

// TestConfiguredPaletteReachesNickColors proves the configured colorPalette
// reaches Styles.NickPalette, classic slot 0 stays #7aa2f7, a live theme
// swap does not move nick hues, NotifyMsg picks up SetColorPalette, and an
// unchanged id keeps the same NickPalette slice.
func TestConfiguredPaletteReachesNickColors(t *testing.T) {
	vividStyles := stylesForNickPalette(theme.ColorPaletteVivid)
	wantVivid := theme.PaletteColors(theme.ColorPaletteVivid, true, theme.FixedAccent())
	index := nickPaletteIndex("mira")
	if got, want := theme.Hex(vividStyles.nickColor("mira")), theme.Hex(wantVivid[index]); got != want {
		t.Fatalf("vivid nickColor(mira) = %s, want slot %d %s", got, index, want)
	}

	ctrl := controller.New()
	classic := New(ctrl, nil)
	if got := theme.Hex(classic.styles.NickPalette[0]); got != "#7aa2f7" {
		t.Fatalf("classic NickPalette[0] = %s, want #7aa2f7", got)
	}
	wantClassic := theme.PaletteColors(theme.ColorPaletteClassic, true, theme.FixedAccent())
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

	unchanged := classic.styles.NickPalette
	updated, _ := classic.Update(NotifyMsg{})
	got := updated.(*Model)
	if len(unchanged) == 0 || len(got.styles.NickPalette) == 0 || &unchanged[0] != &got.styles.NickPalette[0] {
		t.Fatal("NotifyMsg with unchanged ColorPalette rebuilt NickPalette")
	}

	ctrl.SetColorPalette(theme.ColorPaletteVivid)
	updated, _ = got.Update(NotifyMsg{})
	got = updated.(*Model)
	wantVividSlot0 := theme.Hex(wantVivid[0])
	if hex := theme.Hex(got.styles.NickPalette[0]); hex != wantVividSlot0 {
		t.Fatalf("NotifyMsg after SetColorPalette NickPalette[0] = %s, want %s", hex, wantVividSlot0)
	}
	if got.nickPaletteID != theme.ColorPaletteVivid {
		t.Fatalf("nickPaletteID = %v, want vivid", got.nickPaletteID)
	}
}