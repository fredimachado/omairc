package irc

import "strings"

// ListMaskKind selects which channel list mode a wrapper or inquiry targets.
// It mirrors client/src/irc/listmask.rs.
type ListMaskKind int

const (
	// ListMaskBan is channel ban list mode b.
	ListMaskBan ListMaskKind = iota
	// ListMaskQuiet is channel quiet list mode q (CHANMODES A).
	ListMaskQuiet
)

// ModeLetter is the MODE letter this kind inquires or sets.
func (kind ListMaskKind) ModeLetter() byte {
	switch kind {
	case ListMaskQuiet:
		return 'q'
	default:
		return 'b'
	}
}

// ExpandListMask expands a nick or hostmask the way /ban does: a token with no
// ! or @ becomes nick!*@*; otherwise the token is kept as typed.
func ExpandListMask(token string) string {
	if strings.Contains(token, "!") || strings.Contains(token, "@") {
		return token
	}
	return token + "!*@*"
}

// KickNickFromMask is the nick /kickban kicks when the typed token is a mask:
// the text before the first !, or the whole token when there is none.
func KickNickFromMask(token string) string {
	if bang := strings.IndexByte(token, '!'); bang >= 0 {
		return token[:bang]
	}
	return token
}
