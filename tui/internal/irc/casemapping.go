package irc

// Kind selects an IRC case-mapping rule. It mirrors IrcCaseMapping::Kind.
type Kind int

const (
	// KindRfc1459 is the default mapping: []\ are the lowercase forms of {}|,
	// and ^ folds to ~.
	KindRfc1459 Kind = iota
	// KindAscii is ASCII-only case mapping; the special characters are kept.
	KindAscii
	// KindRfc1459Strict folds []\ but keeps ^ distinct from ~.
	KindRfc1459Strict
)

// CaseMapping normalizes identifiers for comparison. It mirrors
// IrcCaseMapping. The zero value is KindRfc1459.
type CaseMapping struct {
	kind Kind
}

// NewCaseMapping builds a mapping for the given kind.
func NewCaseMapping(kind Kind) CaseMapping {
	return CaseMapping{kind: kind}
}

// Kind reports the mapping rule.
func (m CaseMapping) Kind() Kind {
	return m.kind
}

// FromName resolves an advertised CASEMAPPING value. It mirrors
// IrcCaseMapping::fromName; the name is matched ASCII-case-insensitively.
func FromName(name string) (CaseMapping, bool) {
	switch asciiLowerString(name) {
	case "rfc1459":
		return CaseMapping{kind: KindRfc1459}, true
	case "ascii":
		return CaseMapping{kind: KindAscii}, true
	case "rfc1459-strict", "strict-rfc1459":
		return CaseMapping{kind: KindRfc1459Strict}, true
	}
	return CaseMapping{}, false
}

// Normalize lowercases the mapping-sensitive characters of identifier.
func (m CaseMapping) Normalize(identifier string) string {
	normalized := make([]byte, 0, len(identifier))
	for index := 0; index < len(identifier); index++ {
		normalized = append(normalized, normalizedCharacter(identifier[index], m.kind))
	}
	return string(normalized)
}

// Equals reports whether two identifiers are equal under this mapping.
func (m CaseMapping) Equals(left, right string) bool {
	return m.Normalize(left) == m.Normalize(right)
}

func normalizedCharacter(character byte, kind Kind) byte {
	if character >= 'A' && character <= 'Z' {
		return character + ('a' - 'A')
	}
	if kind == KindAscii {
		return character
	}
	switch character {
	case '[':
		return '{'
	case ']':
		return '}'
	case '\\':
		return '|'
	case '^':
		if kind == KindRfc1459 {
			return '~'
		}
		return '^'
	default:
		return character
	}
}

func asciiLowerString(value string) string {
	lowered := make([]byte, len(value))
	for index := 0; index < len(value); index++ {
		lowered[index] = asciiLower(value[index])
	}
	return string(lowered)
}

func asciiLower(character byte) byte {
	if character >= 'A' && character <= 'Z' {
		return character - 'A' + 'a'
	}
	return character
}
