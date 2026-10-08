package termtext

import "testing"

func TestSanitize(t *testing.T) {
	cases := []struct {
		name string
		in   string
		want string
	}{
		{"plain text is untouched", "hello, world ünïcode 日本", "hello, world ünïcode 日本"},
		{"SGR loses its ESC", "a\x1b[41mred\x1b[0mb", "a[41mred[0mb"},
		{"OSC 8 hyperlink loses its ESCs", "\x1b]8;;https://evil.example\x07click\x1b]8;;\x07", "]8;;https://evil.exampleclick]8;;"},
		{"UTF-8 C1 CSI is dropped", "x\u009b2Jy", "x2Jy"},
		{"Latin-1 C1 CSI byte is dropped", "caf\xe9 \x9b2J", "caf\xe9 2J"},
		{"DEL and BEL are dropped", "a\x7fb\x07c\x08d", "abcd"},
		{"mIRC formatting and CTCP survive", "\x01ACTION \x02b\x02 \x0304,01c\x03 \x04ff0000h \x0f\x11\x16\x1d\x1e\x1f\x01", "\x01ACTION \x02b\x02 \x0304,01c\x03 \x04ff0000h \x0f\x11\x16\x1d\x1e\x1f\x01"},
		{"tab, CR, and LF survive for the parser", "a\tb\rc\nd", "a\tb\rc\nd"},
		{"valid UTF-8 continuation bytes are not C1", "\u00db\u0100", "\u00db\u0100"},
	}
	for _, tc := range cases {
		if got := Sanitize(tc.in); got != tc.want {
			t.Errorf("%s: Sanitize(%q) = %q, want %q", tc.name, tc.in, got, tc.want)
		}
	}
}
