package controller

import "testing"

// TestStyledRunsExposesColors proves the shell can read palette and RGB runs
// through the controller without importing internal/irc.
func TestStyledRunsExposesColors(t *testing.T) {
	c := New()

	runs := c.StyledRuns("\x0304red\x03 plain")
	if len(runs) != 2 {
		t.Fatalf("StyledRuns len = %d, want 2", len(runs))
	}
	if runs[0].Text != "red" || runs[0].Style.Fg == nil || !runs[0].Style.Fg.IsPalette() ||
		runs[0].Style.Fg.PaletteIndex() != 4 {
		t.Fatalf("first run = %#v, want palette 4 text red", runs[0])
	}
	if runs[1].Text != " plain" || runs[1].Style.Fg != nil {
		t.Fatalf("second run = %#v, want plain with cleared FG", runs[1])
	}

	hex := c.StyledRuns("\x04FF0080x")
	if len(hex) != 1 || hex[0].Style.Fg == nil || hex[0].Style.Fg.IsPalette() {
		t.Fatalf("hex run = %#v, want one RGB foreground", hex)
	}
	r, g, b := hex[0].Style.Fg.RGB()
	if r != 255 || g != 0 || b != 128 {
		t.Fatalf("RGB = %d,%d,%d, want 255,0,128", r, g, b)
	}
}

// TestHasIrcStyleAndColorCodes gates the transcript styled path: color-only
// bodies report color codes, emphasis-only does not, and HasIrcStyle is either.
func TestHasIrcStyleAndColorCodes(t *testing.T) {
	c := New()

	if c.HasIrcColorCodes("plain") || c.HasIrcStyle("plain") {
		t.Fatal("plain text must not report style or color codes")
	}
	if !c.HasIrcColorCodes("\x034red") || !c.HasIrcStyle("\x034red") {
		t.Fatal("color-only body must report color codes and style")
	}
	if c.HasIrcColorCodes("\x02bold") {
		t.Fatal("emphasis-only must not report color codes")
	}
	if !c.HasIrcStyle("\x02bold") {
		t.Fatal("emphasis-only must report HasIrcStyle")
	}
	if !c.HasIrcColorCodes("\x04FF0000x") {
		t.Fatal("hex color must report color codes")
	}
}

// TestPrefColorsEnabledMirrorsShowIrcColors keeps the view helper in lockstep
// with the stored /pref colors toggle.
func TestPrefColorsEnabledMirrorsShowIrcColors(t *testing.T) {
	c := New()
	if !c.PrefColorsEnabled() || !c.ShowIrcColors() {
		t.Fatal("default PrefColorsEnabled must be true")
	}
	c.SetShowIrcColors(false)
	if c.PrefColorsEnabled() || c.ShowIrcColors() {
		t.Fatal("SetShowIrcColors(false) must clear PrefColorsEnabled")
	}
}
