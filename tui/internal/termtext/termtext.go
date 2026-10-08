// Package termtext keeps text that arrives from a server or from disk from
// steering the terminal. Bubble Tea parses SGR and OSC 8 sequences inside view
// strings, so an ESC byte in a PRIVMSG would restyle the transcript or plant a
// hyperlink whose label hides its target. Qt renders text through QML, so this
// is a terminal-only policy and stays out of internal/irc.
package termtext

import "unicode/utf8"

// kept reports whether a C0 byte survives: the CTCP delimiter, the mIRC
// formatting codes that internal/irc renders or strips, tab, and CR/LF. CR and
// LF are kept so the parser still rejects a frame that carries them, exactly as
// the Qt core does.
func kept(code rune) bool {
	switch code {
	case 0x01, // CTCP
		0x02, 0x03, 0x04, 0x0f, 0x11, 0x16, 0x1d, 0x1e, 0x1f, // mIRC formatting
		'\t', '\r', '\n':
		return true
	}
	return false
}

func dropped(code rune) bool {
	switch {
	case code < 0x20:
		return !kept(code)
	case code == 0x7f:
		return true
	case code >= 0x80 && code <= 0x9f:
		return true
	}
	return false
}

// Sanitize removes ESC, DEL, the C1 range, and every C0 control that IRC text
// does not use. Invalid UTF-8 is treated byte by byte, because internal/irc
// decodes such text as Latin-1, where a stray 0x9B byte becomes U+009B (CSI).
func Sanitize(text string) string {
	if utf8.ValidString(text) {
		for index, code := range text {
			if dropped(code) {
				return sanitizeRunes(text, index)
			}
		}
		return text
	}
	out := make([]byte, 0, len(text))
	for index := 0; index < len(text); index++ {
		if !dropped(rune(text[index])) {
			out = append(out, text[index])
		}
	}
	return string(out)
}

func sanitizeRunes(text string, first int) string {
	out := make([]rune, 0, len(text))
	out = append(out, []rune(text[:first])...)
	for _, code := range text[first:] {
		if !dropped(code) {
			out = append(out, code)
		}
	}
	return string(out)
}
