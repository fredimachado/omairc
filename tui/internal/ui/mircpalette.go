package ui

import (
	"fmt"
	"image/color"

	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// mircPalette is the sixteen desktop MIRC_PALETTE entries from
// client/src/model/format.rs. Protocol carries the index; presentation RGB
// lives here so internal/irc stays palette-index only.
var mircPalette = [16]string{
	"#FFFFFF", "#000000", "#00007F", "#009300", "#FF0000", "#7F0000", "#9C009C", "#FC7F00",
	"#FFFF00", "#00FC00", "#009393", "#00FFFF", "#0000FC", "#FF00FF", "#7F7F7F", "#D2D2D2",
}

// mircLipglossColor maps a controller run color onto a Lip Gloss color. Palette
// indices use mircPalette; RGB triples keep their 24-bit value. Lip Gloss and
// the active terminal color profile down-sample or drop color as needed.
func mircLipglossColor(c *controller.RunColor) (color.Color, bool) {
	if c == nil {
		return nil, false
	}
	if c.IsPalette() {
		index := c.PaletteIndex()
		if int(index) >= len(mircPalette) {
			return nil, false
		}
		return lipgloss.Color(mircPalette[index]), true
	}
	r, g, b := c.RGB()
	return lipgloss.Color(fmt.Sprintf("#%02X%02X%02X", r, g, b)), true
}
