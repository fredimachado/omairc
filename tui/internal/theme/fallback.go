package theme

// Fallback returns the fixed Tokyo-Night palette the TUI has always used,
// ported from the constants in internal/ui/style.go. It is what the shell
// renders when no Omarchy theme is installed (or the file is unreadable), so it
// must keep today's exact literals: the semantic colors, the dark nick palette,
// the page color, and the secondary text.
//
// Structurally it reuses Derive so the surfaces and borders match the dark
// theme math, but the muted/dim text is restored to style.go's constants rather
// than the contrast-clamped derivation.
func Fallback() Colors {
	colors := Derive(Spec{
		Background:    fixedPage,
		HasBackground: true,
		Accent:        fixedAccent,
		HasAccent:     true,
		Mode:          ModeDark,
	})
	colors.TextMuted = fixedMuted
	colors.TextDim = fixedDim
	return colors
}
