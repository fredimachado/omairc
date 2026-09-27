package irc

import "testing"

// TestAvatarHTTPHeaderValueCaseInsensitive proves the header name matches
// regardless of case on either side.
func TestAvatarHTTPHeaderValueCaseInsensitive(t *testing.T) {
	block := []byte("Content-Type: image/png\r\nContent-Length: 5\r\n")
	if got := AvatarHTTPHeaderValue(block, []byte("content-length")); string(got) != "5" {
		t.Errorf("lowercase lookup = %q, want %q", got, "5")
	}
	if got := AvatarHTTPHeaderValue(block, []byte("CONTENT-TYPE")); string(got) != "image/png" {
		t.Errorf("uppercase lookup = %q, want %q", got, "image/png")
	}
	if got := AvatarHTTPHeaderValue(block, []byte("content-Length")); string(got) != "5" {
		t.Errorf("mixed-case lookup = %q, want %q", got, "5")
	}
}

// TestAvatarHTTPHeaderValueMissing proves an absent header yields an empty
// value.
func TestAvatarHTTPHeaderValueMissing(t *testing.T) {
	block := []byte("Content-Type: image/png\r\n")
	if got := AvatarHTTPHeaderValue(block, []byte("X-Missing")); len(got) != 0 {
		t.Errorf("missing header = %q, want empty", got)
	}
	if got := AvatarHTTPHeaderValue(nil, []byte("Content-Type")); len(got) != 0 {
		t.Errorf("nil block = %q, want empty", got)
	}
}

// TestAvatarHTTPHeaderValueLeadingBlankLine proves the leading CRLF is skipped.
func TestAvatarHTTPHeaderValueLeadingBlankLine(t *testing.T) {
	block := []byte("\r\nContent-Type: image/png\r\nContent-Length: 7\r\n")
	if got := AvatarHTTPHeaderValue(block, []byte("content-length")); string(got) != "7" {
		t.Errorf("leading blank line = %q, want %q", got, "7")
	}
}

// TestAvatarHTTPHeaderValueTrims proves the returned value is trimmed while
// the surrounding header whitespace is preserved by the match.
func TestAvatarHTTPHeaderValueTrims(t *testing.T) {
	block := []byte("ETag:   abc  \r\n")
	if got := AvatarHTTPHeaderValue(block, []byte("etag")); string(got) != "abc" {
		t.Errorf("trimmed value = %q, want %q", got, "abc")
	}
}

// TestAvatarHTTPHeaderValueStopsAtEmptyLine proves lookup stops at the first
// empty line and never reaches a header that follows it.
func TestAvatarHTTPHeaderValueStopsAtEmptyLine(t *testing.T) {
	block := []byte("A: 1\r\n\r\nB: 2\r\n")
	if got := AvatarHTTPHeaderValue(block, []byte("B")); len(got) != 0 {
		t.Errorf("header after empty line = %q, want empty", got)
	}
}

// TestAvatarHTTPTransferEncodingIsChunkedPresent proves a chunked value is
// detected, including as one token in a list.
func TestAvatarHTTPTransferEncodingIsChunkedPresent(t *testing.T) {
	if !AvatarHTTPTransferEncodingIsChunked([]byte("Transfer-Encoding: chunked\r\n")) {
		t.Error("plain chunked not detected")
	}
	if !AvatarHTTPTransferEncodingIsChunked([]byte("Transfer-Encoding: gzip, chunked\r\n")) {
		t.Error("chunked token in a list not detected")
	}
}

// TestAvatarHTTPTransferEncodingIsChunkedAbsent proves a non-chunked or
// missing header is not treated as chunked.
func TestAvatarHTTPTransferEncodingIsChunkedAbsent(t *testing.T) {
	if AvatarHTTPTransferEncodingIsChunked([]byte("Content-Length: 5\r\n")) {
		t.Error("missing Transfer-Encoding treated as chunked")
	}
	if AvatarHTTPTransferEncodingIsChunked([]byte("Transfer-Encoding: gzip\r\n")) {
		t.Error("gzip-only encoding treated as chunked")
	}
	if AvatarHTTPTransferEncodingIsChunked(nil) {
		t.Error("nil block treated as chunked")
	}
	if AvatarHTTPTransferEncodingIsChunked([]byte("\r\n")) {
		t.Error("empty block treated as chunked")
	}
}

// TestAvatarHTTPTransferEncodingIsChunkedMixedCase proves the value match is
// case-insensitive.
func TestAvatarHTTPTransferEncodingIsChunkedMixedCase(t *testing.T) {
	if !AvatarHTTPTransferEncodingIsChunked([]byte("transfer-encoding: CHUNKED\r\n")) {
		t.Error("mixed-case chunked not detected")
	}
}

