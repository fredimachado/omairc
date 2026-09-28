// Package theme derives the terminal palette from the Omarchy desktop theme.
//
// It mirrors Backend::loadOmarchyTheme in src/backend.cpp for reading
// colors.toml and the derived colors in src/qml/OmaircStyle.qml for the
// structural mixes. The fixed semantic colors are ported verbatim so the TUI
// stays visually identical to the Qt client; only the surfaces, borders, and
// secondary text are mixed from the theme's background, foreground, and accent.
//
// The package speaks image/color and stays lipgloss-free: internal/ui wraps
// these values in lipgloss at the rendering edge.
package theme

import (
	"fmt"
	"image/color"
	"math"
)

// Mode values for the colors.toml "mode" key. ParseColorsTOML recognizes only
// these two; anything else falls back to background luminance, matching
// Backend::loadOmarchyTheme.
const (
	ModeDark  = "dark"
	ModeLight = "light"
)

// Colors is the derived palette.
//
// The first block is derived from the theme: its values move with the theme's
// background, foreground, and accent. The second block is fixed so the two
// clients stay in step with OmaircStyle.qml; deriving it would break parity.
// Every field is an opaque image/color value; Hex formats one for lipgloss.
type Colors struct {
	// Derived structural colors.
	Background    color.Color
	Foreground    color.Color
	Accent        color.Color
	Selection     color.Color
	Surface       color.Color
	SurfaceRaised color.Color
	Border        color.Color
	BorderFocus   color.Color
	TextMuted     color.Color
	TextDim       color.Color
	AvatarPage    color.Color

	// UnreadMark and UnreadMarkRule paint the transcript's "New messages"
	// boundary, mirroring UnreadMark.qml's markColor/lineColor: the caption
	// mixed from the theme ink toward the accent, and the rule mixed from the
	// page toward the accent.
	UnreadMark     color.Color
	UnreadMarkRule color.Color

	// Fixed colors shared with the Qt client.
	Good          color.Color
	Warning       color.Color
	Danger        color.Color
	Mention       color.Color
	Unread        color.Color
	Page          color.Color
	NickPalette   []color.Color
	NickAvatarMix float64
}

// Spec is a parsed Omarchy colors.toml.
//
// Mode is always resolved: ParseColorsTOML takes the file's "mode" when it is
// "dark" or "light", otherwise it infers the mode from the background
// luminance. The Has* flags record whether the file supplied a usable value, so
// Derive can fall back to the Qt defaults for the missing ones.
type Spec struct {
	Background color.Color
	Foreground color.Color
	Accent     color.Color
	Selection  color.Color

	Mode string

	HasBackground bool
	HasForeground bool
	HasAccent     bool
	HasSelection  bool
	HasMode       bool
}

// Dark reports whether the spec resolves to dark mode. A zero Spec (an unknown
// mode) defaults to dark, matching the TUI's dark-only roots.
func (s Spec) Dark() bool {
	return s.Mode != ModeLight
}

// Hex formats c as "#rrggbb". It is the bridge to lipgloss.Color, which is a
// string color type.
func Hex(c color.Color) string {
	pixel := rgba(c)
	return fmt.Sprintf("#%02x%02x%02x", pixel.R, pixel.G, pixel.B)
}

