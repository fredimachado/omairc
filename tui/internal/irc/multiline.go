package irc

import (
	"strconv"
	"strings"
)

// MultilineType is the batch type that carries one multi-line message.
const MultilineType = "draft/multiline"

// MultilineConcatTag joins a line to the previous one with no line break.
const MultilineConcatTag = "draft/multiline-concat"

// MultilineMaxLines is the most lines one batch buffers before the rest arrive
// one by one.
const MultilineMaxLines = 100

// MultilineMaxBytes is the most raw parameter bytes one batch buffers before the
// rest arrive one by one.
const MultilineMaxBytes = 64 * 1024

// MultilineLimits is what a server advertises in the CAP value
// `max-bytes=4096,max-lines=100`. It is stored for later: receiving needs
// neither, and the buffer ceilings above are ours. It mirrors MultilineLimits in
// `client/src/irc/multiline.rs`.
type MultilineLimits struct {
	MaxBytes    int64
	HasMaxBytes bool
	MaxLines    int64
	HasMaxLines bool
}

// ParseMultilineLimits reads max-bytes and max-lines from a CAP value. Keys are
// case-sensitive; anything else, or a value that is not a number, leaves the
// field unset.
func ParseMultilineLimits(value string) MultilineLimits {
	var limits MultilineLimits
	for _, entry := range strings.Split(value, ",") {
		key, number, found := strings.Cut(entry, "=")
		if !found {
			continue
		}
		switch key {
		case "max-bytes":
			if !limits.HasMaxBytes {
				limits.MaxBytes, limits.HasMaxBytes = parseLimit(number)
			}
		case "max-lines":
			if !limits.HasMaxLines {
				limits.MaxLines, limits.HasMaxLines = parseLimit(number)
			}
		}
	}
	return limits
}

// parseLimit reads an unsigned decimal number, as Rust's u64 parse does.
func parseLimit(text string) (int64, bool) {
	if text == "" || text[0] == '+' || text[0] == '-' {
		return 0, false
	}
	number, err := strconv.ParseInt(text, 10, 64)
	if err != nil {
		return 0, false
	}
	return number, true
}

// AssembleMultiline joins the PRIVMSG and NOTICE lines of one batch into one
// message. The lines are joined with "\n", except where a line carries
// draft/multiline-concat, which joins it to the one before with nothing. The
// sender, time, message id and account come from open, the BATCH line. It reports
// false when there is nothing to show as one message (no such line, or only blank
// ones) or a line is CTCP, which the spec does not allow in a batch; the caller
// then delivers the lines one by one. It mirrors assemble_multiline in
// `client/src/irc/multiline.rs`.
func AssembleMultiline(open Message, lines []Message) (Message, bool) {
	var first *Message
	var command, target string
	var body strings.Builder
	for index := range lines {
		line := lines[index]
		lineCommand := strings.ToUpper(line.Command)
		if lineCommand != "PRIVMSG" && lineCommand != "NOTICE" {
			continue
		}
		text := paramOrEmpty(line, 1)
		if strings.HasPrefix(text, "\x01") {
			return Message{}, false
		}
		lineTarget := paramOrEmpty(line, 0)
		if first == nil {
			first = &lines[index]
			command = lineCommand
			target = lineTarget
		} else if lineCommand != command || !strings.EqualFold(lineTarget, target) {
			// The spec calls a batch that mixes commands or targets invalid.
			continue
		} else if !multilineTagPresent(line, MultilineConcatTag) {
			body.WriteByte('\n')
		}
		body.WriteString(WireText([]byte(text)))
	}
	if first == nil || strings.TrimSpace(body.String()) == "" {
		return Message{}, false
	}

	if openTarget := paramOrEmpty(open, 2); openTarget != "" {
		target = openTarget
	}
	var tags []Tag
	for _, tag := range open.Tags {
		if tag.Name != "batch" {
			tags = append(tags, tag)
		}
	}
	prefix := open.Prefix
	if prefix == nil {
		prefix = first.Prefix
	}
	return Message{
		Tags:    tags,
		Prefix:  prefix,
		Command: command,
		Params:  []string{target, body.String()},
	}, true
}

func paramOrEmpty(message Message, index int) string {
	if index < len(message.Params) {
		return message.Params[index]
	}
	return ""
}

func multilineTagPresent(message Message, name string) bool {
	for _, tag := range message.Tags {
		if tag.Name == name {
			return true
		}
	}
	return false
}
