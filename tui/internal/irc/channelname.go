package irc

import (
	"strings"
	"unicode"
	"unicode/utf8"
)

// ChannelNameSpan is one channel name inside a message or a topic. Start and
// End are byte offsets into the scanned string, and End is exclusive. Name is
// the trimmed token, so trailing sentence punctuation sits outside the span.
// It mirrors IrcChannelNameSpan.
type ChannelNameSpan struct {
	Start int
	End   int
	Name  string
}

// ChannelNameSpans returns every channel name in text. The prefix comes from
// features, never a hard-coded CHANTYPES. An empty CHANTYPES matches nothing.
// A token starts with an advertised channel type, only at the start of text or
// after whitespace or an opening delimiter, and runs until a space, comma,
// colon, or control character. Trailing ".,;:!?" and an unmatched trailing
// bracket are stripped. A result shorter than two characters, or one
// features.IsChannel rejects, is dropped.
func ChannelNameSpans(text string, features ServerFeatures) []ChannelNameSpan {
	types := features.ChannelTypes()
	if text == "" || types == "" {
		return nil
	}
	var spans []ChannelNameSpan
	for index := 0; index < len(text); {
		character, size := utf8.DecodeRuneInString(text[index:])
		if size < 1 {
			break
		}
		boundary := index == 0 || channelBoundaryBefore(text, index)
		if !boundary || !isChannelPrefix(character, types) {
			index += size
			continue
		}
		rawEnd := index + size
		for rawEnd < len(text) {
			next, nextSize := utf8.DecodeRuneInString(text[rawEnd:])
			if nextSize < 1 || stopsChannelName(next) {
				break
			}
			rawEnd += nextSize
		}
		name := trimChannelName(text[index:rawEnd])
		if utf8.RuneCountInString(name) >= 2 && features.IsChannel(name) {
			spans = append(spans, ChannelNameSpan{
				Start: index,
				End:   index + len(name),
				Name:  name,
			})
		}
		index = rawEnd
	}
	return spans
}

// ChannelNameAt returns the channel name whose span contains index, or "".
// index is a byte offset, matching httpURLAt. It mirrors ircChannelNameAt.
func ChannelNameAt(text string, index int, features ServerFeatures) string {
	if text == "" || index < 0 || index >= len(text) {
		return ""
	}
	for _, span := range ChannelNameSpans(text, features) {
		if index >= span.Start && index < span.End {
			return span.Name
		}
	}
	return ""
}

func isChannelPrefix(character rune, types string) bool {
	if character > 0x7F {
		return false
	}
	return strings.IndexByte(types, byte(character)) >= 0
}

func stopsChannelName(character rune) bool {
	if character < 0x20 || character == 0x7F {
		return true
	}
	if unicode.IsSpace(character) {
		return true
	}
	return character == ',' || character == ':'
}

func channelBoundaryBefore(text string, index int) bool {
	character, _ := utf8.DecodeLastRuneInString(text[:index])
	return isChannelBoundary(character)
}

func isChannelBoundary(character rune) bool {
	if unicode.IsSpace(character) {
		return true
	}
	switch character {
	case '(', '[', '{', '<', '"', '\'', ',':
		return true
	default:
		return false
	}
}

func trimChannelName(raw string) string {
	end := len(raw)
	for end > 0 {
		character, size := utf8.DecodeLastRuneInString(raw[:end])
		if size < 1 {
			break
		}
		if strings.ContainsRune(".,;:!?", character) {
			end -= size
			continue
		}
		var open rune
		switch character {
		case ')':
			open = '('
		case ']':
			open = '['
		case '}':
			open = '{'
		default:
			return raw[:end]
		}
		opens := 0
		closes := 0
		for _, current := range raw[:end] {
			switch current {
			case open:
				opens++
			case character:
				closes++
			}
		}
		if closes > opens {
			end -= size
			continue
		}
		break
	}
	return raw[:end]
}
