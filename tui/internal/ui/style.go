package ui

import "charm.land/lipgloss/v2"

// Styles is the shell's small, calm palette. Every color is a fixed hex value:
// this file never reads the OS, the environment, or the terminal, and lipgloss
// down-samples the colors to whatever the terminal supports. A later phase
// feeds Omarchy theme colors in here.
type Styles struct {
	NetworkName        lipgloss.Style
	NetworkNameFocused lipgloss.Style
	GroupLabel         lipgloss.Style
	Conversation       lipgloss.Style
	Selected           lipgloss.Style
	MutedLine          lipgloss.Style
	MentionRow         lipgloss.Style
	Unread             lipgloss.Style

	Topic       lipgloss.Style
	Action      lipgloss.Style
	Event       lipgloss.Style
	Notice      lipgloss.Style
	MentionBody lipgloss.Style
	ConsoleLine lipgloss.Style
	FindMatch   lipgloss.Style

	MembersHeader  lipgloss.Style
	MemberAway     lipgloss.Style
	MemberSelected lipgloss.Style

	// Phase 8 member chrome. The presence dot is green when available and
	// amber when away; the bot mark, account, status subline, and typing
	// ellipsis stay muted so the nick remains the loudest thing in the row.
	MemberPresenceOnline lipgloss.Style
	MemberPresenceAway   lipgloss.Style
	MemberBot            lipgloss.Style
	MemberAccount        lipgloss.Style
	MemberStatus         lipgloss.Style
	MemberTyping         lipgloss.Style

	// PeopleCount is the transcript header's "N PEOPLE" label beside the
	// topic (ConversationColumn.qml's people control).
	PeopleCount lipgloss.Style

	Prompt   lipgloss.Style
	Composer lipgloss.Style

	Dimmer lipgloss.Style

	SheetCard        lipgloss.Style
	SheetTitle       lipgloss.Style
	SheetTab         lipgloss.Style
	SheetTabActive   lipgloss.Style
	SheetLabel       lipgloss.Style
	SheetRow         lipgloss.Style
	SheetRowFocused  lipgloss.Style
	SheetField       lipgloss.Style
	SheetFieldActive lipgloss.Style
	SheetToggleOn    lipgloss.Style
	SheetProblem     lipgloss.Style
	SheetButton      lipgloss.Style
	SheetButtonMuted lipgloss.Style
	SheetButtonFocus lipgloss.Style

	JumpCard     lipgloss.Style
	JumpQuery    lipgloss.Style
	JumpRow      lipgloss.Style
	JumpSelected lipgloss.Style
	JumpEmpty    lipgloss.Style

	SlashRow      lipgloss.Style
	SlashSelected lipgloss.Style
}

// The palette mirrors Omarchy's calm, low-contrast terminal look: one blue
// accent, a dim slate for secondary text, amber for unread, rose for mentions.
var (
	colorAccent  = lipgloss.Color("#7aa2f7")
	colorMuted   = lipgloss.Color("#6c7086")
	colorDim     = lipgloss.Color("#565f89")
	colorUnread  = lipgloss.Color("#e0af68")
	colorMention = lipgloss.Color("#f7768e")
	colorGood    = lipgloss.Color("#9ece6a")
)

// defaultStyles builds the shell's styles from the fixed palette.
func defaultStyles() Styles {
	base := lipgloss.NewStyle()
	card := base.Border(lipgloss.RoundedBorder()).Padding(0, 1)
	return Styles{
		NetworkName:        base.Bold(true).Foreground(colorAccent),
		NetworkNameFocused: base.Bold(true).Foreground(colorUnread),
		GroupLabel:         base.Foreground(colorDim).Faint(true),
		Conversation:       base,
		Selected:           base.Bold(true),
		MutedLine:          base.Foreground(colorDim).Faint(true),
		MentionRow:         base.Foreground(colorMention),
		Unread:             base.Foreground(colorUnread),

		Topic:       base.Foreground(colorMuted),
		Action:      base.Foreground(colorMuted).Italic(true),
		Event:       base.Foreground(colorDim),
		Notice:      base.Foreground(colorMuted),
		MentionBody: base.Foreground(colorMention),
		ConsoleLine: base.Foreground(colorMuted),
		FindMatch:   base.Reverse(true).Bold(true),

		MembersHeader:  base.Bold(true).Foreground(colorAccent),
		MemberAway:     base.Foreground(colorDim),
		MemberSelected: base.Bold(true).Foreground(colorUnread),

		MemberPresenceOnline: base.Foreground(colorGood),
		MemberPresenceAway:   base.Foreground(colorUnread),
		MemberBot:            base.Foreground(colorMuted),
		MemberAccount:        base.Foreground(colorMuted),
		MemberStatus:         base.Foreground(colorMuted),
		MemberTyping:         base.Foreground(colorMuted),

		PeopleCount: base.Bold(true).Foreground(colorMuted),

		Prompt:   base.Foreground(colorAccent),
		Composer: base,

		Dimmer: base.Foreground(colorDim).Faint(true),

		SheetCard:        card,
		SheetTitle:       base.Bold(true).Foreground(colorAccent),
		SheetTab:         base.Foreground(colorDim),
		SheetTabActive:   base.Bold(true).Foreground(colorAccent).Underline(true),
		SheetLabel:       base.Foreground(colorDim).Faint(true),
		SheetRow:         base,
		SheetRowFocused:  base.Bold(true).Foreground(colorAccent),
		SheetField:       base.Foreground(colorMuted),
		SheetFieldActive: base.Bold(true),
		SheetToggleOn:    base.Foreground(colorGood),
		SheetProblem:     base.Foreground(colorMention),
		SheetButton:      base,
		SheetButtonMuted: base.Foreground(colorDim).Faint(true),
		SheetButtonFocus: base.Bold(true).Foreground(colorAccent),

		JumpCard:     card,
		JumpQuery:    base.Foreground(colorAccent),
		JumpRow:      base,
		JumpSelected: base.Bold(true),
		JumpEmpty:    base.Foreground(colorDim).Faint(true),

		SlashRow:      base.Foreground(colorMuted),
		SlashSelected: base.Bold(true).Foreground(colorAccent),
	}
}
