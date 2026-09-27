package ui

import (
	"fmt"
	"image/color"
	"unicode"
	"unicode/utf16"

	"charm.land/bubbles/v2/textinput"
	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/theme"
)

// Styles is the shell's palette, built from the theme package's derived colors.
// buildStyles wraps a theme.Colors in lipgloss at the rendering edge; the theme
// package itself stays lipgloss-free and knows nothing about the widgets below.
//
// Colors is kept on the struct so leaf files can derive one-off styles (a
// background here, a highlight there) without editing this file. Every style
// below is a fixed render of those colors; a rebuild replaces the palette.
type Styles struct {
	// Colors is the source palette every style below is derived from.
	Colors theme.Colors

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

	// UnreadMark* paint the transcript's "New messages" boundary: the caption
	// and the accent-mixed rules on either side of it.
	UnreadMark     lipgloss.Style
	UnreadMarkRule lipgloss.Style

	// UnseenJump is the transcript header's "↓ new" hint, shown while rows are
	// waiting below the reader.
	UnseenJump lipgloss.Style

	MembersHeader lipgloss.Style

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

	// ComposerField is the fill behind the composer's one-row surface block,
	// and ComposerInput is the composer's own input style set: the shared
	// Input look plus that same surface fill, so the field reads as one block
	// under the transcript. The overlay filters keep Input (no fill).
	ComposerField lipgloss.Style
	ComposerInput textinput.Styles

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

	// F1 visual-overhaul chrome. These are additive: the fields above keep
	// their names and purposes, and every render is derived from theme.Colors.
	AppTitle      lipgloss.Style
	Panel         lipgloss.Style
	PanelFocused  lipgloss.Style
	PanelTitle    lipgloss.Style
	SectionHeader lipgloss.Style
	Badge         lipgloss.Style
	BadgeUnread   lipgloss.Style
	BadgeMention  lipgloss.Style
	BadgeMuted    lipgloss.Style
	Keycap        lipgloss.Style
	Footer        lipgloss.Style
	FooterHint    lipgloss.Style
	Divider       lipgloss.Style
	Time          lipgloss.Style
	Empty         lipgloss.Style
	Shadow        lipgloss.Style
	Dimmed        lipgloss.Style
	StatusOK      lipgloss.Style
	StatusWarn    lipgloss.Style
	StatusErr     lipgloss.Style
	Link          lipgloss.Style

	// Input is the shared text-input style set. newTextInput applies it so the
	// overlay filters and the composer render the same theme-driven field.
	Input textinput.Styles
}

// fixedColors is the fixed Tokyo-Night palette the TUI has always rendered.
// Nick colors, the identicon page fill, and the avatar mix amount are fixed
// presentation shared with OmaircStyle.qml, so they read Fallback() rather than
// the live theme.
var (
	fixedColors   = theme.Fallback()
	nickPalette   = fixedColors.NickPalette
	nickAvatarMix = fixedColors.NickAvatarMix
	colorPage     = fixedColors.Page
)

// avatarFill is the identicon background: nickColor mixed into colorPage. Since
// there is no terminal page color, this approximates the QML
// mixColors(pageColor, nickColor, nickAvatarMix).
func avatarFill(nick string) color.Color {
	return mixColors(colorPage, nickColor(nick), nickAvatarMix)
}

// nickPaletteIndex mirrors nickPaletteIndex in OmaircStyle.qml: the sum of the
// nick's UTF-16 code units modulo the palette length. UTF-16 code units (not
// runes) match JavaScript's charCodeAt for non-ASCII nicks.
func nickPaletteIndex(nick string) int {
	hash := 0
	for _, unit := range utf16.Encode([]rune(nick)) {
		hash = (hash + int(unit)) % len(nickPalette)
	}
	return hash
}

// nickColor is the palette color for a nick. Nick colors are fixed
// presentation, never theme-derived, so this reads theme.Colors.NickPalette
// from the fixed fallback palette.
func nickColor(nick string) color.Color {
	return nickPalette[nickPaletteIndex(nick)]
}

// initials is the identicon's single letter: the first rune upper-cased, or "?"
// when the nick is empty. It mirrors initials in OmaircStyle.qml.
func initials(nick string) string {
	for _, r := range nick {
		return string(unicode.ToUpper(r))
	}
	return "?"
}