// TestAvatarHTTPDecodeChunkedBodySingleChunk covers one data chunk terminated
// by a zero chunk.
func TestAvatarHTTPDecodeChunkedBodySingleChunk(t *testing.T) {
	got, ok := AvatarHTTPDecodeChunkedBody([]byte("5\r\nhello\r\n0\r\n\r\n"))
	if !ok {
		t.Fatal("ok = false, want true")
	}
	if string(got) != "hello" {
		t.Errorf("body = %q, want %q", got, "hello")
	}
}

// TestAvatarHTTPDecodeChunkedBodyMultipleChunks covers several data chunks.
func TestAvatarHTTPDecodeChunkedBodyMultipleChunks(t *testing.T) {
	got, ok := AvatarHTTPDecodeChunkedBody(
		[]byte("3\r\nfoo\r\n3\r\nbar\r\n1\r\n!\r\n0\r\n\r\n"))
	if !ok {
		t.Fatal("ok = false, want true")
	}
	if string(got) != "foobar!" {
		t.Errorf("body = %q, want %q", got, "foobar!")
	}
}

// TestAvatarHTTPDecodeChunkedBodyZeroChunkTerminates proves the decoder stops
// at the zero chunk and ignores anything that follows it.
func TestAvatarHTTPDecodeChunkedBodyZeroChunkTerminates(t *testing.T) {
	got, ok := AvatarHTTPDecodeChunkedBody([]byte("0\r\n"))
	if !ok || len(got) != 0 {
		t.Errorf("zero chunk only = (%q, %v), want (empty, true)", got, ok)
	}
	got, ok = AvatarHTTPDecodeChunkedBody([]byte("2\r\nhi\r\n0\r\ntrailing"))
	if !ok || string(got) != "hi" {
		t.Errorf("trailing after zero chunk = (%q, %v), want (%q, true)", got, ok, "hi")
	}
}

// TestAvatarHTTPDecodeChunkedBodyMissingFinalCRLF proves a chunk without its
// trailing CRLF fails.
func TestAvatarHTTPDecodeChunkedBodyMissingFinalCRLF(t *testing.T) {
	if got, ok := AvatarHTTPDecodeChunkedBody([]byte("5\r\nhello")); ok || len(got) != 0 {
		t.Errorf("missing final CRLF = (%q, %v), want (empty, false)", got, ok)
	}
	if got, ok := AvatarHTTPDecodeChunkedBody([]byte("5\r\nhelloXX0\r\n")); ok || len(got) != 0 {
		t.Errorf("wrong chunk delimiter = (%q, %v), want (empty, false)", got, ok)
	}
}

// TestAvatarHTTPDecodeChunkedBodyTruncated proves a body shorter than its
// declared chunk size fails.
func TestAvatarHTTPDecodeChunkedBodyTruncated(t *testing.T) {
	if got, ok := AvatarHTTPDecodeChunkedBody([]byte("5\r\nhel")); ok || len(got) != 0 {
		t.Errorf("truncated body = (%q, %v), want (empty, false)", got, ok)
	}
}

// TestAvatarHTTPDecodeChunkedBodyBadHexSize proves an unparsable or negative
// size line fails.
func TestAvatarHTTPDecodeChunkedBodyBadHexSize(t *testing.T) {
	if got, ok := AvatarHTTPDecodeChunkedBody([]byte("zz\r\nhello\r\n0\r\n")); ok || len(got) != 0 {
		t.Errorf("bad hex size = (%q, %v), want (empty, false)", got, ok)
	}
	if got, ok := AvatarHTTPDecodeChunkedBody([]byte("-1\r\n")); ok || len(got) != 0 {
		t.Errorf("negative size = (%q, %v), want (empty, false)", got, ok)
	}
	if got, ok := AvatarHTTPDecodeChunkedBody([]byte("5hello")); ok || len(got) != 0 {
		t.Errorf("size line without CRLF = (%q, %v), want (empty, false)", got, ok)
	}
}

// TestAvatarHTTPDecodeChunkedBodyEmptyBody proves an empty body fails, because
// it never reaches a zero-size chunk.
func TestAvatarHTTPDecodeChunkedBodyEmptyBody(t *testing.T) {
	if got, ok := AvatarHTTPDecodeChunkedBody(nil); ok || len(got) != 0 {
		t.Errorf("empty body = (%q, %v), want (empty, false)", got, ok)
	}
}
