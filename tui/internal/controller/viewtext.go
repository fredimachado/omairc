package controller

import "github.com/fredimachado/omairc/tui/internal/irc"

// This file is the small text/preference surface the shell reads that must not
// pull internal/irc policy into internal/ui (bin/check-conventions forbids the
// view from importing internal/irc). The controller owns the policy call and
// the view consumes plain strings or aliases.

// EmphasisRun is one styled run of a transcript body, aliasing the core
// Segment so the shell can read Bold/Italic/Underline/Text without importing
// internal/irc.
type EmphasisRun = irc.Segment

// StyledRun is one fully styled stretch of a transcript body, including
// optional mIRC foreground and background. It aliases irc.StyledSegment so the
// shell never imports internal/irc.
type StyledRun = irc.StyledSegment

// RunColor is a mIRC palette index or RGB triple from a styled run. Presentation
// RGB for palette indices lives in the UI layer.
type RunColor = irc.IrcColor

// HasIrcEmphasis reports whether the raw body carries a bold, italic, or
// underline control code. It mirrors IrcTextFormatter::hasIrcEmphasis.
func (c *Controller) HasIrcEmphasis(text string) bool {
	return irc.HasIrcEmphasis(text)
}

// HasIrcColorCodes reports whether the raw body carries a mIRC palette or hex
// color control. Color-only bodies need this to enter the styled path when
// /pref colors is on.
func (c *Controller) HasIrcColorCodes(text string) bool {
	return irc.HasIrcColorCodes(text)
}

// HasIrcStyle reports whether the raw body carries emphasis or color controls.
func (c *Controller) HasIrcStyle(text string) bool {
	return irc.HasIrcStyle(text)
}

// EmphasizedRuns splits a transcript body into styled runs, stripping mIRC
// colors and the dropped reverse/monospace/strikethrough codes. It mirrors
// IrcTextFormatter::emphasizedIrcText as segments.
func (c *Controller) EmphasizedRuns(text string) []EmphasisRun {
	return irc.EmphasizedSegments(text)
}

// StyledRuns splits a transcript body into fully styled runs, including mIRC
// FG/BG when present. Concatenating Text equals PlainIrcText. Reverse,
// monospace, and strikethrough may appear on the style flags but the shell
// does not paint them.
func (c *Controller) StyledRuns(text string) []StyledRun {
	return irc.StyledSegments(text)
}

// PrefAvatarsEnabled reports the stored /pref avatars toggle. The shell uses it
// to decide between the identicon and a rasterized peer avatar.
func (c *Controller) PrefAvatarsEnabled() bool {
	if c.prefs == nil {
		return true
	}
	return c.prefs.Enabled(irc.PrefAvatars)
}

// PrefColorsEnabled reports the stored /pref colors toggle. The shell uses it
// to decide whether mIRC foreground and background paint in the transcript.
func (c *Controller) PrefColorsEnabled() bool {
	if c.prefs == nil {
		return true
	}
	return c.prefs.Enabled(irc.PrefColors)
}
