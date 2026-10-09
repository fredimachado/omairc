package ui

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
)

func TestFooterFitsNarrowWidths(t *testing.T) {
	widths := []int{40, 50, 60, 80, 118}
	contexts := []struct {
		name string
		prep func(*Model)
		wide []string
	}{
		{
			name: "composer",
			prep: func(m *Model) {},
			wide: []string{"Enter", "send", "Tab / Shift+Tab", "complete nick or channel"},
		},
		{
			name: "jump overlay",
			prep: func(m *Model) { m.jump.open = true },
			wide: []string{"Enter select", "Esc dismiss"},
		},
	}

	for _, ctx := range contexts {
		for _, width := range widths {
			m := seededModel(t)
			ctx.prep(m)
			m = resizeModel(t, m, width, 30)
			footer := m.footerView()
			plain := ansiPattern.ReplaceAllString(footer, "")

			if got := lipgloss.Width(footer); got != width {
				t.Fatalf("%s width %d: footer width = %d, want %d:\n%s", ctx.name, width, got, width, plain)
			}
			if !strings.Contains(plain, "Ctrl+/") {
				t.Fatalf("%s width %d: footer missing Ctrl+/ tail:\n%s", ctx.name, width, plain)
			}
			if strings.HasSuffix(plain, "Ctrl+") {
				t.Fatalf("%s width %d: footer cut the Ctrl+/ key:\n%s", ctx.name, width, plain)
			}
			if strings.HasSuffix(strings.TrimRight(plain, " "), "Tab") && !strings.Contains(plain, "complete") {
				t.Fatalf("%s width %d: footer cut the Tab hint:\n%s", ctx.name, width, plain)
			}
			if !strings.Contains(plain, "●") {
				t.Fatalf("%s width %d: footer missing the connection mark:\n%s", ctx.name, width, plain)
			}
			tail := plain
			if idx := strings.LastIndex(tail, "Ctrl+/"); idx >= 0 {
				tail = tail[idx:]
			}
			if strings.HasPrefix(tail, "Ctrl+/ shortcuts") {
				continue
			}
			if tail == "Ctrl+/" || strings.HasPrefix(tail, "Ctrl+/ ") {
				continue
			}
			t.Fatalf("%s width %d: footer tail is not Ctrl+/ shortcuts or Ctrl+/ alone:\n%s", ctx.name, width, plain)

			if width >= 80 {
				for _, want := range ctx.wide {
					if !strings.Contains(plain, want) {
						t.Fatalf("%s width %d: footer missing %q at wide size:\n%s", ctx.name, width, want, plain)
					}
				}
			}
		}
	}
}
