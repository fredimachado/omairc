package irc

// Frame-size limits, mirrored from src/irc/ircmessage.h.
const (
	// MaxClassicFrameBytes is the RFC 1459 line ceiling, including CRLF.
	MaxClassicFrameBytes = 512
	// MaxInboundClassicFrameBytes is a receive ceiling: servers prepend
	// prefixes and emit long 005 lines.
	MaxInboundClassicFrameBytes = 4096
	// MaxTagSectionBytes is the IRCv3 message-tags ceiling, excluding the
	// leading '@' and the trailing space.
	MaxTagSectionBytes = 8191
)

// Error is the protocol error taxonomy. It mirrors IrcError in
// src/irc/ircmessage.h and implements error so Parse can return it.
type Error int

const (
	ErrorNone Error = iota
	ErrorEmptyInput
	ErrorInvalidCharacter
	ErrorInvalidTags
	ErrorInvalidPrefix
	ErrorInvalidCommand
	ErrorTooManyBytes
	ErrorInvalidField
)

// Error renders the same names as ircErrorName in the C++ core.
func (e Error) Error() string {
	switch e {
	case ErrorNone:
		return "none"
	case ErrorEmptyInput:
		return "empty input"
	case ErrorInvalidCharacter:
		return "invalid character"
	case ErrorInvalidTags:
		return "invalid tags"
	case ErrorInvalidPrefix:
		return "invalid prefix"
	case ErrorInvalidCommand:
		return "invalid command"
	case ErrorTooManyBytes:
		return "too many bytes"
	case ErrorInvalidField:
		return "invalid field"
	}
	return "unknown"
}

// Tag is one IRCv3 message tag. Value is nil for a valueless tag. It mirrors
// IrcTag.
type Tag struct {
	Name  string
	Value *string
}

// Prefix is a parsed message source. It mirrors IrcPrefix: Raw always holds
// the full source, and the other fields are filled only when present.
type Prefix struct {
	Raw  string
	Nick string
	User string
	Host string
}

// Message is a parsed IRC line. Prefix is nil when the line carried no source
// (client-to-server lines). It mirrors IrcMessage.
type Message struct {
	Tags    []Tag
	Prefix  *Prefix
	Command string
	Params  []string
}
