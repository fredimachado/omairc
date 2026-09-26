package irc

import "strings"

// MaskedCommand is a redacted wire command. It mirrors IrcMaskedCommand:
// Verb is the uppercased command and Parameters are the surviving or
// replaced parameters.
type MaskedCommand struct {
	Verb       string
	Parameters []string
}

// The C++ core returns std::optional. A disengaged optional means "there is no
// replacement": the line passed through untouched or did not tokenize. In Go
// that maps to ok == false, and ok == true always carries the replacement that
// the C++ optional would have held.

type secretShape int

const (
	secretShapeTailAfterParameters secretShape = iota
	secretShapeKeyedModeTail
	secretShapeServiceRequestBody
)

type secretRule struct {
	verb              string
	shape             secretShape
	visibleParameters int
}

// secretMap mirrors kSecretMap in src/irc/ircsecretpolicy.cpp. Rows must stay
// uppercase, unique, and match their shape; the init below enforces what the
// C++ static_assert covers at compile time.
var secretMap = []secretRule{
	{"PASS", secretShapeTailAfterParameters, 0},
	{"AUTHENTICATE", secretShapeTailAfterParameters, 0},
	{"OPER", secretShapeTailAfterParameters, 1},
	{"JOIN", secretShapeTailAfterParameters, 1},
	{"MODE", secretShapeKeyedModeTail, 2},
	{"324", secretShapeKeyedModeTail, 3},
	{"PRIVMSG", secretShapeServiceRequestBody, 1},
	{"NOTICE", secretShapeServiceRequestBody, 1},
}

// servicePasswordCommands mirrors kServicePasswordCommands.
var servicePasswordCommands = []string{
	"IDENTIFY", "REGISTER", "GHOST", "RECOVER", "RELEASE", "REGAIN", "SETPASS",
}

func init() {
	if !secretMapIsWellFormed() {
		panic("irc: secret map rows must be uppercase, unique, and match their shape")
	}
}

func secretVerbIsUppercase(verb string) bool {
	if verb == "" {
		return false
	}
	for index := 0; index < len(verb); index++ {
		if verb[index] >= 'a' && verb[index] <= 'z' {
			return false
		}
	}
	return true
}

func secretMapIsWellFormed() bool {
	for index, row := range secretMap {
		if !secretVerbIsUppercase(row.verb) || row.visibleParameters < 0 {
			return false
		}
		switch row.shape {
		case secretShapeServiceRequestBody:
			if row.visibleParameters != 1 {
				return false
			}
		case secretShapeKeyedModeTail:
			if row.visibleParameters < 2 {
				return false
			}
		case secretShapeTailAfterParameters:
		}
		for earlier := 0; earlier < index; earlier++ {
			if secretMap[earlier].verb == row.verb {
				return false
			}
		}
	}
	return true
}

func secretRuleFor(verb string) *secretRule {
	for index := range secretMap {
		if verb == secretMap[index].verb {
			return &secretMap[index]
		}
	}
	return nil
}

func isServicePasswordCommand(word string) bool {
	for _, command := range servicePasswordCommands {
		if word == command {
			return true
		}
	}
	return false
}

// alnumOnlyUpper keeps ASCII alphanumerics and uppercases letters. It mirrors
// alnumOnlyUpper in the C++ core: every other byte is dropped.
func alnumOnlyUpper(token string) string {
	var builder strings.Builder
	builder.Grow(len(token))
	for index := 0; index < len(token); index++ {
		character := token[index]
		switch {
		case character >= 'A' && character <= 'Z':
			builder.WriteByte(character)
		case character >= 'a' && character <= 'z':
			builder.WriteByte(character - 'a' + 'A')
		case character >= '0' && character <= '9':
			builder.WriteByte(character)
		}
	}
	return builder.String()
}

