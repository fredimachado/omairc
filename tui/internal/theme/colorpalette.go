package theme

import (
	"image/color"
	"strings"
)

// ColorPaletteID names a color set for nicks and network icons. Tokens are
// stable config keys under [preferences] colorPalette=… in omairc.conf.
// Unknown tokens fall back to ColorPaletteClassic. Add a constant, token, and
// table row to ship a new set.
type ColorPaletteID int

const (
	// ColorPaletteClassic is the original Tokyo-Night-derived hues. Slot 0
	// follows the theme accent at resolve time.
	ColorPaletteClassic ColorPaletteID = iota
	// ColorPaletteVivid is the brighter fixed chromas.
	ColorPaletteVivid
)

// colorPaletteLen is the hash modulus shared with client/src/model/color_palette.rs.
const colorPaletteLen = 8

type colorPaletteDef struct {
	id          ColorPaletteID
	token       string
	accentSlot0 bool
	dark        [colorPaletteLen]string
	light       [colorPaletteLen]string
}

// colorPaletteTable is the registry. Keep dark/light families parallel.
var colorPaletteTable = []colorPaletteDef{
	{
		id:          ColorPaletteClassic,
		token:       "classic",
		accentSlot0: true,
		dark: [colorPaletteLen]string{
			"#5584aa", // overwritten by accent
			"#c099ff",
			"#7fc8a9",
			"#efb366",
			"#ed8f9d",
			"#9ece6a",
			"#7dcfff",
			"#ddd06e",
		},
		light: [colorPaletteLen]string{
			"#2077b2", // overwritten by accent
			"#7950b8",
			"#237a58",
			"#a45f14",
			"#b44355",
			"#4d7c0f",
			"#0f7b8f",
			"#7c6a0a",
		},
	},
	{
		id:          ColorPaletteVivid,
		token:       "vivid",
		accentSlot0: false,
		dark: [colorPaletteLen]string{
			"#2f6ff5",
			"#8a5ef8",
			"#2eb8c9",
			"#e4661c",
			"#e14d99",
			"#68c65e",
			"#e14844",
			"#e8c23a",
		},
		light: [colorPaletteLen]string{
			"#3474f6",
			"#895df6",
			"#2db6c7",
			"#e4681e",
			"#df4c98",
			"#67c45c",
			"#e04744",
			"#e6bf38",
		},
	},
}

// activeColorPalette is what Derive/Fallback resolve. Default classic. The shell
// sets it from omairc.conf before the theme watcher loads.
var activeColorPalette = ColorPaletteClassic

// SetColorPalette selects the color palette for subsequent Derive/Fallback calls.
// Unknown callers should prefer ParseColorPalette first.
func SetColorPalette(id ColorPaletteID) {
	if _, ok := colorPaletteDefFor(id); ok {
		activeColorPalette = id
		return
	}
	activeColorPalette = ColorPaletteClassic
}

// ActiveColorPalette returns the palette Derive currently resolves.
func ActiveColorPalette() ColorPaletteID {
	return activeColorPalette
}

// ParseColorPalette resolves a stored token, ASCII-case-insensitively.
func ParseColorPalette(token string) (ColorPaletteID, bool) {
	folded := strings.ToLower(strings.TrimSpace(token))
	for _, row := range colorPaletteTable {
		if row.token == folded {
			return row.id, true
		}
	}
	return ColorPaletteClassic, false
}

// Token returns the config token for id.
func (id ColorPaletteID) Token() string {
	if def, ok := colorPaletteDefFor(id); ok {
		return def.token
	}
	return ColorPaletteClassic.Token()
}

// PaletteColors resolves the eight colors for id in the given mode. When the
// definition marks slot 0 as accent-backed, accent replaces that slot.
func PaletteColors(id ColorPaletteID, dark bool, accent color.Color) []color.Color {
	def, ok := colorPaletteDefFor(id)
	if !ok {
		def, _ = colorPaletteDefFor(ColorPaletteClassic)
	}
	hexes := def.light
	if dark {
		hexes = def.dark
	}
	out := make([]color.Color, colorPaletteLen)
	for i, hex := range hexes {
		out[i] = mustHex(hex)
	}
	if def.accentSlot0 {
		out[0] = accent
	}
	return out
}

func colorPaletteDefFor(id ColorPaletteID) (colorPaletteDef, bool) {
	for _, row := range colorPaletteTable {
		if row.id == id {
			return row, true
		}
	}
	return colorPaletteDef{}, false
}
