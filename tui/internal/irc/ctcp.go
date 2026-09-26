package irc

import (
	"fmt"
	"strconv"
	"strings"
	"time"
)

const ctcpDelimiter = '\x01'

// CtcpRequest is a parsed CTCP payload. It mirrors IrcCtcpRequest.
type CtcpRequest struct {
	Command  string
	Argument string
}

// ParseCtcpRequest parses a CTCP body wrapped in \x01 delimiters. ok is false
// when body is not delimited or carries no command.
func ParseCtcpRequest(body string) (CtcpRequest, bool) {
	if len(body) < 2 || body[0] != ctcpDelimiter || body[len(body)-1] != ctcpDelimiter {
		return CtcpRequest{}, false
	}

	payload := body[1 : len(body)-1]
	separator := strings.IndexByte(payload, ' ')
	rawCommand := payload
	if separator >= 0 {
		rawCommand = payload[:separator]
	}
	command := strings.ToUpper(strings.TrimSpace(rawCommand))
	if command == "" {
		return CtcpRequest{}, false
	}

	argument := ""
	if separator >= 0 {
		argument = payload[separator+1:]
	}
	return CtcpRequest{Command: command, Argument: argument}, true
}

// CtcpPayload re-wraps a request in \x01 delimiters.
func CtcpPayload(request CtcpRequest) string {
	body := request.Command
	if request.Argument != "" {
		body = request.Command + " " + request.Argument
	}
	return string(ctcpDelimiter) + body + string(ctcpDelimiter)
}

// CtcpVersionReplyText advertises the project URL, not the build number.
func CtcpVersionReplyText() string {
	return "https://omairc.app"
}

// FormatCtcpReplyText renders the local Status line for one CTCP reply. PING
// reports lag against now when the argument is a millisecond timestamp.
func FormatCtcpReplyText(command, nick, argument string, now time.Time) string {
	if command == "PING" {
		if sentMilliseconds, err := strconv.ParseInt(argument, 10, 64); err == nil {
			lag := now.UnixMilli() - sentMilliseconds
			if lag >= 0 && lag < 24*60*60*1000 {
				return fmt.Sprintf("PING reply from %s: %d ms", nick, lag)
			}
		}
		if argument == "" {
			return fmt.Sprintf("PING reply from %s", nick)
		}
		return fmt.Sprintf("PING reply from %s: %s", nick, argument)
	}
	if argument == "" {
		return fmt.Sprintf("%s reply from %s", command, nick)
	}
	return fmt.Sprintf("%s reply from %s: %s", command, nick, argument)
}
