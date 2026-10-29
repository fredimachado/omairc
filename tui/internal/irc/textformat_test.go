package irc

import (
	"fmt"
	"slices"
	"strings"
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

func palette(index uint8) *IrcColor {
	c := PaletteColor(index)
	return &c
}

func rgb(r, g, b uint8) *IrcColor {
	c := RGBColor(r, g, b)
	return &c
}

func styled(text string, style Style) StyledSegment {
	return StyledSegment{Style: style, Text: text}
}

func assertStyledSegments(t *testing.T, text string, want []StyledSegment) {
	t.Helper()
	got := StyledSegments(text)
	if !slices.EqualFunc(got, want, styledSegmentEqual) {
		t.Errorf("StyledSegments(%q) = %#v, want %#v", text, got, want)
	}
}

func styledSegmentEqual(a, b StyledSegment) bool {
	return a.Text == b.Text && styleEqual(a.Style, b.Style)
}

func styleEqual(a, b Style) bool {
	return a.Bold == b.Bold &&
		a.Italic == b.Italic &&
		a.Underline == b.Underline &&
		a.Reverse == b.Reverse &&
		a.Monospace == b.Monospace &&
		a.Strikethrough == b.Strikethrough &&
		ircColorPtrEqual(a.Fg, b.Fg) &&
		ircColorPtrEqual(a.Bg, b.Bg)
}

func ircColorPtrEqual(a, b *IrcColor) bool {
	if a == nil || b == nil {
		return a == b
	}
	return *a == *b
}

func concatStyledText(segs []StyledSegment) string {
	var b strings.Builder
	for _, seg := range segs {
		b.WriteString(seg.Text)
	}
	return b.String()
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

// TestStyledSegmentsAllSixteenPaletteEntries proves each mIRC index 0-15 sets
// the foreground to a palette color, mirroring Rust apply_mirc_color.
func TestStyledSegmentsAllSixteenPaletteEntries(t *testing.T) {
	for index := uint8(0); index < 16; index++ {
		input := fmt.Sprintf("\x03%02dx", index)
		want := []StyledSegment{styled("x", Style{Fg: palette(index)})}
		assertStyledSegments(t, input, want)
	}
}

// TestStyledSegmentsForegroundOnly covers single-digit and two-digit fg codes.
func TestStyledSegmentsForegroundOnly(t *testing.T) {
	assertStyledSegments(t, "\x034red",
		[]StyledSegment{styled("red", Style{Fg: palette(4)})})
	assertStyledSegments(t, "\x0304red",
		[]StyledSegment{styled("red", Style{Fg: palette(4)})})
}

// TestStyledSegmentsCommaPairSetsBackground mirrors Rust
// a_comma_pair_sets_the_background_and_a_bare_code_resets_both.
func TestStyledSegmentsCommaPairSetsBackground(t *testing.T) {
	assertStyledSegments(t, "\x0304,12a\x03b",
		[]StyledSegment{
			styled("a", Style{Fg: palette(4), Bg: palette(12)}),
			styled("b", Style{}),
		})
}

// TestStyledSegmentsBareColorReset clears fg and bg without clearing emphasis.
func TestStyledSegmentsBareColorReset(t *testing.T) {
	assertStyledSegments(t, "\x02\x0304a\x03b",
		[]StyledSegment{
			styled("a", Style{Bold: true, Fg: palette(4)}),
			styled("b", Style{Bold: true}),
		})
}

// TestStyledSegmentsFullReset clears colors and emphasis together.
func TestStyledSegmentsFullReset(t *testing.T) {
	assertStyledSegments(t, "\x02\x0304a\x0fb",
		[]StyledSegment{
			styled("a", Style{Bold: true, Fg: palette(4)}),
			styled("b", Style{}),
		})
}

// TestStyledSegmentsUnsupportedPaletteIndex mirrors Rust
// a_palette_color_persists_past_a_two_digit_code_it_cannot_use: indices above
// 15 are consumed but leave the previous color alone.
func TestStyledSegmentsUnsupportedPaletteIndex(t *testing.T) {
	assertStyledSegments(t, "\x0304a\x0399b",
		[]StyledSegment{
			styled("a", Style{Fg: palette(4)}),
			styled("b", Style{Fg: palette(4)}),
		})
	assertStyledSegments(t, "\x0399x",
		[]StyledSegment{styled("x", Style{})})
}

// TestStyledSegmentsRGBPairs mirrors Rust hex_foreground_and_background_parse_to_rgb.
func TestStyledSegmentsRGBPairs(t *testing.T) {
	assertStyledSegments(t, "\x04FF0080,00ff00x",
		[]StyledSegment{
			styled("x", Style{Fg: rgb(255, 0, 128), Bg: rgb(0, 255, 0)}),
		})
	assertStyledSegments(t, "\x04FF0000red",
		[]StyledSegment{styled("red", Style{Fg: rgb(255, 0, 0)})})
}

// TestStyledSegmentsMalformedSequences preserve Rust consumption: incomplete
// hex drops only the code and keeps the previous style; a trailing comma after
// a palette fg stays as text.
func TestStyledSegmentsMalformedSequences(t *testing.T) {
	assertStyledSegments(t, "\x04ZZwarm",
		[]StyledSegment{styled("ZZwarm", Style{})})
	assertStyledSegments(t, "\x0304a\x04ZZb",
		[]StyledSegment{
			styled("a", Style{Fg: palette(4)}),
			styled("ZZb", Style{Fg: palette(4)}),
		})
	assertStyledSegments(t, "\x0304,",
		[]StyledSegment{styled(",", Style{Fg: palette(4)})})
	assertStyledSegments(t, "\x03123,4x",
		[]StyledSegment{styled("3,4x", Style{Fg: palette(12)})})
}

// TestStyledSegmentsMixedEmphasisAndColors keeps bold/italic with palette colors.
func TestStyledSegmentsMixedEmphasisAndColors(t *testing.T) {
	assertStyledSegments(t, "\x02\x0304bold red\x03\x02 plain",
		[]StyledSegment{
			styled("bold red", Style{Bold: true, Fg: palette(4)}),
			styled(" plain", Style{}),
		})
	assertStyledSegments(t, "\x0304\x1dred italic\x1d\x03",
		[]StyledSegment{
			styled("red italic", Style{Italic: true, Fg: palette(4)}),
		})
}

// TestStyledSegmentsMultibyteText keeps UTF-8 intact under color and emphasis.
func TestStyledSegmentsMultibyteText(t *testing.T) {
	assertStyledSegments(t, "\x02\x0304héllo → 世界\x0f",
		[]StyledSegment{
			styled("héllo → 世界", Style{Bold: true, Fg: palette(4)}),
		})
}

// TestStyledSegmentsConcatEqualsPlainIrcText proves every visible byte lands
// in a segment and the concatenation matches PlainIrcText.
func TestStyledSegmentsConcatEqualsPlainIrcText(t *testing.T) {
	cases := []string{
		"plain",
		"\x02bold\x02",
		"\x0304red\x03",
		"\x0304,12a\x03b",
		"\x04FF0080,00ff00x",
		"\x02héllo\x0304 → \x0f世界\x04FF0000!",
		"\x0304a\x0399b",
		"\x04ZZwarm",
		"\x02a\x16b\x11c\x1ed\x02",
		"\x03123,4x",
	}
	for _, input := range cases {
		got := concatStyledText(StyledSegments(input))
		want := PlainIrcText(input)
		if got != want {
			t.Errorf("concat(StyledSegments(%q)) = %q, PlainIrcText = %q", input, got, want)
		}
	}
}

// TestStyledSegmentsReverseMonospaceStrikethroughAreStyles mirrors Rust
// reverse_monospace_and_strikethrough_are_styles_in_their_own_right.
func TestStyledSegmentsReverseMonospaceStrikethroughAreStyles(t *testing.T) {
	got := StyledSegments("\x16r\x11m\x1es")
	if len(got) != 3 {
		t.Fatalf("len = %d, want 3; got %#v", len(got), got)
	}
	last := got[2].Style
	if !last.Reverse || !last.Monospace || !last.Strikethrough {
		t.Errorf("last style = %#v, want reverse+monospace+strikethrough", last)
	}
}
