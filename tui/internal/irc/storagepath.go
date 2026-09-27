package irc

import (
	"crypto/sha256"
	"encoding/hex"
	"strings"
)

// This file ports the pure half of src/irc/ircstoragepath.cpp: the segment
// encoders and decoders. The filesystem helpers (legacyStoragePathExists,
// legacyStorageDirExists, storageCanonicalPath, storagePathsSameFile, and
// storageDirSegmentsShareLocation) belong to a later internal/storage package
// and are deliberately absent here.

// isSafeStorageByte reports whether byteValue may appear unescaped in a
// segment. A trailing '.' is unsafe because Windows strips it from names.
func isSafeStorageByte(byteValue byte, isLastByte bool) bool {
	if byteValue >= 'a' && byteValue <= 'z' {
		return true
	}
	if byteValue >= '0' && byteValue <= '9' {
		return true
	}
	switch byteValue {
	case '#', '&', '+', '-', '_', '{', '}', '~', '[', ']', '^':
		return true
	case '.':
		return !isLastByte
	default:
		return false
	}
}

// percentEncodeByte renders one byte as '%' plus two lowercase hex digits.
func percentEncodeByte(byteValue byte) string {
	const hexDigits = "0123456789abcdef"
	return "%" + string([]byte{hexDigits[byteValue>>4], hexDigits[byteValue&0x0f]})
}

// encodeStorageBytes percent-encodes every unsafe byte of utf8.
func encodeStorageBytes(utf8 []byte) string {
	var builder strings.Builder
	builder.Grow(len(utf8) * 3)
	for index := 0; index < len(utf8); index++ {
		current := utf8[index]
		if isSafeStorageByte(current, index == len(utf8)-1) {
			builder.WriteByte(current)
		} else {
			builder.WriteString(percentEncodeByte(current))
		}
	}
	return builder.String()
}

// encodeWithLeadingEscape percent-encodes the first byte unconditionally and
// the rest with encodeStorageBytes. It breaks Win32 device names like "con"
// while keeping the remainder readable.
func encodeWithLeadingEscape(utf8 []byte) string {
	if len(utf8) == 0 {
		return ""
	}
	return percentEncodeByte(utf8[0]) + encodeStorageBytes(utf8[1:])
}

// hashLongSegment hashes the original UTF-8 bytes into the "h" + lowercase hex
// SHA-256 form used for segments that would overflow the length cap.
func hashLongSegment(utf8 []byte) string {
	sum := sha256.Sum256(utf8)
	return "h" + hex.EncodeToString(sum[:])
}

// trimTrailingSpacesAndDots mirrors the C++ chop loop over ' ' and '.'.
func trimTrailingSpacesAndDots(text string) string {
	index := len(text)
	for index > 0 && (text[index-1] == ' ' || text[index-1] == '.') {
		index--
	}
	return text[:index]
}

// deviceBaseToken trims trailing spaces and dots from fullName, then returns
// everything before the first '.'. It mirrors deviceBaseToken in the C++.
func deviceBaseToken(fullName string) string {
	fullName = trimTrailingSpacesAndDots(fullName)
	if dot := strings.IndexByte(fullName, '.'); dot >= 0 {
		return fullName[:dot]
	}
	return fullName
}

// deviceBaseMatches reports whether a stem names a Win32 reserved device:
// con/prn/aux/nul, or com/lpt followed by a digit or a superscript digit.
func deviceBaseMatches(stem string) bool {
	stem = trimTrailingSpacesAndDots(stem)
	if stem == "" {
		return false
	}

	lowered := asciiLowerString(stem)
	if lowered == "con" || lowered == "prn" || lowered == "aux" || lowered == "nul" {
		return true
	}

	runes := []rune(lowered)
	if len(runes) != 4 {
		return false
	}

	prefix := string(runes[:3])
	if prefix != "com" && prefix != "lpt" {
		return false
	}

	suffix := runes[3]
	if suffix >= '0' && suffix <= '9' {
		return true
	}
	return suffix == '\u00B9' || suffix == '\u00B2' || suffix == '\u00B3'
}

// isWin32DeviceName reports whether text combined with extension names a Win32
// reserved device. It is evaluted on every platform, matching the C++.
func isWin32DeviceName(text, extension string) bool {
	return deviceBaseMatches(deviceBaseToken(text + extension))
}

