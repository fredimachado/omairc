package theme

import (
	"image/color"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// ColorPaletteID names a color set for nicks and network icons. Tokens live in
// the IRC layer; this package keeps the hex tables and the active selection.
// Unknown tokens fall back to ColorPaletteClassic at ParseColorPalette.
type ColorPaletteID = irc.ColorPaletteID

const (
	// ColorPaletteClassic is the original Tokyo-Night-derived hues. Slot 0
	// follows the theme accent at resolve time.
	ColorPaletteClassic = irc.ColorPaletteClassic
	// ColorPaletteVivid is the brighter fixed chromas.
	ColorPaletteVivid = irc.ColorPaletteVivid
)

// ColorPaletteLen is the hash modulus shared with client/src/model/color_palette.rs.
const ColorPaletteLen = 8

type colorPaletteDef struct {
	id          ColorPaletteID
	token       string
	accentSlot0 bool
	dark        [ColorPaletteLen]string
	light       [ColorPaletteLen]string
	darkColors  [ColorPaletteLen]color.Color
	lightColors [ColorPaletteLen]color.Color
}

// colorPaletteTable is the registry. Keep dark/light families parallel.
// Hex strings stay in source for the Rust↔Go parity gate; parsed colors are
// filled once at init so Derive does not re-parse. Tokens must match
// irc.ColorPaletteToken for each id.
var colorPaletteTable = []colorPaletteDef{
	{
		id:          ColorPaletteClassic,
		token:       "classic",
		accentSlot0: true,
		dark: [ColorPaletteLen]string{
			"#5584aa", // overwritten by accent
			"#c099ff",
			"#7fc8a9",
			"#efb366",
			"#ed8f9d",
			"#9ece6a",
			"#7dcfff",
			"#ddd06e",
		},
		light: [ColorPaletteLen]string{
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
		dark: [ColorPaletteLen]string{
			"#2f6ff5",
			"#8a5ef8",
			"#2eb8c9",
			"#e4661c",
			"#e14d99",
			"#68c65e",
			"#e14844",
			"#e8c23a",
		},
		light: [ColorPaletteLen]string{
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

func init() {
	for i := range colorPaletteTable {
		for j, hex := range colorPaletteTable[i].dark {
			colorPaletteTable[i].darkColors[j] = mustHex(hex)
		}
		for j, hex := range colorPaletteTable[i].light {
			colorPaletteTable[i].lightColors[j] = mustHex(hex)
		}
	}
}

// activeColorPalette is what Derive/Fallback resolve. Default classic. The shell
// sets it from omairc.conf before the theme watcher loads.
var activeColorPalette = ColorPaletteClassic

// SetColorPalette selects the color palette for subsequent Derive/Fallback calls.
// Unknown callers should prefer ParseColorPalette first.
func SetColorPalette(id ColorPaletteID) {
	_ = colorPaletteDefFor(id)
	activeColorPalette = id
}

// ActiveColorPalette returns the palette Derive currently resolves.
func ActiveColorPalette() ColorPaletteID {
	return activeColorPalette
}

// ParseColorPalette resolves a stored token, ASCII-case-insensitively.
func ParseColorPalette(token string) (ColorPaletteID, bool) {
	return irc.ParseColorPalette(token)
}

// PaletteColors resolves the eight colors for id in the given mode. When the
// definition marks slot 0 as accent-backed, accent replaces that slot.
func PaletteColors(id ColorPaletteID, dark bool, accent color.Color) []color.Color {
	def := colorPaletteDefFor(id)
	src := def.lightColors
	if dark {
		src = def.darkColors
	}
	out := make([]color.Color, ColorPaletteLen)
	copy(out, src[:])
	if def.accentSlot0 {
		out[0] = accent
	}
	return out
}

func colorPaletteDefFor(id ColorPaletteID) colorPaletteDef {
	for _, row := range colorPaletteTable {
		if row.id == id {
			return row
		}
	}
	panic("every ColorPaletteID has a table row")
}
