package theme

import (
	"bufio"
	"image/color"
	"io"
	"os"
	"strconv"
	"strings"
)

// ParseColorsTOML parses an Omarchy colors.toml, mirroring
// Backend::loadOmarchyTheme in src/backend.cpp: blank lines and lines starting
// with "#" are skipped, each remaining line is split on the first "=", and the
// value's surrounding single or double quotes are stripped. Only the keys
// mode/background/foreground/accent/selection are read; unknown keys and
// malformed values are ignored, exactly as Qt ignores them.
//
// The returned Spec.Mode is resolved: a recognized "mode" wins, otherwise the
// mode is inferred from the background's luminance. An empty or missing file
// yields a dark spec with no presence flags, which Derive turns into the Qt
// defaults.
func ParseColorsTOML(r io.Reader) (Spec, error) {
	spec := Spec{Mode: ModeDark}
	if r == nil {
		return spec, nil
	}

	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 0, 4096), 1<<20)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		equals := strings.IndexByte(line, '=')
		if equals < 0 {
			continue
		}
		key := strings.TrimSpace(line[:equals])
		value := unquote(strings.TrimSpace(line[equals+1:]))

		switch key {
		case "mode":
			switch value {
			case ModeDark, ModeLight:
				spec.Mode = value
				spec.HasMode = true
			}
		case "background":
			if parsed, ok := parseHexColor(value); ok {
				spec.Background = parsed
				spec.HasBackground = true
			}
		case "foreground":
			if parsed, ok := parseHexColor(value); ok {
				spec.Foreground = parsed
				spec.HasForeground = true
			}
		case "accent":
			if parsed, ok := parseHexColor(value); ok {
				spec.Accent = parsed
				spec.HasAccent = true
			}
		case "selection":
			if parsed, ok := parseHexColor(value); ok {
				spec.Selection = parsed
				spec.HasSelection = true
			}
		}
	}
	if err := scanner.Err(); err != nil {
		return spec, err
	}

	// mode wins; else infer from the background, matching the Qt loader. A spec
	// with neither stays dark.
	if !spec.HasMode && spec.HasBackground {
		spec.Mode = inferMode(spec.Background)
	}
	return spec, nil
}

// Load derives the palette from the current Omarchy colors.toml. A missing,
// unreadable, or malformed file yields Fallback(), matching
// Backend::loadOmarchyTheme when no theme is installed.
func Load() Colors {
	colors, _ := loadColors(colorsPath())
	return colors
}

// loadColors reads path and derives the palette. found reports whether a theme
// file was successfully read and parsed; when it is false, colors is Fallback().
func loadColors(path string) (colors Colors, found bool) {
	if path == "" {
		return Fallback(), false
	}
	file, err := os.Open(path)
	if err != nil {
		return Fallback(), false
	}
	defer file.Close()

	spec, err := ParseColorsTOML(file)
	if err != nil {
		return Fallback(), false
	}
	return Derive(spec), true
}

// inferMode mirrors the naive luminance test in Backend::loadOmarchyTheme:
// a weighted average of the gamma-encoded channels, dark below 0.5.
func inferMode(c color.Color) string {
	pixel := rgba(c)
	luminance := 0.299*float64(pixel.R)/255 +
		0.587*float64(pixel.G)/255 +
		0.114*float64(pixel.B)/255
	if luminance < 0.5 {
		return ModeDark
	}
	return ModeLight
}

// unquote strips one pair of matching surrounding single or double quotes,
// matching the Qt loader. Unquoted values pass through unchanged.
func unquote(value string) string {
	if len(value) >= 2 {
		first, last := value[0], value[len(value)-1]
		if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
			return value[1 : len(value)-1]
		}
	}
	return value
}

// parseHexColor parses "#rrggbb" (the leading "#" is optional). It reports false
// for anything else, so a malformed value is ignored rather than misread.
func parseHexColor(value string) (color.Color, bool) {
	digits := strings.TrimPrefix(value, "#")
	if len(digits) != 6 {
		return nil, false
	}
	parsed, err := strconv.ParseUint(digits, 16, 32)
	if err != nil {
		return nil, false
	}
	return color.RGBA{
		R: uint8(parsed >> 16),
		G: uint8(parsed >> 8),
		B: uint8(parsed),
		A: 0xff,
	}, true
}

// mustHex parses a package color literal and panics on a typo. It is only used
// for the constants above, so a panic means the source is wrong.
func mustHex(value string) color.Color {
	parsed, ok := parseHexColor(value)
	if !ok {
		panic("theme: invalid color literal " + value)
	}
	return parsed
}
