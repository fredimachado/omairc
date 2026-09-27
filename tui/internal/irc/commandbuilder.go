package irc

import (
	"strings"
	"unicode"
)

// Line appends CRLF to a command, rejecting empty input, embedded CR/LF/NUL,
// and lines past MaxClassicFrameBytes. It mirrors IrcCommandBuilder::line.
func Line(command string) (string, error) {
	if command == "" {
		return "", ErrorEmptyInput
	}
	if containsForbidden(command) {
		return "", ErrorInvalidCharacter
	}
	if len(command)+2 > MaxClassicFrameBytes {
		return "", ErrorTooManyBytes
	}
	return command + "\r\n", nil
}

// SplitTrailingParam splits body into frame-sized chunks preceded by prefix and
// followed by suffix. It prefers a space boundary and never splits a UTF-8
// continuation byte from its lead byte. It mirrors splitTrailingParam.
func SplitTrailingParam(prefix, body, suffix string) []string {
	if body == "" || containsForbidden(prefix) || containsForbidden(body) ||
		containsForbidden(suffix) {
		return nil
	}
	if len(prefix)+len(suffix)+3 > MaxClassicFrameBytes {
		return nil
	}
	maxBody := MaxClassicFrameBytes - len(prefix) - len(suffix) - 2

	var chunks []string
	offset := 0
	for offset < len(body) {
		remaining := len(body) - offset
		if remaining <= maxBody {
			chunks = append(chunks, body[offset:])
			break
		}

		window := body[offset : offset+maxBody]
		lastSpace := strings.LastIndexByte(window, ' ')
		if lastSpace != -1 && lastSpace > 0 {
			chunks = append(chunks, window[:lastSpace])
			offset += lastSpace + 1
			continue
		}

		take := maxBody
		for take > 0 && body[offset+take]&0xC0 == 0x80 {
			take--
		}
		if take == 0 {
			return nil
		}
		chunks = append(chunks, body[offset:offset+take])
		offset += take
	}
	return chunks
}

// Nick builds a NICK line.
func Nick(nickname string) (string, error) {
	if !isSingleField(nickname) {
		return "", ErrorInvalidField
	}
	return Line("NICK " + nickname)
}

// User builds the USER line with the fixed "8 * :realname" shape.
func User(username, realname string) (string, error) {
	if !isSingleField(username) || realname == "" || containsForbidden(realname) {
		return "", ErrorInvalidField
	}
	return Line("USER " + username + " 8 * :" + realname)
}

// Pass builds a PASS line.
func Pass(password string) (string, error) {
	if !isSingleField(password) {
		return "", ErrorInvalidField
	}
	return Line("PASS " + password)
}

// Join builds a JOIN line. An empty key omits the key parameter.
func Join(channel, key string) (string, error) {
	if !isJoinField(channel) {
		return "", ErrorInvalidField
	}
	if key == "" {
		return Line("JOIN " + channel)
	}
	if !isJoinField(key) {
		return "", ErrorInvalidField
	}
	return Line("JOIN " + channel + " " + key)
}

// Registration builds the optional PASS plus NICK and USER lines.
func Registration(nickname, username, realname, password string) (string, error) {
	nickCommand, err := Nick(nickname)
	if err != nil {
		return "", err
	}
	userCommand, err := User(username, realname)
	if err != nil {
		return "", err
	}

	result := ""
	if password != "" {
		passCommand, err := Pass(password)
		if err != nil {
			return "", err
		}
		result += passCommand
	}
	result += nickCommand
	result += userCommand
	return result, nil
}

// isSingleField rejects empty, embedded CR/LF/NUL, and ASCII space.
func isSingleField(field string) bool {
	return field != "" && !containsForbidden(field) &&
		strings.IndexByte(field, ' ') == -1
}

// isJoinField rejects a leading ':', empty, ASCII or Unicode whitespace, ',',
// and NUL. It mirrors the QString-based check in the C++ builder, so it is
// Unicode-aware.
func isJoinField(field string) bool {
	if field == "" || field[0] == ':' {
		return false
	}
	for _, character := range field {
		if unicode.IsSpace(character) || character == ',' || character == 0 {
			return false
		}
	}
	return true
}
