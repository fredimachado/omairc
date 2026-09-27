package irc

import "testing"

// TestPlainIrcTextBold covers the exact feature-map example.
func TestPlainIrcTextBold(t *testing.T) {
	if got := PlainIrcText("hey \x02fred"); got != "hey fred" {
		t.Errorf("PlainIrcText(bold) = %q, want %q", got, "hey fred")
	}
}

// TestPlainIrcTextMircColors covers color runs, the reset code, single-digit
// colors, a trailing bare color, and a comma with no digits.
func TestPlainIrcTextMircColors(t *testing.T) {
	cases := []struct {
		in   string
		want string
	}{
		{"\x0304red\x0fplain", "redplain"},
		{"\x0304,05red", "red"},
		{"\x034red", "red"},
		{"\x03", ""},
		{"\x0304,", ","},
	}
	for _, testCase := range cases {
		if got := PlainIrcText(testCase.in); got != testCase.want {
			t.Errorf("PlainIrcText(%q) = %q, want %q", testCase.in, got, testCase.want)
		}
	}
}

// TestPlainIrcTextHexColors covers hex color runs with and without a
// background pair.
func TestPlainIrcTextHexColors(t *testing.T) {
	cases := []struct {
		in   string
		want string
	}{
		{"\x04000000blue", "blue"},
		{"\x04000000,ffffffblue", "blue"},
	}
	for _, testCase := range cases {
		if got := PlainIrcText(testCase.in); got != testCase.want {
			t.Errorf("PlainIrcText(%q) = %q, want %q", testCase.in, got, testCase.want)
		}
	}
}

// TestPlainIrcTextControlCodes proves all six formatting control codes are
// dropped while their neighbours survive.
func TestPlainIrcTextControlCodes(t *testing.T) {
	if got := PlainIrcText("\x02b\x0f\x11m\x16r\x1d\x1e\x1f"); got != "bmr" {
		t.Errorf("PlainIrcText(control codes) = %q, want %q", got, "bmr")
	}
}

// TestPlainIrcTextNonASCII proves byte-indexed stripping leaves multibyte text
// intact.
func TestPlainIrcTextNonASCII(t *testing.T) {
	const text = "\x02héllo → 世界\x0f"
	if got := PlainIrcText(text); got != "héllo → 世界" {
		t.Errorf("PlainIrcText(non-ASCII) = %q, want %q", got, "héllo → 世界")
	}
}
