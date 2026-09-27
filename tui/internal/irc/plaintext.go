package irc

import "strings"

// The mIRC control codes plainIrcText understands.
const (
	plainTextBold          = 0x02
	plainTextColor         = 0x03
	plainTextHexColor      = 0x04
	plainTextReset         = 0x0f
	plainTextMonospace     = 0x11
	plainTextReverse       = 0x16
	plainTextItalic        = 0x1d
	plainTextStrikethrough = 0x1e
	plainTextUnderline     = 0x1f
)

// PlainIrcText strips mIRC color/hex-color runs and formatting control codes,
// mirroring IrcTextFormatter::plainIrcText. It walks bytes: the consumed
// sequence is always ASCII, and no UTF-8 continuation byte can equal a control
// code, so byte indexing preserves the rest of the UTF-8 text.
func PlainIrcText(text string) string {
	stripped := stripIrcColors(text)
	var out strings.Builder
	out.Grow(len(stripped))
	for index := 0; index < len(stripped); index++ {
		switch stripped[index] {
		case plainTextBold, plainTextReset, plainTextMonospace,
			plainTextReverse, plainTextItalic, plainTextStrikethrough,
			plainTextUnderline:
		default:
			out.WriteByte(stripped[index])
		}
	}
	return out.String()
}

// stripIrcColors removes mIRC color and hex-color codes, mirroring
// IrcTextFormatter::stripIrcColors.
func stripIrcColors(text string) string {
	var out strings.Builder
	out.Grow(len(text))
	for index := 0; index < len(text); index++ {
		switch text[index] {
		case plainTextColor:
			index = consumeMircColor(text, index)
		case plainTextHexColor:
			index = consumeHexColor(text, index)
		default:
			out.WriteByte(text[index])
		}
	}
	return out.String()
}

func plainTextASCIIDigit(code byte) bool {
	return code >= '0' && code <= '9'
}

func plainTextASCIIHex(code byte) bool {
	return plainTextASCIIDigit(code) ||
		(code >= 'A' && code <= 'F') ||
		(code >= 'a' && code <= 'f')
}

// plainTextSixHexAt reports whether text carries six hex digits at start.
func plainTextSixHexAt(text string, start int) bool {
	if start+5 >= len(text) {
		return false
	}
	for offset := 0; offset < 6; offset++ {
		if !plainTextASCIIHex(text[start+offset]) {
			return false
		}
	}
	return true
}

// consumeMircColor returns the last index of
// \x03(?:\d{1,2}(?:,\d{1,2})?)?, or index when nothing follows. It mirrors
// consumeMircColor.
func consumeMircColor(text string, index int) int {
	size := len(text)
	i := index + 1
	if i >= size || !plainTextASCIIDigit(text[i]) {
		return index
	}
	i++
	if i < size && plainTextASCIIDigit(text[i]) {
		i++
	}
	if i < size && text[i] == ',' && i+1 < size && plainTextASCIIDigit(text[i+1]) {
		i += 2
		if i < size && plainTextASCIIDigit(text[i]) {
			i++
		}
	}
	return i - 1
}

// consumeHexColor returns the last index of
// \x04(?:[0-9A-Fa-f]{6}(?:,[0-9A-Fa-f]{6})?)?, or index when nothing follows.
// It mirrors consumeHexColor.
func consumeHexColor(text string, index int) int {
	size := len(text)
	i := index + 1
	if !plainTextSixHexAt(text, i) {
		return index
	}
	i += 6
	if i < size && text[i] == ',' && plainTextSixHexAt(text, i+1) {
		i += 7
	}
	return i - 1
}
