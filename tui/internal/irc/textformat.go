package irc

// IrcColor is a mIRC palette entry (0-15) or a 24-bit color from a \x04 hex
// code. Presentation RGB for palette indices lives in the UI layer, not here.
type IrcColor struct {
	kind    ircColorKind
	index   uint8
	r, g, b uint8
}

type ircColorKind uint8

const (
	ircColorPalette ircColorKind = iota
	ircColorRGB
)

// PaletteColor is a mIRC palette index. Values above 15 are not applied by the
// walker; constructing one is still valid for tests and callers.
func PaletteColor(index uint8) IrcColor {
	return IrcColor{kind: ircColorPalette, index: index}
}

// RGBColor is a 24-bit color from a valid \x04 sequence.
func RGBColor(r, g, b uint8) IrcColor {
	return IrcColor{kind: ircColorRGB, r: r, g: g, b: b}
}

// IsPalette reports whether c is a palette index rather than RGB.
func (c IrcColor) IsPalette() bool { return c.kind == ircColorPalette }

// PaletteIndex returns the palette entry when IsPalette is true.
func (c IrcColor) PaletteIndex() uint8 { return c.index }

// RGB returns the red, green, and blue channels when c is RGB.
func (c IrcColor) RGB() (uint8, uint8, uint8) { return c.r, c.g, c.b }

// Style is every attribute a control code can set. A zero Style is plain text.
// Reverse, monospace, and strikethrough are tracked for parity with Rust
// styled_runs; the TUI does not render them yet.
type Style struct {
	Bold, Italic, Underline       bool
	Reverse, Monospace            bool
	Strikethrough                 bool
	Fg, Bg                        *IrcColor
}

// StyledSegment is one styled stretch of the plain text, including optional
// foreground and background colors. Concatenating Text across the slices from
// StyledSegments equals PlainIrcText.
type StyledSegment struct {
	Style Style
	Text  string
}

// Segment is the emphasis-only projection of a styled run: bold, italic, and
// underline. The transcript paints FG/BG through StyledSegments when
// showIrcColors is on; callers that only need emphasis use EmphasizedSegments.
type Segment struct {
	Bold      bool
	Italic    bool
	Underline bool
	Text      string
}

// HasIrcEmphasis mirrors IrcTextFormatter::hasIrcEmphasis: it reports whether
// the raw text carries a bold, italic, or underline control code. Colors are
// not stripped first, so a color-only string reports false.
func HasIrcEmphasis(text string) bool {
	for index := 0; index < len(text); index++ {
		switch text[index] {
		case plainTextBold, plainTextItalic, plainTextUnderline:
			return true
		}
	}
	return false
}

// HasIrcColorCodes reports whether the raw text carries a mIRC palette (\x03)
// or hex (\x04) color control. It does not require a well-formed payload: the
// presence of the introducer is enough for the transcript to consider the
// styled path when colors are enabled.
func HasIrcColorCodes(text string) bool {
	for index := 0; index < len(text); index++ {
		switch text[index] {
		case plainTextColor, plainTextHexColor:
			return true
		}
	}
	return false
}

// HasIrcStyle reports whether the raw text carries emphasis or color controls.
// The transcript uses it with PrefColors to decide whether to walk styled runs.
func HasIrcStyle(text string) bool {
	return HasIrcEmphasis(text) || HasIrcColorCodes(text)
}