// letterDistance is the Levenshtein distance over bytes, mirroring the C++
// implementation. Callers only feed it alnumOnlyUpper output, so bytes and
// UTF-16 code units agree.
func letterDistance(left, right string) int {
	rows := len(left)
	cols := len(right)
	if rows == 0 {
		return cols
	}
	if cols == 0 {
		return rows
	}
	previous := make([]int, cols+1)
	current := make([]int, cols+1)
	for column := 0; column <= cols; column++ {
		previous[column] = column
	}
	for row := 1; row <= rows; row++ {
		current[0] = row
		for column := 1; column <= cols; column++ {
			cost := 1
			if left[row-1] == right[column-1] {
				cost = 0
			}
			current[column] = min(current[column-1]+1, previous[column]+1, previous[column-1]+cost)
		}
		previous, current = current, previous
	}
	return previous[cols]
}

func nearestServicePassword(word string) string {
	alnum := alnumOnlyUpper(word)
	if len(alnum) < 4 {
		return ""
	}
	best := ""
	bestDistance := 2
	for _, command := range servicePasswordCommands {
		distance := letterDistance(alnum, command)
		if distance < bestDistance {
			bestDistance = distance
			best = command
		} else if distance == bestDistance {
			best = ""
		}
	}
	if best == "" || bestDistance > 1 {
		return ""
	}
	return best
}

type secretWireCommand struct {
	verb       string
	parameters []string
}

func skipFieldSpaces(text string) string {
	index := 0
	for index < len(text) && (text[index] == ' ' || text[index] == '\t') {
		index++
	}
	return text[index:]
}

func fieldBreak(text string) int {
	for index := 0; index < len(text); index++ {
		if text[index] == ' ' || text[index] == '\t' {
			return index
		}
	}
	return -1
}

// secretTokenize mirrors tokenize(): skip tags and prefix, upper the verb, and
// split the rest into parameters with a single trailing parameter.
func secretTokenize(line string) (secretWireCommand, bool) {
	rest := skipFieldSpaces(line)
	if rest == "" {
		return secretWireCommand{}, false
	}

	if rest[0] == '@' {
		space := fieldBreak(rest)
		if space < 0 {
			return secretWireCommand{}, false
		}
		rest = skipFieldSpaces(rest[space+1:])
		if rest == "" {
			return secretWireCommand{}, false
		}
	}

	if rest[0] == ':' {
		space := fieldBreak(rest)
		if space < 0 {
			return secretWireCommand{}, false
		}
		rest = skipFieldSpaces(rest[space+1:])
		if rest == "" {
			return secretWireCommand{}, false
		}
	}

	verbEnd := fieldBreak(rest)
	verb := rest
	if verbEnd >= 0 {
		verb = rest[:verbEnd]
	}
	verb = strings.ToUpper(verb)
	if verb == "" {
		return secretWireCommand{}, false
	}
	if verbEnd < 0 {
		rest = ""
	} else {
		rest = skipFieldSpaces(rest[verbEnd+1:])
	}

	command := secretWireCommand{verb: verb}
	for rest != "" {
		if rest[0] == ':' {
			command.parameters = append(command.parameters, rest[1:])
			break
		}
		space := fieldBreak(rest)
		if space < 0 {
			command.parameters = append(command.parameters, rest)
			break
		}
		command.parameters = append(command.parameters, rest[:space])
		rest = skipFieldSpaces(rest[space+1:])
	}
	return command, true
}

func secretViewOf(message Message) secretWireCommand {
	command := secretWireCommand{verb: strings.ToUpper(WireText([]byte(message.Command)))}
	command.parameters = make([]string, 0, len(message.Params))
	for _, parameter := range message.Params {
		command.parameters = append(command.parameters, WireText([]byte(parameter)))
	}
	return command
}

func trailingSeparator(trailing string) string {
	if trailing == "" || strings.HasPrefix(trailing, ":") || strings.Contains(trailing, " ") {
		return " :"
	}
	return " "
}