// Derive builds the palette from spec. Missing colors fall back to the Qt
// defaults for the resolved mode, structural colors are mixed from the
// background/foreground/accent, and secondary text is clamped for contrast.
func Derive(spec Spec) Colors {
	dark := spec.Dark()
	background := pickColor(spec.Background, spec.HasBackground, dark, darkBackgroundDefault, lightBackgroundDefault)
	foreground := pickColor(spec.Foreground, spec.HasForeground, dark, darkForegroundDefault, lightForegroundDefault)
	accent := pickColor(spec.Accent, spec.HasAccent, dark, darkAccentDefault, lightAccentDefault)
	selection := pickColor(spec.Selection, spec.HasSelection, dark, darkSelectionDefault, lightSelectionDefault)

	// In dark mode the foreground is lighter than the background, so mixing
	// toward it lightens; in light mode it darkens. The identicon mix amount is
	// likewise mode-specific (0.23 dark / 0.16 light), and the light mode swaps
	// in OmaircStyle.qml's light nick palette.
	nickPalette := nickPaletteLight
	nickAvatarMix := 0.16
	if dark {
		nickPalette = nickPaletteDark
		nickAvatarMix = 0.23
	}

	return Colors{
		Background:    background,
		Foreground:    foreground,
		Accent:        accent,
		Selection:     selection,
		Surface:       mixColors(background, foreground, shadeAmount(dark, 0.035, 0.025)),
		SurfaceRaised: mixColors(background, foreground, shadeAmount(dark, 0.075, 0.055)),
		Border:        mixColors(background, foreground, shadeAmount(dark, 0.13, 0.11)),
		BorderFocus:   mixColors(background, accent, shadeAmount(dark, 0.70, 0.80)),
		TextMuted: derivedSecondary(
			background, foreground,
			shadeAmount(dark, 0.52, 0.47), mutedContrastRatio, fixedMuted),
		TextDim: derivedSecondary(
			background, foreground,
			shadeAmount(dark, 0.34, 0.30), dimContrastRatio, fixedDim),
		AvatarPage: background,

		// UnreadMark.qml mixes ink toward the accent for the caption and the
		// page toward the accent for the rule, with mode-specific amounts.
		UnreadMark:     mixColors(foreground, accent, shadeAmount(dark, 0.55, 0.42)),
		UnreadMarkRule: mixColors(background, accent, shadeAmount(dark, 0.48, 0.36)),

		Good:          fixedGood,
		Warning:       fixedWarning,
		Danger:        fixedDanger,
		Mention:       fixedMention,
		Unread:        fixedUnread,
		Page:          fixedPage,
		NickPalette:   nickPalette,
		NickAvatarMix: nickAvatarMix,
	}
}

// Contrast targets and the cap on how far clampContrast may nudge a derived
// color. The cap keeps "muted" text from being chased into harsh white; past
// it, deriveSecondary falls back to the fixed Tokyo-Night constants.
const (
	mutedContrastRatio = 4.5
	dimContrastRatio   = 3.0
	maxContrastClamp   = 0.4
	contrastStep       = 0.05
)

// Derived palette helpers.

// shadeAmount selects the dark- or light-mode mix amount.
func shadeAmount(dark bool, darkValue, lightValue float64) float64 {
	if dark {
		return darkValue
	}
	return lightValue
}

// pickColor returns value when the spec supplied one, else the mode's default.
func pickColor(value color.Color, present, dark bool, darkDefault, lightDefault color.Color) color.Color {
	if present && value != nil {
		return value
	}
	if dark {
		return darkDefault
	}
	return lightDefault
}

// deriveSecondary mixes a secondary text color from the theme and keeps it
// legible: the candidate is nudged toward the higher-contrast extreme, and when
// even the capped nudge cannot reach minRatio it falls back to the fixed
// constant instead of emitting text that disappears into the background.
func derivedSecondary(background, foreground color.Color, mix, minRatio float64, fallback color.Color) color.Color {
	candidate := mixColors(background, foreground, mix)
	if contrastRatio(candidate, background) >= minRatio {
		return candidate
	}
	clamped := clampContrast(candidate, background, minRatio, maxContrastClamp)
	if contrastRatio(clamped, background) >= minRatio {
		return clamped
	}
	return fallback
}

// clampContrast blends foreground toward black or white, whichever offers more
// contrast against background, until minRatio is met or amount runs out. It
// returns the best color it found; the caller decides whether that is enough.
func clampContrast(foreground, background color.Color, minRatio, amount float64) color.Color {
	if contrastRatio(foreground, background) >= minRatio {
		return foreground
	}
	target := colorWhite
	if contrastRatio(colorBlack, background) > contrastRatio(colorWhite, background) {
		target = colorBlack
	}
	best := foreground
	bestRatio := contrastRatio(foreground, background)
	for step := contrastStep; step <= amount+1e-9; step += contrastStep {
		candidate := mixColors(foreground, target, step)
		ratio := contrastRatio(candidate, background)
		if ratio > bestRatio {
			best, bestRatio = candidate, ratio
		}
		if ratio >= minRatio {
			return candidate
		}
	}
	return best
}

// contrastRatio is the WCAG contrast ratio between two colors.
func contrastRatio(a, b color.Color) float64 {
	luminanceA := relativeLuminance(a)
	luminanceB := relativeLuminance(b)
	if luminanceA < luminanceB {
		luminanceA, luminanceB = luminanceB, luminanceA
	}
	return (luminanceA + 0.05) / (luminanceB + 0.05)
}

