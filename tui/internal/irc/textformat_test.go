package irc

import (
	"slices"
	"testing"
)

// assertEmphasizedSegments compares the styled runs for text against want.
func assertEmphasizedSegments(t *testing.T, text string, want []Segment) {
	t.Helper()
	got := EmphasizedSegments(text)
	if !slices.Equal(got, want) {
		t.Errorf("EmphasizedSegments(%q) = %#v, want %#v", text, got, want)
	}
}

// TestEmphasizedSegmentsBoldItalicUnderline mirrors
// IrcTextFormatterTest::emphasizedBoldItalicUnderline.
func TestEmphasizedSegmentsBoldItalicUnderline(t *testing.T) {
	assertEmphasizedSegments(t, "hello \x02world\x02",
		[]Segment{{Text: "hello "}, {Bold: true, Text: "world"}})
	assertEmphasizedSegments(t, "\x1ditalic\x1d",
		[]Segment{{Italic: true, Text: "italic"}})
	assertEmphasizedSegments(t, "\x1funder\x1f",
		[]Segment{{Underline: true, Text: "under"}})
}

// TestEmphasizedSegmentsNestedBoldItalic mirrors
// IrcTextFormatterTest::emphasizedNestedClosesInnerFirst: the closing order of
// the tags does not matter for runs, only the exact flags and texts do.
func TestEmphasizedSegmentsNestedBoldItalic(t *testing.T) {
	assertEmphasizedSegments(t, "\x02bold\x1ditalic\x1d\x02",
		[]Segment{
			{Bold: true, Text: "bold"},
			{Bold: true, Italic: true, Text: "italic"},
		})
}

// TestEmphasizedSegmentsResetMidText proves 0x0f closes every open run and
// the text after it is plain.
func TestEmphasizedSegmentsResetMidText(t *testing.T) {
	assertEmphasizedSegments(t, "\x02a\x0fb",
		[]Segment{{Bold: true, Text: "a"}, {Text: "b"}})
	assertEmphasizedSegments(t, "\x02\x1d\x1f\x0f",
		nil)
}

// TestEmphasizedSegmentsDropsExtraCodes proves reverse, monospace, and
// strikethrough are dropped without splitting an open bold run, mirroring
// IrcTextFormatterTest::emphasizedReverseDoesNotSplitBold and
// emphasizedExtraCodesStayInsideBold.
func TestEmphasizedSegmentsDropsExtraCodes(t *testing.T) {
	assertEmphasizedSegments(t, "\x02a\x16b\x02",
		[]Segment{{Bold: true, Text: "ab"}})
	assertEmphasizedSegments(t, "\x02a\x11b\x1ec\x02",
		[]Segment{{Bold: true, Text: "abc"}})
}

// TestEmphasizedSegmentsColorOnly mirrors
// IrcTextFormatterTest::emphasizedColorOnly: a color-only string has no
// emphasis and yields one plain segment with the visible text.
func TestEmphasizedSegmentsColorOnly(t *testing.T) {
	assertEmphasizedSegments(t, "\x0304red\x03",
		[]Segment{{Text: "red"}})
}

// TestEmphasizedSegmentsLiteralHTML proves literal markup stays literal text
// rather than becoming emphasis.
func TestEmphasizedSegmentsLiteralHTML(t *testing.T) {
	assertEmphasizedSegments(t, "<b>not html</b>",
		[]Segment{{Text: "<b>not html</b>"}})
}

// TestEmphasizedSegmentsHexColor mirrors
// IrcTextFormatterTest::emphasizedHexColor: the hex run is dropped and never
// leaks into the text.
func TestEmphasizedSegmentsHexColor(t *testing.T) {
	assertEmphasizedSegments(t, "\x04FF0000red",
		[]Segment{{Text: "red"}})
}

// TestEmphasizedSegmentsPreservesDoubleSpaces mirrors
// IrcTextFormatterTest::emphasizedPreservesDoubleSpaces.
func TestEmphasizedSegmentsPreservesDoubleSpaces(t *testing.T) {
	assertEmphasizedSegments(t, "a  \x02b\x02",
		[]Segment{{Text: "a  "}, {Bold: true, Text: "b"}})
}

// TestEmphasizedSegmentsNonASCII proves byte-indexed scanning leaves multibyte
// text intact byte-for-byte while still honoring surrounding emphasis.
func TestEmphasizedSegmentsNonASCII(t *testing.T) {
	assertEmphasizedSegments(t, "\x02héllo → 世界\x0f",
		[]Segment{{Bold: true, Text: "héllo → 世界"}})
}

// TestHasIrcEmphasis mirrors IrcTextFormatterTest::hasIrcEmphasisIgnoresStrippedCodes.
func TestHasIrcEmphasis(t *testing.T) {
	cases := []struct {
		in   string
		want bool
	}{
		{"\x16flip\x16", false},
		{"\x11mono\x11", false},
		{"\x0304red\x03", false},
		{"\x04FF0000red", false},
		{"hello \x02world\x02", true},
		{"\x1ditalic\x1d", true},
		{"\x1funder\x1f", true},
	}
	for _, testCase := range cases {
		if got := HasIrcEmphasis(testCase.in); got != testCase.want {
			t.Errorf("HasIrcEmphasis(%q) = %v, want %v", testCase.in, got, testCase.want)
		}
	}
}
