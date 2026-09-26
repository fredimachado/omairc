package ui

import "charm.land/lipgloss/v2"

// Styles is the shell's small, calm palette. Every color is a fixed hex value:
// this file never reads the OS, the environment, or the terminal, and lipgloss
// down-samples the colors to whatever the terminal supports. A later phase
// feeds Omarchy theme colors in here.
type Styles struct {
	NetworkName  lipgloss.Style
	GroupLabel   lipgloss.Style
	Conversation lipgloss.Style
	Selected     lipgloss.Style
	MutedLine    lipgloss.Style
	MentionRow   lipgloss.Style
	Unread       lipgloss.Style

	Topic       lipgloss.Style
	Action      lipgloss.Style
	Event       lipgloss.Style
	Notice      lipgloss.Style
	MentionBody lipgloss.Style
	ConsoleLine lipgloss.Style

	MembersHeader lipgloss.Style
	MemberAway    lipgloss.Style

	Prompt   lipgloss.Style
	Composer lipgloss.Style
}

// The palette mirrors Omarchy's calm, low-contrast terminal look: one blue
// accent, a dim slate for secondary text, amber for unread, rose for mentions.
var (
	colorAccent  = lipgloss.Color("#7aa2f7")
	colorMuted   = lipgloss.Color("#6c7086")
	colorDim     = lipgloss.Color("#565f89")
	colorUnread  = lipgloss.Color("#e0af68")
	colorMention = lipgloss.Color("#f7768e")
)

// defaultStyles builds the shell's styles from the fixed palette.
func defaultStyles() Styles {
	base := lipgloss.NewStyle()
	return Styles{
		NetworkName:  base.Bold(true).Foreground(colorAccent),
		GroupLabel:   base.Foreground(colorDim).Faint(true),
		Conversation: base,
		Selected:     base.Bold(true),
		MutedLine:    base.Foreground(colorDim).Faint(true),
		MentionRow:   base.Foreground(colorMention),
		Unread:       base.Foreground(colorUnread),

		Topic:       base.Foreground(colorMuted),
		Action:      base.Foreground(colorMuted).Italic(true),
		Event:       base.Foreground(colorDim),
		Notice:      base.Foreground(colorMuted),
		MentionBody: base.Foreground(colorMention),
		ConsoleLine: base.Foreground(colorMuted),

		MembersHeader: base.Bold(true).Foreground(colorAccent),
		MemberAway:    base.Foreground(colorDim),

		Prompt:   base.Foreground(colorAccent),
		Composer: base,
	}
}