func formatLine(command secretWireCommand) string {
	if len(command.parameters) == 0 {
		return command.verb
	}
	head := command.parameters[:len(command.parameters)-1]
	trailing := command.parameters[len(command.parameters)-1]
	line := command.verb
	if len(head) > 0 {
		line += " " + strings.Join(head, " ")
	}
	return line + trailingSeparator(trailing) + trailing
}

func maskTailAfterParameters(command secretWireCommand, visibleParameters int) (secretWireCommand, bool) {
	if len(command.parameters) <= visibleParameters {
		return secretWireCommand{}, false
	}
	masked := command
	masked.parameters = append([]string{}, command.parameters[:visibleParameters]...)
	masked.parameters = append(masked.parameters, "***")
	return masked, true
}

func modeLetterConsumesParam(letter byte, adding bool) bool {
	if letter == 'k' || letter == 'o' || letter == 'v' || letter == 'b' {
		return true
	}
	return letter == 'l' && adding
}

func isRfcDefaultModeLetter(letter byte, adding bool) bool {
	if modeLetterConsumesParam(letter, adding) {
		return true
	}
	return letter == 'i' || letter == 'm' || letter == 'n' ||
		letter == 'p' || letter == 's' || letter == 't'
}

func maskKeyedModeTail(command secretWireCommand, visibleParameters int) (secretWireCommand, bool) {
	if visibleParameters < 2 || len(command.parameters) <= visibleParameters {
		return secretWireCommand{}, false
	}
	modes := command.parameters[visibleParameters-1]
	adding := true
	hasKey := false
	ambiguous := false
	for index := 0; index < len(modes); index++ {
		letter := modes[index]
		if letter == '+' {
			adding = true
			continue
		}
		if letter == '-' {
			adding = false
			continue
		}
		if letter == 'k' {
			hasKey = true
		} else if !isRfcDefaultModeLetter(letter, adding) {
			ambiguous = true
		}
	}
	if !hasKey {
		return secretWireCommand{}, false
	}
	if ambiguous {
		masked := command
		masked.parameters = append([]string{}, command.parameters[:visibleParameters]...)
		masked.parameters = append(masked.parameters, "***")
		return masked, true
	}

	masked := command
	masked.parameters = append([]string{}, command.parameters...)
	maskedKey := false
	argument := visibleParameters
	adding = true
	for index := 0; index < len(modes); index++ {
		letter := modes[index]
		if letter == '+' {
			adding = true
			continue
		}
		if letter == '-' {
			adding = false
			continue
		}
		if !modeLetterConsumesParam(letter, adding) {
			continue
		}
		if argument >= len(masked.parameters) {
			break
		}
		if letter == 'k' {
			masked.parameters[argument] = "***"
			maskedKey = true
		}
		argument++
	}
	if !maskedKey {
		return secretWireCommand{}, false
	}
	return masked, true
}

type recipientKind int

const (
	recipientChannel recipientKind = iota
	recipientPrivate
)

func canStartPrivateRecipient(mark byte) bool {
	if (mark >= 'A' && mark <= 'Z') || (mark >= 'a' && mark <= 'z') {
		return true
	}
	switch mark {
	case '-', '[', ']', '\\', '`', '_', '^', '{', '|', '}':
		return true
	}
	return false
}

func classifyRecipient(target, channelTypes string) recipientKind {
	if target == "" {
		return recipientPrivate
	}
	mark := target[0]
	isChannel := false
	if channelTypes == "" {
		isChannel = !canStartPrivateRecipient(mark)
	} else {
		isChannel = strings.IndexByte(channelTypes, mark) != -1
	}
	if isChannel {
		return recipientChannel
	}
	return recipientPrivate
}

func hasPrivateRecipient(targets, channelTypes string) bool {
	for _, piece := range strings.Split(targets, ",") {
		if classifyRecipient(piece, channelTypes) == recipientPrivate {
			return true
		}
	}
	return false
}

