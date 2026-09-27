package theme

import (
	"image/color"
	"testing"
)

// assertHex compares an image/color value against a "#rrggbb" literal.
func assertHex(t *testing.T, name string, got color.Color, want string) {
	t.Helper()
	if hex := Hex(got); hex != want {
		t.Errorf("%s = %s, want %s", name, hex, want)
	}
}
