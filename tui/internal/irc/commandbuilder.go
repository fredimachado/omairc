package irc

import (
	"strings"
	"unicode"
	"unicode/utf8"
)

func frameLimit(frameBytes []int) int {
	if len(frameBytes) == 0 {
		return MaxClassicFrameBytes
	}
	return frameBytes[0]
}

// Line appends CRLF to a command, rejecting empty input, embedded CR/LF/NUL,
// and lines past the frame. frameBytes includes CRLF and defaults to the
// classic 512-byte frame. It mirrors IrcCommandBuilder::line.
func Line(command string, frameBytes ...int) (string, error) {
	frame := frameLimit(frameBytes)
	if command == "" {
		return "", ErrorEmptyInput
	}
	if containsForbidden(command) {
		return "", ErrorInvalidCharacter
	}
	if frame < 2 || len(command)+2 > frame {
		return "", ErrorTooManyBytes
	}
	return command + "\r\n", nil
}

// ComposerByteBudget is the maximum UTF-8 bytes the composer may hold so one
// send stays inside frameBytes. An empty target is Status: a raw line,
// excluding CRLF. Any other target is the PRIVMSG trailing body that fits
// (`PRIVMSG <target> :<body>\r\n`), the same residual SplitTrailingParam uses
// for that prefix and an empty suffix. Tags are not subtracted. frameBytes
// defaults to 512 when LINELEN is absent. It mirrors
// IrcCommandBuilder::composerByteBudget.
func ComposerByteBudget(target string, frameBytes ...int) int {
	frame := frameLimit(frameBytes)
	if frame < 2 {
		return 0
	}
	if target == "" {
		return frame - 2
	}
	prefix := "PRIVMSG " + target + " :"
	if len(prefix)+3 > frame {
		return 0
	}
	return frame - len(prefix) - 2
}

// ActionComposerByteBudget is the composer cap for a `/me` draft. The CTCP
// ACTION wrapper is 9 bytes beyond a PRIVMSG, and the 4-byte `/me ` counts.
// It mirrors IrcCommandBuilder::actionComposerByteBudget.
func ActionComposerByteBudget(target string, frameBytes ...int) int {
	const actionWrapper = 9
	const mePrefix = 4
	privmsg := ComposerByteBudget(target, frameBytes...)
	if privmsg < actionWrapper {
		return 0
	}
	return privmsg - actionWrapper + mePrefix
}

func draftIsAction(draft string) bool {
	trimmed := strings.TrimLeft(draft, " \t")
	if !strings.HasPrefix(trimmed, "/") {
		return false
	}
	space := strings.IndexByte(trimmed, ' ')
	if space < 0 {
		return false
	}
	token := trimmed[:space]
	return len(token) == 3 && (token[1] == 'm' || token[1] == 'M') &&
		(token[2] == 'e' || token[2] == 'E')
}

// ComposerByteBudgetForDraft selects the action cap when draft is `/me` plus
// a body on a conversation. Status stays a raw line. It mirrors
// IrcCommandBuilder::composerByteBudgetForDraft.
func ComposerByteBudgetForDraft(target, draft string, frameBytes ...int) int {
	if target == "" || !draftIsAction(draft) {
		return ComposerByteBudget(target, frameBytes...)
	}
	return ActionComposerByteBudget(target, frameBytes...)
}

// ClampUtf8Prefix keeps the longest prefix whose UTF-8 encoding fits in
// maxBytes. A 1, 2, 3, or 4-byte scalar is kept whole or dropped. It mirrors
// IrcCommandBuilder::clampUtf8Prefix.
func ClampUtf8Prefix(text string, maxBytes int) string {
	if maxBytes <= 0 || text == "" {
		return ""
	}
	end := 0
	for end < len(text) {
		_, size := utf8.DecodeRuneInString(text[end:])
		if size <= 0 || end+size > maxBytes {
			break
		}
		end += size
	}
	return text[:end]
}

// SplitTrailingParam splits body into frame-sized chunks preceded by prefix and
// followed by suffix. It prefers a space boundary and never splits a UTF-8
// continuation byte from its lead byte. frameBytes includes CRLF and defaults
// to 512. It mirrors splitTrailingParam.
func SplitTrailingParam(prefix, body, suffix string, frameBytes ...int) []string {
	frame := frameLimit(frameBytes)
	if body == "" || containsForbidden(prefix) || containsForbidden(body) ||
		containsForbidden(suffix) {
		return nil
	}
	if frame < 3 || len(prefix)+len(suffix)+3 > frame {
		return nil
	}
	maxBody := frame - len(prefix) - len(suffix) - 2

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
