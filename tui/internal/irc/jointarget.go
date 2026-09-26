package irc

import (
	"strings"
	"unicode"
	"unicode/utf16"
)

// JoinTarget is one validated JOIN target: a channel name and an optional
// secret key. It mirrors IrcJoinTarget; build one with MakeJoinTarget or
// ParseJoinTargets.
type JoinTarget struct {
	channel string
	key     string
	hasKey  bool
}

// Channel returns the validated channel name.
func (t JoinTarget) Channel() string {
	return t.channel
}

// Key returns the channel key and whether one is present.
func (t JoinTarget) Key() (string, bool) {
	return t.key, t.hasKey
}

// HasKey reports whether the target carries a channel key.
func (t JoinTarget) HasKey() bool {
	return t.hasKey
}

// MakeJoinTarget validates one channel and optional key. It mirrors
// IrcJoinTarget::make: an empty name fails, a name that does not start with an
// advertised channel type gets '#' prepended, and the result must be at least
// two characters with no field breakers. ok is false when the target is
// invalid.
func MakeJoinTarget(channel string, key *string, features ServerFeatures) (JoinTarget, bool) {
	name := trimSpace(channel)
	if name == "" {
		return JoinTarget{}, false
	}
	if !features.IsChannel(name) {
		name = "#" + name
	}
	if utf16Length(name) < 2 || hasFieldBreakers(name) {
		return JoinTarget{}, false
	}

	target := JoinTarget{channel: name}
	if key != nil {
		trimmedKey := trimSpace(*key)
		if !isSafeJoinKey(trimmedKey) {
			return JoinTarget{}, false
		}
		target.key = trimmedKey
		target.hasKey = true
	}
	return target, true
}

// ParseJoinTargets splits a /join argument into validated targets. Commas
// separate entries and each entry may carry one key after whitespace. It
// mirrors ircParseJoinTargets: nil means the whole argument is invalid or
// empty.
func ParseJoinTargets(argument string, features ServerFeatures) []JoinTarget {
	var targets []JoinTarget
	for _, entry := range strings.Split(argument, ",") {
		trimmed := trimSpace(entry)
		if trimmed == "" {
			continue
		}
		tokens := splitWhitespaceRuns(trimmed)
		if len(tokens) == 0 || len(tokens) > 2 {
			return nil
		}
		var key *string
		if len(tokens) == 2 {
			key = &tokens[1]
		}
		target, ok := MakeJoinTarget(tokens[0], key, features)
		if !ok {
			return nil
		}
		targets = append(targets, target)
	}
	if len(targets) == 0 {
		return nil
	}
	return targets
}

func hasFieldBreakers(token string) bool {
	for _, character := range token {
		if unicode.IsSpace(character) || character == ',' || character == 0 {
			return true
		}
	}
	return false
}

func isSafeJoinKey(key string) bool {
	return key != "" && key[0] != ':' && !hasFieldBreakers(key)
}

// trimSpace mirrors QString::trimmed: both ends lose Unicode whitespace.
func trimSpace(value string) string {
	return strings.TrimSpace(value)
}

// splitWhitespaceRuns splits on runs of ASCII whitespace, matching the
// QRegularExpression "\\s+" used by ircParseJoinTargets. An empty result means
// the input carried no token.
func splitWhitespaceRuns(text string) []string {
	var tokens []string
	start := -1
	for index := 0; index < len(text); index++ {
		if isASCIIWhitespace(text[index]) {
			if start != -1 {
				tokens = append(tokens, text[start:index])
				start = -1
			}
			continue
		}
		if start == -1 {
			start = index
		}
	}
	if start != -1 {
		tokens = append(tokens, text[start:])
	}
	return tokens
}

func isASCIIWhitespace(character byte) bool {
	switch character {
	case ' ', '\t', '\n', '\v', '\f', '\r':
		return true
	}
	return false
}

// utf16Length counts UTF-16 code units, matching QString::size.
func utf16Length(text string) int {
	return len(utf16.Encode([]rune(text)))
}
