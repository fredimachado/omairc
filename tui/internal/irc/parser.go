package irc

import "strings"

// Parse parses one IRC line (without its CRLF) into a Message. On failure it
// returns a zero Message and an Error value; on success the error is nil. It
// mirrors IrcParser::parse in src/irc/ircparser.cpp.
func Parse(line string) (Message, error) {
	if line == "" {
		return Message{}, ErrorEmptyInput
	}
	if containsForbidden(line) {
		return Message{}, ErrorInvalidCharacter
	}

	var message Message
	position := 0

	if line[0] == '@' {
		space := strings.IndexByte(line, ' ')
		if space == -1 || space == 1 || space-1 > MaxTagSectionBytes {
			return Message{}, ErrorInvalidTags
		}
		if !parseTags(line[1:space], &message.Tags) {
			return Message{}, ErrorInvalidTags
		}
		position = space + 1
		for position < len(line) && line[position] == ' ' {
			position++
		}
	}

	if position < len(line) && line[position] == ':' {
		relative := strings.IndexByte(line[position:], ' ')
		if relative == -1 || relative == 1 {
			return Message{}, ErrorInvalidPrefix
		}
		space := position + relative
		message.Prefix = parsePrefix(line[position+1 : space])
		if message.Prefix == nil {
			return Message{}, ErrorInvalidPrefix
		}
		position = space + 1
		for position < len(line) && line[position] == ' ' {
			position++
		}
	}

	if position == len(line) {
		return Message{}, ErrorInvalidCommand
	}

	relative := strings.IndexByte(line[position:], ' ')
	var command string
	if relative == -1 {
		command = line[position:]
	} else {
		command = line[position : position+relative]
	}
	if !isValidCommand(command) {
		return Message{}, ErrorInvalidCommand
	}
	message.Command = uppercaseASCII(command)

	if relative != -1 {
		message.Params = parseParameters(line[position+relative+1:])
	}

	return message, nil
}

func containsForbidden(text string) bool {
	return strings.IndexByte(text, '\r') != -1 ||
		strings.IndexByte(text, '\n') != -1 ||
		strings.IndexByte(text, 0) != -1
}

func isValidCommand(command string) bool {
	if len(command) == 3 {
		digits := true
		for index := 0; index < len(command); index++ {
			if !isASCIIDigit(command[index]) {
				digits = false
				break
			}
		}
		if digits {
			return true
		}
	}

	if command == "" {
		return false
	}
	for index := 0; index < len(command); index++ {
		if !isASCIIAlpha(command[index]) {
			return false
		}
	}
	return true
}

func uppercaseASCII(text string) string {
	result := []byte(text)
	for index := range result {
		if result[index] >= 'a' && result[index] <= 'z' {
			result[index] -= 'a' - 'A'
		}
	}
	return string(result)
}

func unescapeTagValue(value string) string {
	result := make([]byte, 0, len(value))
	for index := 0; index < len(value); index++ {
		if value[index] != '\\' {
			result = append(result, value[index])
			continue
		}
		index++
		if index == len(value) {
			break
		}
		switch value[index] {
		case ':':
			result = append(result, ';')
		case 's':
			result = append(result, ' ')
		case '\\':
			result = append(result, '\\')
		case 'r':
			result = append(result, '\r')
		case 'n':
			result = append(result, '\n')
		default:
			result = append(result, value[index])
		}
	}
	return string(result)
}

func isValidTagName(name string) bool {
	if name != "" && name[0] == '+' {
		name = name[1:]
	}
	if name == "" {
		return false
	}

	slash := strings.IndexByte(name, '/')
	vendor := ""
	key := name
	if slash != -1 {
		vendor = name[:slash]
		key = name[slash+1:]
	}

	if key == "" {
		return false
	}
	if slash != -1 && strings.IndexByte(name[slash+1:], '/') != -1 {
		return false
	}
	for index := 0; index < len(key); index++ {
		if !isValidKeyCharacter(key[index]) {
			return false
		}
	}
	if slash == -1 {
		return true
	}
	if vendor == "" {
		return false
	}
	for index := 0; index < len(vendor); index++ {
		if !isValidVendorCharacter(vendor[index]) {
			return false
		}
	}
	return true
}

func isValidKeyCharacter(character byte) bool {
	return character < 0x80 && (isASCIIAlnum(character) || character == '-')
}

func isValidVendorCharacter(character byte) bool {
	return character < 0x80 &&
		(isASCIIAlnum(character) || character == '-' || character == '.')
}

func parseTags(section string, tags *[]Tag) bool {
	start := 0
	for start <= len(section) {
		relative := strings.IndexByte(section[start:], ';')
		var tag string
		if relative == -1 {
			tag = section[start:]
		} else {
			tag = section[start : start+relative]
		}

		equals := strings.IndexByte(tag, '=')
		var name string
		if equals == -1 {
			name = tag
		} else {
			name = tag[:equals]
		}
		if !isValidTagName(name) {
			return false
		}

		parsed := Tag{Name: name}
		if equals != -1 {
			value := unescapeTagValue(tag[equals+1:])
			parsed.Value = &value
		}
		*tags = append(*tags, parsed)

		if relative == -1 {
			return true
		}
		start = start + relative + 1
	}
	return false
}

func parsePrefix(raw string) *Prefix {
	if raw == "" || containsForbidden(raw) {
		return nil
	}

	prefix := &Prefix{Raw: raw}

	at := strings.IndexByte(raw, '@')
	nickPart := raw
	if at != -1 {
		if at == 0 || at+1 == len(raw) || strings.IndexByte(raw[at+1:], '@') != -1 {
			return nil
		}
		nickPart = raw[:at]
		prefix.Host = raw[at+1:]
	}

	bang := strings.IndexByte(nickPart, '!')
	if bang != -1 {
		if bang == 0 || bang+1 == len(nickPart) ||
			strings.IndexByte(nickPart[bang+1:], '!') != -1 {
			return nil
		}
		prefix.Nick = nickPart[:bang]
		prefix.User = nickPart[bang+1:]
	} else if at != -1 {
		prefix.Nick = nickPart
	}

	return prefix
}

func parseParameters(input string) []string {
	var parameters []string
	position := 0

	for position < len(input) {
		for position < len(input) && input[position] == ' ' {
			position++
		}
		if position == len(input) {
			break
		}

		if len(parameters) == 14 {
			if input[position] == ':' {
				position++
			}
			parameters = append(parameters, input[position:])
			break
		}

		if input[position] == ':' {
			parameters = append(parameters, input[position+1:])
			break
		}

		relative := strings.IndexByte(input[position:], ' ')
		if relative == -1 {
			parameters = append(parameters, input[position:])
			break
		}
		parameters = append(parameters, input[position:position+relative])
		position = position + relative + 1
	}

	return parameters
}

func isASCIIDigit(character byte) bool {
	return character >= '0' && character <= '9'
}

func isASCIIAlpha(character byte) bool {
	return (character >= 'A' && character <= 'Z') ||
		(character >= 'a' && character <= 'z')
}

func isASCIIAlnum(character byte) bool {
	return isASCIIDigit(character) || isASCIIAlpha(character)
}
