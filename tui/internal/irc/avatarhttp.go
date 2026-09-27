package irc

import (
	"bytes"
	"strconv"
)

// AvatarHTTPHeaderValue returns the value of the named header from a raw HTTP
// header block, matching the header name case-insensitively. It mirrors
// ircAvatarHttpHeaderValue (src/irc/ircavatarhttp.cpp): a leading blank line
// (the CRLF that some callers leave in front of the block) is skipped, then
// CRLF-terminated lines are walked until an empty line, and the matched
// value is trimmed. An absent header yields an empty slice.
func AvatarHTTPHeaderValue(headerBlock, name []byte) []byte {
	// "<lowercased name>: " is the needle every header line is compared
	// against. Build it in a fresh slice so it never aliases name.
	needle := make([]byte, 0, len(name)+2)
	needle = append(needle, bytes.ToLower(name)...)
	needle = append(needle, ':', ' ')

	offset := 0
	if bytes.HasPrefix(headerBlock, []byte("\r\n")) {
		offset = 2
	}
	for offset < len(headerBlock) {
		lineEnd := bytes.Index(headerBlock[offset:], []byte("\r\n"))
		lineLength := len(headerBlock) - offset
		if lineEnd >= 0 {
			lineLength = lineEnd
		}
		line := headerBlock[offset : offset+lineLength]
		if len(line) == 0 {
			break
		}
		if len(line) >= len(needle) &&
			bytes.Equal(bytes.ToLower(line[:len(needle)]), needle) {
			return bytes.TrimSpace(line[len(needle):])
		}
		if lineEnd < 0 {
			break
		}
		offset += lineEnd + 2
	}
	return []byte{}
}

// AvatarHTTPTransferEncodingIsChunked reports whether the raw header block
// carries a Transfer-Encoding value that mentions "chunked", case-insensitively.
// It mirrors ircAvatarHttpTransferEncodingIsChunked.
func AvatarHTTPTransferEncodingIsChunked(headerBlock []byte) bool {
	return bytes.Contains(
		bytes.ToLower(AvatarHTTPHeaderValue(headerBlock, []byte("Transfer-Encoding"))),
		[]byte("chunked"))
}

// AvatarHTTPDecodeChunkedBody decodes an HTTP chunked-transfer body from raw
// bytes. It mirrors ircAvatarHttpDecodeChunkedBody (src/irc/ircavatarhttp.cpp):
// each chunk is a hex size line, the chunk bytes, and a trailing CRLF; a
// zero-size chunk terminates the body. The bool is the C++ ok out-parameter:
// false for a missing CRLF, an unparsable or negative size, a truncated chunk,
// a missing chunk delimiter, or a body that never reaches a zero-size chunk.
func AvatarHTTPDecodeChunkedBody(raw []byte) ([]byte, bool) {
	output := []byte{}
	offset := 0
	for offset < len(raw) {
		lineEnd := bytes.Index(raw[offset:], []byte("\r\n"))
		if lineEnd < 0 {
			return []byte{}, false
		}
		chunkSize, err := strconv.ParseInt(string(raw[offset:offset+lineEnd]), 16, 64)
		if err != nil || chunkSize < 0 {
			return []byte{}, false
		}
		offset += lineEnd + 2
		if chunkSize == 0 {
			return output, true
		}
		// Compare in int64 so a huge size cannot overflow int and slip past
		// the bounds check.
		if chunkSize > int64(len(raw)-offset) {
			return []byte{}, false
		}
		output = append(output, raw[offset:offset+int(chunkSize)]...)
		offset += int(chunkSize)
		if offset+2 > len(raw) ||
			!bytes.Equal(raw[offset:offset+2], []byte("\r\n")) {
			return []byte{}, false
		}
		offset += 2
	}
	return []byte{}, false
}
