package irc

import "bytes"

// FaultPreviewBytes bounds how much of a bad frame is kept for a Status line.
const FaultPreviewBytes = 160

// FrameFault describes one rejected frame. Preview is truncated to
// FaultPreviewBytes and keeps raw bytes, matching IrcFrameFault.
type FrameFault struct {
	Err       Error
	Preview   string
	ByteCount int
}

// FrameResult is one Feed result: the complete frames plus any faults.
type FrameResult struct {
	Frames []string
	Faults []FrameFault
}

// Framer reassembles IRC frames from a byte stream. It mirrors IrcFramer in
// src/irc/ircframer.cpp, including the trailing-'\r' retention while a bad
// frame is discarded.
type Framer struct {
	buffer     []byte
	discarding bool
}

// Feed appends incoming bytes and returns every complete frame and fault it
// can now resolve.
func (f *Framer) Feed(incoming []byte) FrameResult {
	var result FrameResult
	f.buffer = append(f.buffer, incoming...)

	for {
		if f.discarding {
			delimiter := bytes.Index(f.buffer, []byte("\r\n"))
			if delimiter == -1 {
				f.retainTrailingCarriageReturn()
				return result
			}
			f.buffer = f.buffer[delimiter+2:]
			f.discarding = false
			continue
		}

		delimiter := bytes.Index(f.buffer, []byte("\r\n"))
		if delimiter != -1 {
			nul := bytes.IndexByte(f.buffer, 0)
			if nul != -1 && nul < delimiter {
				f.recordFault(&result, ErrorInvalidCharacter, f.buffer[:delimiter])
				f.buffer = f.buffer[delimiter+2:]
				continue
			}
			if !f.completeFrameIsValid(delimiter) {
				f.recordFault(&result, ErrorTooManyBytes, f.buffer[:delimiter])
				f.buffer = f.buffer[delimiter+2:]
				continue
			}

			result.Frames = append(result.Frames, string(f.buffer[:delimiter]))
			f.buffer = f.buffer[delimiter+2:]
			continue
		}

		if bytes.IndexByte(f.buffer, 0) != -1 {
			f.beginDiscard(&result, ErrorInvalidCharacter)
			continue
		}
		if f.pendingFrameIsOverlong() {
			f.beginDiscard(&result, ErrorTooManyBytes)
			continue
		}
		return result
	}
}

func (f *Framer) completeFrameIsValid(delimiter int) bool {
	if len(f.buffer) == 0 || f.buffer[0] != '@' {
		return delimiter+2 <= MaxInboundClassicFrameBytes
	}

	space := bytes.IndexByte(f.buffer, ' ')
	if space == -1 || space >= delimiter {
		return delimiter+2 <= MaxInboundClassicFrameBytes
	}

	tagPayloadBytes := space - 1
	classicFrameBytes := delimiter - space - 1 + 2
	return tagPayloadBytes <= MaxTagSectionBytes &&
		classicFrameBytes <= MaxInboundClassicFrameBytes
}

func (f *Framer) pendingFrameIsOverlong() bool {
	if len(f.buffer) == 0 || f.buffer[0] != '@' {
		return len(f.buffer) >= MaxInboundClassicFrameBytes
	}

	space := bytes.IndexByte(f.buffer, ' ')
	if space == -1 {
		return len(f.buffer) > 1+MaxTagSectionBytes
	}
	if space > 0 && space-1 > MaxTagSectionBytes {
		return true
	}

	return len(f.buffer)-space-1 >= MaxInboundClassicFrameBytes
}

func (f *Framer) recordFault(result *FrameResult, err Error, raw []byte) {
	take := len(raw)
	if take > FaultPreviewBytes {
		take = FaultPreviewBytes
	}
	result.Faults = append(result.Faults, FrameFault{
		Err:       err,
		Preview:   string(raw[:take]),
		ByteCount: len(raw),
	})
}

func (f *Framer) beginDiscard(result *FrameResult, err Error) {
	f.recordFault(result, err, f.buffer)
	f.discarding = true
	keepCarriageReturn := len(f.buffer) > 0 && f.buffer[len(f.buffer)-1] == '\r'
	f.buffer = f.buffer[:0]
	if keepCarriageReturn {
		f.buffer = append(f.buffer, '\r')
	}
}

// retainTrailingCarriageReturn drops the buffer but keeps a lone trailing
// '\r', so a CRLF split across Feed calls still terminates the discard.
func (f *Framer) retainTrailingCarriageReturn() {
	keepCarriageReturn := len(f.buffer) > 0 && f.buffer[len(f.buffer)-1] == '\r'
	f.buffer = f.buffer[:0]
	if keepCarriageReturn {
		f.buffer = append(f.buffer, '\r')
	}
}
