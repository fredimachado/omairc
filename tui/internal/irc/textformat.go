package irc

// Segment is one styled run of transcript text. The TUI never renders mIRC
// colors, so only bold, italic, and underline survive as flags; a segment
// with all three false is plain text.
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

// EmphasizedSegments mirrors IrcTextFormatter::emphasizedIrcText, but yields
// styled runs instead of HTML. It strips mIRC color and hex-color runs first,
// then toggles bold (0x02), italic (0x1d), and underline (0x1f); 0x0f resets
// all three; reverse (0x16), monospace (0x11), and strikethrough (0x1e) are
// dropped without flushing, so a surrounding run stays open across them.
//
// It walks bytes rather than runes: every consumed sequence is ASCII, and no
// UTF-8 continuation byte can equal a control code, so byte indexing
// preserves the rest of the UTF-8 text (same rationale as plaintext.go).
func EmphasizedSegments(text string) []Segment {
	stripped := stripIrcColors(text)
	var (
		segments  []Segment
		bold      bool
		italic    bool
		underline bool
		run       []byte
	)
	flush := func() {
		if len(run) == 0 {
			return
		}
		segments = append(segments, Segment{
			Bold:      bold,
			Italic:    italic,
			Underline: underline,
			Text:      string(run),
		})
		run = run[:0]
	}
	for index := 0; index < len(stripped); index++ {
		switch stripped[index] {
		case plainTextBold:
			flush()
			bold = !bold
		case plainTextItalic:
			flush()
			italic = !italic
		case plainTextUnderline:
			flush()
			underline = !underline
		case plainTextReset:
			flush()
			bold, italic, underline = false, false, false
		case plainTextReverse, plainTextMonospace, plainTextStrikethrough:
			// Dropped without a flush so an open run is not split.
		default:
			run = append(run, stripped[index])
		}
	}
	flush()
	return segments
}
