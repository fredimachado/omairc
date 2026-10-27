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

// ColorPaletteToken is the stored and /pref value. It mirrors ColorPaletteId::token.
func ColorPaletteToken(id ColorPaletteID) string {
	switch id {
	case ColorPaletteVivid:
		return "vivid"
	default:
		return "classic"
	}
}

// ParseColorPalette resolves a stored or typed value. An empty or unknown
// token is not classic; the caller applies the ordinary default. It mirrors
// ColorPaletteId::from_token.
func ParseColorPalette(token string) (ColorPaletteID, bool) {
	switch strings.ToLower(strings.TrimSpace(token)) {
	case "classic":
		return ColorPaletteClassic, true
	case "vivid":
		return ColorPaletteVivid, true
	default:
		return ColorPaletteClassic, false
	}
}