// relativeLuminance is the WCAG relative luminance of c (alpha ignored).
func relativeLuminance(c color.Color) float64 {
	pixel := rgba(c)
	return 0.2126*channelLuminance(pixel.R) +
		0.7152*channelLuminance(pixel.G) +
		0.0722*channelLuminance(pixel.B)
}

// channelLuminance linearizes one 8-bit channel.
func channelLuminance(value uint8) float64 {
	channel := float64(value) / 255
	if channel <= 0.03928 {
		return channel / 12.92
	}
	return math.Pow((channel+0.055)/1.055, 2.4)
}

// rgba returns the opaque 8-bit channels of c.
func rgba(c color.Color) color.RGBA {
	if c == nil {
		return color.RGBA{A: 0xff}
	}
	converted, ok := color.RGBAModel.Convert(c).(color.RGBA)
	if !ok {
		return color.RGBA{A: 0xff}
	}
	return converted
}

// mixColors linearly mixes tint into base by amount and returns the opaque
// result. It mirrors mixColors in OmaircStyle.qml (and the copy still in
// internal/ui/style.go) for opaque colors.
func mixColors(base, tint color.Color, amount float64) color.RGBA {
	baseR, baseG, baseB, _ := base.RGBA()
	tintR, tintG, tintB, _ := tint.RGBA()
	mix := func(a, b uint32) uint8 {
		// RGBA returns 16-bit channels; >>8 is the 8-bit value for an opaque
		// color.
		from := float64(a >> 8)
		to := float64(b >> 8)
		value := from + (to-from)*amount
		if value < 0 {
			value = 0
		}
		if value > 255 {
			value = 255
		}
		return uint8(value + 0.5)
	}
	return color.RGBA{R: mix(baseR, tintR), G: mix(baseG, tintG), B: mix(baseB, tintB), A: 0xff}
}

// The fixed palette. The semantic colors and the dark nick palette are today's
// internal/ui/style.go constants; deriving them would drift from
// OmaircStyle.qml. fixedAccent/fixedMuted/fixedDim also seed Fallback.
var (
	fixedAccent  = mustHex("#7aa2f7")
	fixedMuted   = mustHex("#6c7086")
	fixedDim     = mustHex("#565f89")
	fixedGood    = mustHex("#9ece6a")
	fixedWarning = mustHex("#e0af68")
	fixedDanger  = mustHex("#f7768e")
	fixedMention = mustHex("#f7768e")
	fixedUnread  = mustHex("#e0af68")
	fixedPage    = mustHex("#1a1b26")

	colorWhite = color.RGBA{R: 0xff, G: 0xff, B: 0xff, A: 0xff}
	colorBlack = color.RGBA{A: 0xff}
)

// Qt's fallback colors from Backend::loadOmarchyTheme, used when colors.toml
// omits a key or no theme is installed.
var (
	darkBackgroundDefault  = mustHex("#101010")
	lightBackgroundDefault = mustHex("#ffffff")
	darkForegroundDefault  = mustHex("#eeeeee")
	lightForegroundDefault = mustHex("#222324")
	darkAccentDefault      = mustHex("#5584aa")
	lightAccentDefault     = mustHex("#2077b2")
	darkSelectionDefault   = mustHex("#186a9a")
	lightSelectionDefault  = mustHex("#2077b2")
)

// Nick palette per mode, mirroring nickPalette in OmaircStyle.qml. The dark
// entries are today's style.go constants. Eight fixed hue families; slot 0 is
// the theme accent in QML but the fixed fallback accent here, and slots 5-7 are
// the green/cyan/gold additions. Both modes stay parallel: slot i is the same
// family in either mode. The slice length is the hash modulus.
var (
	nickPaletteDark = []color.Color{
		fixedAccent,
		mustHex("#c099ff"),
		mustHex("#7fc8a9"),
		mustHex("#efb366"),
		mustHex("#ed8f9d"),
		mustHex("#9ece6a"),
		mustHex("#7dcfff"),
		mustHex("#ddd06e"),
	}
	nickPaletteLight = []color.Color{
		mustHex("#7aa2f7"),
		mustHex("#7950b8"),
		mustHex("#237a58"),
		mustHex("#a45f14"),
		mustHex("#b44355"),
		mustHex("#4d7c0f"),
		mustHex("#0f7b8f"),
		mustHex("#7c6a0a"),
	}
)
