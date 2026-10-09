package ui

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/version"
)

func TestFooterFitsNarrowWidths(t *testing.T) {
	versionLabel := version.Value
	contexts := []struct {
		name string
		prep func(*Model)
	}{
		{name: "composer", prep: func(m *Model) {}},
		{name: "jump overlay", prep: func(m *Model) { m.jump.open = true }},
	}

	for _, ctx := range contexts {
		for _, width := range []int{40, 50, 60, 80, 118} {
			m := seededModel(t)
			ctx.prep(m)
			m = resizeModel(t, m, width, 30)
			footer := m.footerView()
			plain := ansiPattern.ReplaceAllString(footer, "")

			if got := lipgloss.Width(footer); got != width {
				t.Fatalf("%s width %d: footer width = %d, want %d:\n%s", ctx.name, width, got, width, plain)
			}
			if !strings.Contains(plain, "●") {
				t.Fatalf("%s width %d: footer missing the connection mark:\n%s", ctx.name, width, plain)
			}
			if !strings.Contains(plain, "Ctrl+/") {
				t.Fatalf("%s width %d: footer missing Ctrl+/ tail:\n%s", ctx.name, width, plain)
			}
			if strings.HasSuffix(plain, "Ctrl+") {
				t.Fatalf("%s width %d: footer cut the Ctrl+/ key:\n%s", ctx.name, width, plain)
			}

			hasVersion := strings.Contains(plain, versionLabel)
			hasShortcuts := strings.Contains(plain, "Ctrl+/ shortcuts")

			switch width {
			case 40, 50, 60:
				if !hasShortcuts {
					t.Fatalf("%s width %d: footer must keep Ctrl+/ shortcuts:\n%s", ctx.name, width, plain)
				}
				if hasVersion {
					t.Fatalf("%s width %d: footer must drop the version before shortening:\n%s", ctx.name, width, plain)
				}
			case 80:
				if ctx.name == "composer" {
					for _, want := range []string{"Tab / Shift+Tab", "complete nick or channel"} {
						if !strings.Contains(plain, want) {
							t.Fatalf("%s width %d: footer missing %q:\n%s", ctx.name, width, want, plain)
						}
					}
					if hasVersion {
						t.Fatalf("%s width %d: composer footer must drop the version:\n%s", ctx.name, width, plain)
					}
				}
			case 118:
				if ctx.name == "composer" {
					if !strings.Contains(plain, "Ctrl+K") || !strings.Contains(plain, "jump") {
						t.Fatalf("%s width %d: composer footer must keep Ctrl+K jump:\n%s", ctx.name, width, plain)
					}
				}
			}

			if ctx.name == "jump overlay" && width == 60 {
				for _, want := range []string{"Enter select", "Esc dismiss", "Ctrl+/ shortcuts"} {
					if !strings.Contains(plain, want) {
						t.Fatalf("%s width %d: footer missing %q:\n%s", ctx.name, width, want, plain)
					}
				}
				if hasVersion {
					t.Fatalf("%s width %d: jump footer must drop the version:\n%s", ctx.name, width, plain)
				}
			}

			if !hasShortcuts {
				tail := plain
				if idx := strings.LastIndex(tail, "Ctrl+/"); idx >= 0 {
					tail = strings.TrimSpace(tail[idx:])
				}
				if tail != "Ctrl+/" {
					t.Fatalf("%s width %d: footer tail must be Ctrl+/ shortcuts or Ctrl+/ alone:\n%s", ctx.name, width, plain)
				}
				if strings.HasPrefix(tail, "Ctrl+/ "+versionLabel) || strings.HasSuffix(plain, "Ctrl+/ "+versionLabel) {
					t.Fatalf("%s width %d: key-only Ctrl+/ must not sit beside the version:\n%s", ctx.name, width, plain)
				}
			}
		}
	}
}