// mixColors linearly mixes tint into base by amount and formats the result as
// "#rrggbb". It mirrors mixColors in OmaircStyle.qml for opaque colors. The
// theme package keeps its own unexported copy; this thin local helper is only
// for one-off presentation mixes such as the identicon fill.
func mixColors(base, tint color.Color, amount float64) color.Color {
	baseR, baseG, baseB, _ := base.RGBA()
	tintR, tintG, tintB, _ := tint.RGBA()
	mix := func(a, b uint32) int {
		// RGBA returns 16-bit channels; >>8 is the 8-bit value for an opaque
		// color.
		from := float64(a >> 8)
		to := float64(b >> 8)
		value := from + (to-from)*amount
		if value < 0 {
			value = 0
		}
		if value > 255 {
			value = 255
		}
		return int(value + 0.5)
	}
	return lipgloss.Color(fmt.Sprintf("#%02x%02x%02x",
		mix(baseR, tintR), mix(baseG, tintG), mix(baseB, tintB)))
}

// defaultStyles builds the shell's styles from the fixed fallback palette.
func defaultStyles() Styles {
	return buildStyles(theme.Fallback())
}

// buildStyles derives every style from colors. The theme package owns the
// palette; this file is the only place lipgloss sees it.
func buildStyles(colors theme.Colors) Styles {
	base := lipgloss.NewStyle()
	card := base.Border(lipgloss.RoundedBorder()).Padding(0, 1)
	input := textinput.DefaultDarkStyles()
	input.Focused.Text = base.Foreground(colors.Foreground)
	input.Focused.Placeholder = base.Foreground(colors.TextDim)
	input.Focused.Suggestion = base.Foreground(colors.TextDim)
	input.Focused.Prompt = base.Foreground(colors.Accent)
	input.Blurred.Text = base.Foreground(colors.TextMuted)
	input.Blurred.Placeholder = base.Foreground(colors.TextDim)
	input.Blurred.Suggestion = base.Foreground(colors.TextDim)
	input.Blurred.Prompt = base.Foreground(colors.TextDim)
	input.Cursor.Color = colors.Accent
	composerField := base.Background(colors.Surface)
	return Styles{
		Colors: colors,

		NetworkName:        base.Bold(true).Foreground(colors.Accent),
		NetworkNameFocused: base.Bold(true).Foreground(colors.Unread),
		GroupLabel:         base.Foreground(colors.TextDim).Faint(true),
		Conversation:       base,
		Selected:           base.Bold(true),
		MutedLine:          base.Foreground(colors.TextDim).Faint(true),
		MentionRow:         base.Foreground(colors.Mention),
		Unread:             base.Foreground(colors.Unread),

		Topic:       base.Foreground(colors.TextMuted),
		Action:      base.Foreground(colors.TextMuted).Italic(true),
		Event:       base.Foreground(colors.TextDim),
		Notice:      base.Foreground(colors.TextMuted),
		MentionBody: base.Foreground(colors.Mention),
		ConsoleLine: base.Foreground(colors.TextMuted),
		FindMatch:   base.Reverse(true).Bold(true),

		UnreadMark:     base.Foreground(colors.UnreadMark).Bold(true),
		UnreadMarkRule: base.Foreground(colors.UnreadMarkRule),
		UnseenJump:     base.Foreground(colors.Accent).Bold(true),

		MembersHeader: base.Bold(true).Foreground(colors.Accent),

		MemberPresenceOnline: base.Foreground(colors.Good),
		MemberPresenceAway:   base.Foreground(colors.Unread),
		MemberBot:            base.Foreground(colors.TextMuted),
		MemberAccount:        base.Foreground(colors.TextMuted),
		MemberStatus:         base.Foreground(colors.TextMuted),
		MemberTyping:         base.Foreground(colors.TextMuted),

		PeopleCount: base.Bold(true).Foreground(colors.TextMuted),

		Prompt:        base.Foreground(colors.Accent),
		Composer:      base,
		ComposerField: composerField,
		ComposerInput: composerInputStyles(input, colors.Surface),

		Dimmer: base.Foreground(colors.TextDim).Faint(true),

		SheetCard:        card,
		SheetTitle:       base.Bold(true).Foreground(colors.Accent),
		SheetTab:         base.Foreground(colors.TextDim),
		SheetTabActive:   base.Bold(true).Foreground(colors.Accent).Underline(true),
		SheetLabel:       base.Foreground(colors.TextDim).Faint(true),
		SheetRow:         base,
		SheetRowFocused:  base.Bold(true).Foreground(colors.Accent),
		SheetField:       base.Foreground(colors.TextMuted),
		SheetFieldActive: base.Bold(true),
		SheetToggleOn:    base.Foreground(colors.Good),
		SheetProblem:     base.Foreground(colors.Mention),
		SheetButton:      base,
		SheetButtonMuted: base.Foreground(colors.TextDim).Faint(true),
		SheetButtonFocus: base.Bold(true).Foreground(colors.Accent),

		JumpCard:     card,
		JumpQuery:    base.Foreground(colors.Accent),
		JumpRow:      base,
		JumpSelected: base.Bold(true),
		JumpEmpty:    base.Foreground(colors.TextDim).Faint(true),

		SlashRow:      base.Foreground(colors.TextMuted),
		SlashSelected: base.Bold(true).Foreground(colors.Accent),

		AppTitle:      base.Bold(true).Foreground(colors.Accent),
		Panel:         base.Border(lipgloss.RoundedBorder()).BorderForeground(colors.Border),
		PanelFocused:  base.Border(lipgloss.RoundedBorder()).BorderForeground(colors.BorderFocus),
		PanelTitle:    base.Bold(true).Foreground(colors.Foreground),
		SectionHeader: base.Bold(true).Foreground(colors.TextMuted),
		Badge:         base.Bold(true).Foreground(colors.Foreground).Background(colors.SurfaceRaised),
		BadgeUnread:   base.Bold(true).Foreground(colors.Unread),
		BadgeMention:  base.Bold(true).Foreground(colors.Mention),
		BadgeMuted:    base.Foreground(colors.TextDim),
		Keycap:        base.Foreground(colors.Foreground).Background(colors.SurfaceRaised),
		Footer:        base.Foreground(colors.TextMuted),
		FooterHint:    base.Foreground(colors.TextDim),
		Divider:       base.Foreground(colors.Border),
		Time:          base.Foreground(colors.TextDim),
		Empty:         base.Foreground(colors.TextDim).Faint(true),
		Shadow:        base.Foreground(colors.Border),
		Dimmed:        base.Faint(true),
		StatusOK:      base.Foreground(colors.Good),
		StatusWarn:    base.Foreground(colors.Warning),
		StatusErr:     base.Foreground(colors.Danger),
		Link:          base.Foreground(colors.Accent).Underline(true),

		Input: input,
	}
}