func serviceBodyTokens(body string) []string {
	normalized := strings.TrimSpace(body)
	if normalized != "" && normalized[0] == 0x01 {
		normalized = normalized[1:]
	}
	if normalized != "" && normalized[len(normalized)-1] == 0x01 {
		normalized = normalized[:len(normalized)-1]
	}
	normalized = strings.TrimSpace(normalized)
	normalized = strings.ReplaceAll(normalized, "\t", " ")

	var tokens []string
	for _, token := range strings.Split(normalized, " ") {
		if token != "" {
			tokens = append(tokens, token)
		}
	}
	return tokens
}

func matchedServiceCommand(tokens []string) string {
	if len(tokens) < 2 {
		return ""
	}
	first := strings.ToUpper(tokens[0])
	if isServicePasswordCommand(first) {
		return first
	}
	if near := nearestServicePassword(first); near != "" {
		return near
	}
	if len(tokens) >= 3 && first == "SET" {
		second := alnumOnlyUpper(tokens[1])
		if second == "PASSWORD" || letterDistance(second, "PASSWORD") == 1 {
			return "SET PASSWORD"
		}
	}
	return ""
}

func maskServiceRequestBody(command secretWireCommand, channelTypes string) (secretWireCommand, bool) {
	if len(command.parameters) < 2 {
		return secretWireCommand{}, false
	}
	if !hasPrivateRecipient(command.parameters[0], channelTypes) {
		return secretWireCommand{}, false
	}
	body := strings.Join(command.parameters[1:], " ")
	word := matchedServiceCommand(serviceBodyTokens(body))
	if word == "" {
		return secretWireCommand{}, false
	}
	masked := command
	masked.parameters = []string{command.parameters[0], word + " ***"}
	return masked, true
}

func secretMask(command secretWireCommand, channelTypes string) (secretWireCommand, bool) {
	rule := secretRuleFor(command.verb)
	if rule == nil {
		return secretWireCommand{}, false
	}
	switch rule.shape {
	case secretShapeTailAfterParameters:
		return maskTailAfterParameters(command, rule.visibleParameters)
	case secretShapeKeyedModeTail:
		return maskKeyedModeTail(command, rule.visibleParameters)
	case secretShapeServiceRequestBody:
		return maskServiceRequestBody(command, channelTypes)
	}
	return secretWireCommand{}, false
}

func nearestSecretRule(verb string) *secretRule {
	alnum := alnumOnlyUpper(verb)
	if len(alnum) < 3 {
		return nil
	}
	var best *secretRule
	bestDistance := 2
	for index := range secretMap {
		row := &secretMap[index]
		distance := letterDistance(alnum, row.verb)
		if distance < bestDistance {
			bestDistance = distance
			best = row
		} else if distance == bestDistance {
			best = nil
		}
	}
	if best == nil || bestDistance > 1 {
		return nil
	}
	return best
}

func conservativePreviewMask(line string) (string, bool) {
	alnum := alnumOnlyUpper(line)
	var best *secretRule
	bestLength := 0
	for index := range secretMap {
		row := &secretMap[index]
		verb := row.verb
		if len(verb) >= 3 && strings.HasPrefix(alnum, verb) && len(verb) > bestLength {
			best = row
			bestLength = len(verb)
		}
	}
	if best == nil || len(alnum) <= bestLength {
		return "", false
	}
	return best.verb + " ***", true
}

func previewMaskAt(slice, channelTypes string) (string, bool) {
	if parsed, ok := secretTokenize(slice); ok {
		if masked, ok := secretMask(parsed, channelTypes); ok {
			return formatLine(masked), true
		}
		if len(parsed.parameters) > 0 {
			if rule := nearestSecretRule(parsed.verb); rule != nil {
				canonical := parsed
				canonical.verb = rule.verb
				if masked, ok := secretMask(canonical, channelTypes); ok {
					return formatLine(masked), true
				}
			}
		}
	}
	return conservativePreviewMask(slice)
}

