package session

import (
	"sort"
	"strings"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// draft/multiline receive. A multiline batch is buffered whole and delivered as
// one synthetic PRIVMSG or NOTICE (irc.AssembleMultiline), through the path any
// single line takes: live as MessageReceived, or into the replay root's
// collected lines when the batch is nested in CHATHISTORY. Nothing downstream
// knows about batches. It mirrors `client/src/session/multiline.rs`.

// multilineBuffer holds the lines one open multiline batch has buffered.
type multilineBuffer struct {
	// open is the `BATCH +ref draft/multiline <target>` line, which carries the tags.
	open irc.Message
	// lines are the raw tagged lines, at most irc.MultilineMaxLines.
	lines []irc.Message
	// bytes is the raw text bytes of lines, at most irc.MultilineMaxBytes.
	bytes int
	// overflowed means a ceiling was hit: the buffered part was delivered, and
	// the rest of the batch is delivered line by line.
	overflowed bool
}

// multilineEnabledLocked reports whether a multiline batch is assembled: it needs
// batch to be a batch and message-tags for the batch and concat tags on its
// lines.
func (s *Session) multilineEnabledLocked() bool {
	enabled := s.capabilities.Enabled()
	return enabled.Contains(irc.CapabilityBatch) &&
		enabled.Contains(irc.CapabilityMessageTags) &&
		enabled.Contains(irc.CapabilityMultiline)
}

// openMultilineLocked starts buffering a frame that BATCH +ref draft/multiline
// opened.
func (s *Session) openMultilineLocked(frame *openBatch, open irc.Message) {
	if s.multilineEnabledLocked() {
		frame.multiline = &multilineBuffer{open: open}
	}
}

// captureMultilineLocked buffers one line of an open multiline batch. It reports
// true when reference is one, and the line is therefore swallowed.
func (s *Session) captureMultilineLocked(reference string, line irc.Message) bool {
	frame, ok := s.openBatches[reference]
	if !ok || frame.multiline == nil {
		return false
	}
	buffer := frame.multiline
	root := frame.replayRoot
	command := strings.ToUpper(line.Command)
	if command != "PRIVMSG" && command != "NOTICE" {
		return true
	}

	size := 0
	if len(line.Params) > 1 {
		size = len(line.Params[1])
	}
	var deliver []irc.Message
	full := len(buffer.lines) >= irc.MultilineMaxLines ||
		buffer.bytes+size > irc.MultilineMaxBytes
	if full && !buffer.overflowed {
		deliver = multilinePieces(buffer.open, buffer.lines)
		buffer.lines = nil
		buffer.bytes = 0
		buffer.overflowed = true
	}
	if buffer.overflowed {
		deliver = append(deliver, withoutBatchTag(line))
	} else {
		buffer.bytes += size
		buffer.lines = append(buffer.lines, line)
	}
	for _, message := range deliver {
		s.deliverAssembledLocked(root, message)
	}
	return true
}

// finishMultilineLocked delivers a closed multiline batch. root is the replay root
// the batch sat in, or empty for a live batch.
func (s *Session) finishMultilineLocked(root string, buffer *multilineBuffer) {
	for _, message := range multilinePieces(buffer.open, buffer.lines) {
		s.deliverAssembledLocked(root, message)
	}
}

// deliverAssembledLocked sends one message out of a multiline batch into the
// collected lines of the replay it sits in, or live.
func (s *Session) deliverAssembledLocked(root string, message irc.Message) {
	if root == "" {
		s.emitMessageReceived(message)
		return
	}
	// A replay root that is gone took its children with it. Delivering the line
	// live would replay history as new traffic.
	if frame, ok := s.openBatches[root]; ok {
		frame.pushCollected(message)
	}
}

// flushOpenMultilineLocked delivers every live multiline batch that never closed,
// because the connection ended first. One nested in a replay is dropped with its
// unclosed root, as its siblings are. Each buffer is emptied as it is flushed, so
// a second call delivers nothing.
func (s *Session) flushOpenMultilineLocked() {
	var references []string
	for reference, frame := range s.openBatches {
		if frame.multiline != nil && frame.replayRoot == "" {
			references = append(references, reference)
		}
	}
	sort.Strings(references)
	for _, reference := range references {
		frame := s.openBatches[reference]
		buffer := frame.multiline
		frame.multiline = nil
		s.finishMultilineLocked("", buffer)
	}
}

// multilinePieces is the message a batch assembles to, or its lines one by one
// when they do not assemble (blank, or CTCP, which the spec does not allow in a
// batch).
func multilinePieces(open irc.Message, lines []irc.Message) []irc.Message {
	if len(lines) == 0 {
		return nil
	}
	if message, ok := irc.AssembleMultiline(open, lines); ok {
		return []irc.Message{message}
	}
	pieces := make([]irc.Message, len(lines))
	for index, line := range lines {
		pieces[index] = withoutBatchTag(line)
	}
	return pieces
}

func withoutBatchTag(line irc.Message) irc.Message {
	var tags []irc.Tag
	for _, tag := range line.Tags {
		if tag.Name != "batch" {
			tags = append(tags, tag)
		}
	}
	line.Tags = tags
	return line
}
