package irc

import (
	"strings"
	"unicode/utf8"
)

// WireText converts raw wire bytes to display text. Valid UTF-8 passes through;
// anything else is decoded as Latin-1, one rune per byte, so no U+FFFD is
// injected. It mirrors ircWireText in src/irc/ircwiretext.cpp.
func WireText(raw []byte) string {
	if utf8.Valid(raw) {
		return string(raw)
	}
	decoded := make([]rune, len(raw))
	for index, value := range raw {
		decoded[index] = rune(value)
	}
	return string(decoded)
}

// containsDot reports whether text contains an ASCII period, used by
// PrefixNick to recognize server names.
func containsDot(text string) bool {
	return strings.IndexByte(text, '.') != -1
}
