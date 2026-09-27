package theme

import (
	"image/color"
	"testing"
)

func TestDeriveDefaultsDark(t *testing.T) {
	colors := Derive(Spec{Mode: ModeDark})
	assertHex(t, "Background", colors.Background, "#101010")
	assertHex(t, "Foreground", colors.Foreground, "#eeeeee")
	assertHex(t, "Accent", colors.Accent, "#5584aa")
	assertHex(t, "Selection", colors.Selection, "#186a9a")
	assertHex(t, "AvatarPage", colors.AvatarPage, "#101010")
	if colors.NickAvatarMix != 0.23 {
		t.Fatalf("NickAvatarMix = %v, want 0.23", colors.NickAvatarMix)
	}
	if colors.NickPalette != nickPaletteDark {
		t.Fatalf("NickPalette = %v, want the dark palette", colors.NickPalette)
	}
}

func TestDeriveDefaultsLight(t *testing.T) {
	colors := Derive(Spec{Mode: ModeLight})
	assertHex(t, "Background", colors.Background, "#ffffff")
	assertHex(t, "Foreground", colors.Foreground, "#222324")
	assertHex(t, "Accent", colors.Accent, "#2077b2")
	assertHex(t, "Selection", colors.Selection, "#2077b2")
	if colors.NickAvatarMix != 0.16 {
		t.Fatalf("NickAvatarMix = %v, want 0.16", colors.NickAvatarMix)
	}
	if colors.NickPalette != nickPaletteLight {
		t.Fatalf("NickPalette = %v, want the light palette", colors.NickPalette)
	}
}

func TestDeriveUsesSpecColors(t *testing.T) {
	colors := Derive(Spec{
		Background:    mustHex("#202020"),
		Foreground:    mustHex("#e0e0e0"),
		Accent:        mustHex("#4455aa"),
		Selection:     mustHex("#334488"),
		Mode:          ModeDark,
		HasBackground: true,
		HasForeground: true,
		HasAccent:     true,
		HasSelection:  true,
	})
	assertHex(t, "Background", colors.Background, "#202020")
	assertHex(t, "Foreground", colors.Foreground, "#e0e0e0")
	assertHex(t, "Accent", colors.Accent, "#4455aa")
	assertHex(t, "Selection", colors.Selection, "#334488")
}

func TestDeriveFlipsShadeDirectionWithMode(t *testing.T) {
	dark := Derive(Spec{
		Background:    mustHex("#202020"),
		Foreground:    mustHex("#e0e0e0"),
		Mode:          ModeDark,
		HasBackground: true,
		HasForeground: true,
	})
	light := Derive(Spec{
		Background:    mustHex("#e0e0e0"),
		Foreground:    mustHex("#202020"),
		Mode:          ModeLight,
		HasBackground: true,
		HasForeground: true,
	})

	if relativeLuminance(dark.Surface) <= relativeLuminance(dark.Background) {
		t.Error("dark Surface should be lighter than the background")
	}
	if relativeLuminance(dark.SurfaceRaised) <= relativeLuminance(dark.Surface) {
		t.Error("dark SurfaceRaised should be lighter than Surface")
	}
	if relativeLuminance(light.Surface) >= relativeLuminance(light.Background) {
		t.Error("light Surface should be darker than the background")
	}
	if relativeLuminance(light.SurfaceRaised) >= relativeLuminance(light.Surface) {
		t.Error("light SurfaceRaised should be darker than Surface")
	}
}

func TestDeriveClampsSecondaryTextForContrast(t *testing.T) {
	colors := Derive(Spec{
		Background:    mustHex("#000000"),
		Foreground:    mustHex("#555555"),
		Mode:          ModeDark,
		HasBackground: true,
		HasForeground: true,
	})

	if Hex(colors.TextMuted) == Hex(fixedMuted) {
		t.Fatal("expected the muted text to be clamped, not the fallback constant")
	}
	if ratio := contrastRatio(colors.TextMuted, colors.Background); ratio < mutedContrastRatio {
		t.Fatalf("TextMuted contrast = %.2f, want >= %.1f", ratio, mutedContrastRatio)
	}
	if ratio := contrastRatio(colors.TextDim, colors.Background); ratio < dimContrastRatio {
		t.Fatalf("TextDim contrast = %.2f, want >= %.1f", ratio, dimContrastRatio)
	}
}

func TestDeriveFallsBackWhenContrastCannotBeReached(t *testing.T) {
	colors := Derive(Spec{
		Background:    mustHex("#808080"),
		Foreground:    mustHex("#808080"),
		Mode:          ModeDark,
		HasBackground: true,
		HasForeground: true,
	})

	assertHex(t, "TextMuted", colors.TextMuted, Hex(fixedMuted))
	assertHex(t, "TextDim", colors.TextDim, Hex(fixedDim))
	if got, want := Hex(colors.TextMuted), "#6c7086"; got != want {
		t.Fatalf("TextMuted = %s, want %s", got, want)
	}
	if got, want := Hex(colors.TextDim), "#565f89"; got != want {
		t.Fatalf("TextDim = %s, want %s", got, want)
	}
}

func TestNickPaletteHasFiveEntries(t *testing.T) {
	var palette [5]color.Color = Derive(Spec{Mode: ModeDark}).NickPalette
	if len(palette) != 5 {
		t.Fatalf("len(NickPalette) = %d, want 5", len(palette))
	}
}
