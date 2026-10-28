package irc

import "strings"

// ColorPaletteID names a color set for nicks and network icons. Tokens are the
// /pref palette values and the colorPalette config key. Hex tables live in
// internal/theme; this package owns only the id. It mirrors ColorPaletteId in
// client/src/irc/colorpalette.rs.
type ColorPaletteID int

const (
	// ColorPaletteClassic is the original Tokyo-Night-derived hues. Slot 0
	// follows the theme accent at resolve time.
	ColorPaletteClassic ColorPaletteID = iota
	// ColorPaletteVivid is the brighter fixed chromas.
	ColorPaletteVivid
)

type colorPaletteSpec struct {
	id    ColorPaletteID
	token string
}

// colorPaletteCatalog is catalog order, shared by token parse, Next, and the
// Preferences tab cycle. It mirrors ColorPaletteId::ALL.
var colorPaletteCatalog = []colorPaletteSpec{
	{ColorPaletteClassic, "classic"},
	{ColorPaletteVivid, "vivid"},
}

// ColorPalettes is the catalog ids in order. It mirrors ColorPaletteId::ALL.
func ColorPalettes() []ColorPaletteID {
	out := make([]ColorPaletteID, len(colorPaletteCatalog))
	for i, row := range colorPaletteCatalog {
		out[i] = row.id
	}
	return out
}

// ColorPaletteToken is the stored and /pref value. It mirrors ColorPaletteId::token.
func ColorPaletteToken(id ColorPaletteID) string {
	for _, row := range colorPaletteCatalog {
		if row.id == id {
			return row.token
		}
	}
	return colorPaletteCatalog[0].token
}

// ParseColorPalette resolves a stored or typed value. An empty or unknown
// token is not classic; the caller applies the ordinary default. It mirrors
// ColorPaletteId::from_token.
func ParseColorPalette(token string) (ColorPaletteID, bool) {
	folded := strings.ToLower(strings.TrimSpace(token))
	for _, row := range colorPaletteCatalog {
		if row.token == folded {
			return row.id, true
		}
	}
	return ColorPaletteClassic, false
}

// NextColorPalette is the next catalog id. The Preferences tab cycles with Space.
// It mirrors ColorPaletteId::next.
func NextColorPalette(id ColorPaletteID) ColorPaletteID {
	if len(colorPaletteCatalog) == 0 {
		return ColorPaletteClassic
	}
	for i, row := range colorPaletteCatalog {
		if row.id == id {
			return colorPaletteCatalog[(i+1)%len(colorPaletteCatalog)].id
		}
	}
	return colorPaletteCatalog[0].id
}
