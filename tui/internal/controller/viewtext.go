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

// HasIrcEmphasis reports whether the raw body carries a bold, italic, or
// underline control code. It mirrors IrcTextFormatter::hasIrcEmphasis.
func (c *Controller) HasIrcEmphasis(text string) bool {
	return irc.HasIrcEmphasis(text)
}

// EmphasizedRuns splits a transcript body into styled runs, stripping mIRC
// colors and the dropped reverse/monospace/strikethrough codes. It mirrors
// IrcTextFormatter::emphasizedIrcText as segments.
func (c *Controller) EmphasizedRuns(text string) []EmphasisRun {
	return irc.EmphasizedSegments(text)
}

// PrefAvatarsEnabled reports the stored /pref avatars toggle. The shell uses it
// to decide between the identicon and a rasterized peer avatar.
func (c *Controller) PrefAvatarsEnabled() bool {
	if c.prefs == nil {
		return true
	}
	return c.prefs.Enabled(irc.PrefAvatars)
}