// composerInputStyles derives the composer's input style set from the shared
// one: the same prompt, placeholder, and caret, plus the surface fill on every
// state so the field's own glyphs carry the block background instead of
// punching a window-coloured hole in it. The overlay filters keep the
// unfilled Input set.
func composerInputStyles(input textinput.Styles, surface color.Color) textinput.Styles {
	fill := func(style lipgloss.Style) lipgloss.Style {
		return style.Background(surface)
	}
	input.Focused.Text = fill(input.Focused.Text)
	input.Focused.Placeholder = fill(input.Focused.Placeholder)
	input.Focused.Suggestion = fill(input.Focused.Suggestion)
	input.Focused.Prompt = fill(input.Focused.Prompt)
	input.Blurred.Text = fill(input.Blurred.Text)
	input.Blurred.Placeholder = fill(input.Blurred.Placeholder)
	input.Blurred.Suggestion = fill(input.Blurred.Suggestion)
	input.Blurred.Prompt = fill(input.Blurred.Prompt)
	return input
}

// newTextInput builds a text input from the shell styles. Leaf overlays adopt
// it so the placeholder, prompt, and theme-driven input styles live in one
// place instead of each call site rebuilding them.
func newTextInput(styles Styles, placeholder string, prompt string) textinput.Model {
	input := textinput.New()
	input.SetStyles(styles.Input)
	input.Placeholder = placeholder
	input.Prompt = prompt
	return input
}