// StyledSegments mirrors Rust styled_runs: one walker reads color sequences
// before removing them, toggles emphasis, and yields runs whose text covers
// PlainIrcText. It reuses consumeMircColor / consumeHexColor so consumption
// agrees with the plain-text stripper. It walks bytes rather than runes: every
// consumed sequence is ASCII, and no UTF-8 continuation byte can equal a
// control code.
func StyledSegments(text string) []StyledSegment {
	var (
		segments []StyledSegment
		style    Style
		run      []byte
	)
	flush := func() {
		if len(run) == 0 {
			return
		}
		segments = append(segments, StyledSegment{
			Style: style,
			Text:  string(run),
		})
		run = run[:0]
	}
	for index := 0; index < len(text); index++ {
		switch text[index] {
		case plainTextColor:
			flush()
			end := consumeMircColor(text, index)
			applyMircColor(text, index, &style)
			index = end
		case plainTextHexColor:
			flush()
			end := consumeHexColor(text, index)
			if end > index {
				applyHexColor(text, index, &style)
			}
			index = end
		case plainTextBold:
			flush()
			style.Bold = !style.Bold
		case plainTextItalic:
			flush()
			style.Italic = !style.Italic
		case plainTextUnderline:
			flush()
			style.Underline = !style.Underline
		case plainTextReverse:
			flush()
			style.Reverse = !style.Reverse
		case plainTextMonospace:
			flush()
			style.Monospace = !style.Monospace
		case plainTextStrikethrough:
			flush()
			style.Strikethrough = !style.Strikethrough
		case plainTextReset:
			flush()
			style = Style{}
		default:
			run = append(run, text[index])
		}
	}
	flush()
	return segments
}

// EmphasizedSegments mirrors IrcTextFormatter::emphasizedIrcText: bold, italic,
// and underline only over the plain text. Colors and reverse/monospace/
// strikethrough do not appear on the projection; adjacent runs that share the
// three emphasis flags are merged. Built from StyledSegments so color
// consumption cannot drift from the full walker.
func EmphasizedSegments(text string) []Segment {
	var segments []Segment
	for _, run := range StyledSegments(text) {
		bold, italic, underline := run.Style.Bold, run.Style.Italic, run.Style.Underline
		if n := len(segments); n > 0 {
			last := &segments[n-1]
			if last.Bold == bold && last.Italic == italic && last.Underline == underline {
				last.Text += run.Text
				continue
			}
		}
		segments = append(segments, Segment{
			Bold:      bold,
			Italic:    italic,
			Underline: underline,
			Text:      run.Text,
		})
	}
	return segments
}

// applyMircColor applies the \x03 at start. A bare code resets both colors;
// digits set the foreground and an optional ,bg sets the background. Palette
// indices past 15 are ignored and keep the previous color, as in Rust/Qt.
func applyMircColor(text string, start int, style *Style) {
	at := start + 1
	fg, ok := readPaletteCode(text, &at)
	if !ok {
		style.Fg = nil
		style.Bg = nil
		return
	}
	if fg <= 15 {
		color := PaletteColor(fg)
		style.Fg = &color
	}
	if at < len(text) && text[at] == ',' {
		bgAt := at + 1
		if bg, ok := readPaletteCode(text, &bgAt); ok && bg <= 15 {
			color := PaletteColor(bg)
			style.Bg = &color
		}
	}
}

// applyHexColor applies a valid \x04 sequence at start: six hex digits set the
// foreground and an optional , plus six more set the background. Callers must
// only invoke this when consumeHexColor advanced past start.
func applyHexColor(text string, start int, style *Style) {
	fg, ok := rgbAt(text, start+1)
	if !ok {
		return
	}
	style.Fg = &fg
	comma := start + 7
	if comma < len(text) && text[comma] == ',' {
		if bg, ok := rgbAt(text, comma+1); ok {
			style.Bg = &bg
		}
	}
}

// readPaletteCode reads one or two decimal digits at *at and advances *at past
// them. It mirrors Rust read_code.
func readPaletteCode(text string, at *int) (uint8, bool) {
	if *at >= len(text) || !plainTextASCIIDigit(text[*at]) {
		return 0, false
	}
	value := text[*at] - '0'
	*at++
	if *at < len(text) && plainTextASCIIDigit(text[*at]) {
		value = value*10 + (text[*at] - '0')
		*at++
	}
	return value, true
}

// rgbAt reads six hex digits at start as an RGB color, or false when the run
// is incomplete or non-hex.
func rgbAt(text string, start int) (IrcColor, bool) {
	if !plainTextSixHexAt(text, start) {
		return IrcColor{}, false
	}
	var value uint32
	for offset := 0; offset < 6; offset++ {
		digit, ok := hexDigitValue(text[start+offset])
		if !ok {
			return IrcColor{}, false
		}
		value = value*16 + uint32(digit)
	}
	return RGBColor(uint8(value>>16), uint8(value>>8), uint8(value)), true
}
