package ui

// Tests that the shell leaves the terminal's own background visible: View must
// not set OSC 10/11, and lipgloss must not paint the theme page color across
// empty transcript or sidebar rows.

import (
	"strings"
	"testing"
)

// TestViewLeavesTerminalColorsUnset pins the OSC 10/11 choice: Omarchy shares
// palette with the terminal, but setting foreground would still reset the
// default via OSC 10 the same way background resets OSC 11. Both stay unset so
// quit does not leave a stale default.
func TestViewLeavesTerminalColorsUnset(t *testing.T) {
	m := seededModel(t)
	view := m.View()
	if view.BackgroundColor != nil {
		t.Fatalf("BackgroundColor = %v, want unset (nil)", view.BackgroundColor)
	}
	if view.ForegroundColor != nil {
		t.Fatalf("ForegroundColor = %v, want unset (nil)", view.ForegroundColor)
	}
}

// TestSeededRenderHasNoThemeBackgroundFill parses the demo frame and rejects
// Colors.Background as an SGR background on any row. A page-wide lipgloss fill
// would paint every line while deliberate surfaces (topic band, badges,
// selection, mention wash, composer frame, overlay shadow) use other colors.
func TestSeededRenderHasNoThemeBackgroundFill(t *testing.T) {
	m := seededModel(t)
	windowBG := backgroundParams(m.styles.Colors.Background)
	for index, raw := range strings.Split(m.View().Content, "\n") {
		if strings.Contains(raw, windowBG) {
			t.Fatalf("line %d carries the theme window background (page fill must stay terminal-native):\n%s", index, raw)
		}
	}
}