// OmaircStorageSegment encodes text as an on-disk path segment. The optional
// extensionForDeviceCheck is appended only for the long-segment/device-name
// checks, mirroring the C++ default argument.
func OmaircStorageSegment(text string, extensionForDeviceCheck ...string) string {
	extension := ""
	if len(extensionForDeviceCheck) > 0 {
		extension = extensionForDeviceCheck[0]
	}

	if text == "" || text == "." || text == ".." {
		return "_"
	}

	utf8 := []byte(text)
	var segment string
	if isWin32DeviceName(text, extension) {
		segment = encodeWithLeadingEscape(utf8)
	} else {
		segment = encodeStorageBytes(utf8)
	}
	if len(segment)+len(extension) > 255 {
		return hashLongSegment(utf8)
	}
	if OmaircStorageSegmentIsHash(segment) {
		segment = percentEncodeByte('h') + segment[1:]
	}
	return segment
}

// OmaircTargetSegment normalizes target through mapping, then encodes it with
// OmaircWireStorageSegment semantics (wire text then segment encoding).
func OmaircTargetSegment(target string, mapping CaseMapping, extensionForDeviceCheck ...string) string {
	normalized := mapping.Normalize(target)
	return OmaircStorageSegment(WireText([]byte(normalized)), extensionForDeviceCheck...)
}

// OmaircWireStorageSegment applies WireText to text's UTF-8 bytes first, then
// OmaircStorageSegment.
func OmaircWireStorageSegment(text string, extensionForDeviceCheck ...string) string {
	return OmaircStorageSegment(WireText([]byte(text)), extensionForDeviceCheck...)
}

// LegacyStorageSegment is the pre-Omairc segment encoding.
func LegacyStorageSegment(text string) string {
	text = strings.ReplaceAll(text, "%", "%25")
	text = strings.ReplaceAll(text, "/", "%2f")
	text = strings.ReplaceAll(text, "\\", "%5c")
	text = strings.ReplaceAll(text, "\x00", "%00")
	if text == "" || text == "." || text == ".." {
		return "_"
	}
	return text
}

// DecodeLegacySegment reverses LegacyStorageSegment.
func DecodeLegacySegment(text string) string {
	text = strings.ReplaceAll(text, "%00", "\x00")
	text = strings.ReplaceAll(text, "%5c", "\\")
	text = strings.ReplaceAll(text, "%2f", "/")
	text = strings.ReplaceAll(text, "%25", "%")
	return text
}

// OmaircStorageSegmentIsHash reports whether segment is a hashed long segment
// ("h" + 64 lowercase hex chars).
func OmaircStorageSegmentIsHash(segment string) bool {
	if len(segment) != 65 || segment[0] != 'h' {
		return false
	}
	for index := 1; index < len(segment); index++ {
		character := segment[index]
		if (character >= '0' && character <= '9') || (character >= 'a' && character <= 'f') {
			continue
		}
		return false
	}
	return true
}

// DecodeOmaircStorageSegment reverses OmaircStorageSegment (hashes decode to
// themselves).
func DecodeOmaircStorageSegment(segment string) string {
	if OmaircStorageSegmentIsHash(segment) {
		return segment
	}

	decodedBytes := make([]byte, 0, len(segment))
	for index := 0; index < len(segment); {
		if segment[index] == '%' && index+2 < len(segment) {
			if value, ok := hexByteValue(segment[index+1], segment[index+2]); ok {
				decodedBytes = append(decodedBytes, value)
				index += 3
				continue
			}
		}
		decodedBytes = append(decodedBytes, segment[index])
		index++
	}

	// The C++ decodes with QString::fromUtf8 and falls back to Latin-1 when the
	// result is empty but the bytes are not; keep the same guard.
	decoded := string(decodedBytes)
	if decoded == "" && len(decodedBytes) != 0 {
		return latin1BytesToString(decodedBytes)
	}
	return decoded
}

// hexByteValue decodes two ASCII hex digits (either case) into one byte.
func hexByteValue(high, low byte) (byte, bool) {
	highValue, ok := hexDigitValue(high)
	if !ok {
		return 0, false
	}
	lowValue, ok := hexDigitValue(low)
	if !ok {
		return 0, false
	}
	return highValue<<4 | lowValue, true
}

// hexDigitValue maps one ASCII hex digit to its value.
func hexDigitValue(character byte) (byte, bool) {
	switch {
	case character >= '0' && character <= '9':
		return character - '0', true
	case character >= 'a' && character <= 'f':
		return character - 'a' + 10, true
	case character >= 'A' && character <= 'F':
		return character - 'A' + 10, true
	default:
		return 0, false
	}
}

// latin1BytesToString maps each byte to one rune, one-to-one.
func latin1BytesToString(raw []byte) string {
	decoded := make([]rune, len(raw))
	for index, value := range raw {
		decoded[index] = rune(value)
	}
	return string(decoded)
}