func laterCommandPreviewMask(line, channelTypes string) (string, bool) {
	rest := skipFieldSpaces(line)
	for rest != "" {
		breakAt := fieldBreak(rest)
		if breakAt < 0 {
			break
		}
		rest = skipFieldSpaces(rest[breakAt+1:])
		if rest == "" {
			break
		}
		if safe, ok := previewMaskAt(rest, channelTypes); ok {
			return safe, true
		}
	}
	return "", false
}

func redactWireLine(line, channelTypes string) (string, bool) {
	parsed, ok := secretTokenize(line)
	if !ok {
		return "", false
	}
	masked, ok := secretMask(parsed, channelTypes)
	if !ok {
		return "", false
	}
	return formatLine(masked), true
}

func redactPreviewLine(line, channelTypes string) (string, bool) {
	if safe, ok := redactWireLine(line, channelTypes); ok {
		return safe, true
	}
	if parsed, ok := secretTokenize(line); ok && len(parsed.parameters) > 0 {
		if rule := nearestSecretRule(parsed.verb); rule != nil {
			canonical := parsed
			canonical.verb = rule.verb
			if masked, ok := secretMask(canonical, channelTypes); ok {
				return formatLine(masked), true
			}
		}
	}
	if later, ok := laterCommandPreviewMask(line, channelTypes); ok {
		return later, true
	}
	return conservativePreviewMask(line)
}

func redactMessage(message Message, channelTypes string) (MaskedCommand, bool) {
	verb := strings.ToUpper(WireText([]byte(message.Command)))
	rule := secretRuleFor(verb)
	if rule == nil {
		return MaskedCommand{}, false
	}
	if rule.shape == secretShapeServiceRequestBody {
		if len(message.Params) < 2 {
			return MaskedCommand{}, false
		}
		if !hasPrivateRecipient(WireText([]byte(message.Params[0])), channelTypes) {
			return MaskedCommand{}, false
		}
	} else if verb == "JOIN" {
		return MaskedCommand{}, false
	} else if len(message.Params) <= rule.visibleParameters {
		return MaskedCommand{}, false
	}
	masked, ok := secretMask(secretViewOf(message), channelTypes)
	if !ok {
		return MaskedCommand{}, false
	}
	return MaskedCommand{Verb: masked.verb, Parameters: masked.parameters}, true
}

// RedactWireLine replaces a secret-bearing wire line with a masked form. ok is
// false when the line is not redacted (it did not tokenize or matched no mask),
// in which case the caller keeps the original line. It mirrors
// IrcSecretPolicy::redactWireLine.
func RedactWireLine(line, channelTypes string) (string, bool) {
	return redactWireLine(line, channelTypes)
}

// RedactPreviewLine is the fail-closed preview redactor: it first tries
// redactWireLine, then a near-miss command, then a later command in the
// fragment, then a conservative leading-verb mask. ok is false only when none
// produced a replacement. It mirrors IrcSecretPolicy::redactPreviewLine.
func RedactPreviewLine(line, channelTypes string) (string, bool) {
	return redactPreviewLine(line, channelTypes)
}

// RedactMessage masks a parsed message. ok is true only when a replacement was
// produced, mirroring an engaged std::optional<IrcMaskedCommand>; ok is false
// when the C++ returned std::nullopt, which leaves the message untouched.
func RedactMessage(message Message, channelTypes string) (MaskedCommand, bool) {
	return redactMessage(message, channelTypes)
}

// AllowsTranscript reports whether text is safe to keep in a conversation
// transcript. It fails closed: a wire command that redaction would mask, or a
// service-request body that RedactMessage would mask, is not allowed.
// It mirrors IrcSecretPolicy::allowsTranscript.
func AllowsTranscript(text, channelTypes string) bool {
	if text == "" {
		return true
	}
	if _, ok := redactWireLine(text, channelTypes); ok {
		return false
	}
	probe := Message{Command: "PRIVMSG", Params: []string{"NickServ", text}}
	_, ok := redactMessage(probe, channelTypes)
	return !ok
}
