package gate

import (
	"bytes"
	"testing"
)

func TestKeySequenceTable(t *testing.T) {
	cases := []struct {
		name string
		want []byte
	}{
		{"ctrl+q", []byte{0x11}},
		{"ctrl+c", []byte{0x03}},
		{"ctrl+l", []byte{0x0c}},
		{"enter", []byte{0x0d}},
		{"Return", []byte{0x0d}},
		{"return", []byte{0x0d}},
		{"tab", []byte{0x09}},
		{"Tab", []byte{0x09}},
		{"space", []byte{0x20}},
		{"escape", []byte{0x1b}},
		{"Escape", []byte{0x1b}},
		{"backspace", []byte{0x7f}},
		{"shift+Tab", []byte("\x1b[Z")},
		{"Shift+Tab", []byte("\x1b[Z")},
		{"Up", []byte("\x1b[A")},
		{"up", []byte("\x1b[A")},
		{"Down", []byte("\x1b[B")},
		{"down", []byte("\x1b[B")},
		{"Right", []byte("\x1b[C")},
		{"right", []byte("\x1b[C")},
		{"Left", []byte("\x1b[D")},
		{"left", []byte("\x1b[D")},
		{"Page_Up", []byte("\x1b[5~")},
		{"page_up", []byte("\x1b[5~")},
		{"Page_Down", []byte("\x1b[6~")},
		{"page_down", []byte("\x1b[6~")},
		{"Home", []byte("\x1b[H")},
		{"home", []byte("\x1b[H")},
		{"End", []byte("\x1b[F")},
		{"end", []byte("\x1b[F")},
		{"alt+a", []byte("\x1b" + "a")},
		{"alt+A", []byte("\x1b" + "A")},
		{"alt+Down", []byte("\x1b[1;3B")},
		{"alt+Up", []byte("\x1b[1;3A")},
		{"alt+Right", []byte("\x1b[1;3C")},
		{"alt+Left", []byte("\x1b[1;3D")},
		{"ctrl+slash", []byte{0x1f}},
		{"ctrl+a", []byte{0x01}},
		{"ctrl+z", []byte{0x1a}},
		{"ctrl+A", []byte{0x01}},
		{"ctrl+`", []byte("\x1b[96;5u")},
		{"ctrl+,", []byte("\x1b[44;5u")},
		{"ctrl+enter", []byte("\x1b[13;5u")},
		{"ctrl+tab", []byte("\x1b[9;5u")},
		{"ctrl+shift+delete", []byte("\x1b[57349;6u")},
	}
	for _, tc := range cases {
		got, err := KeySequence(tc.name)
		if err != nil {
			t.Fatalf("KeySequence(%q) error: %v", tc.name, err)
		}
		if !bytes.Equal(got, tc.want) {
			t.Fatalf("KeySequence(%q) = %v, want %v", tc.name, got, tc.want)
		}
	}
}

func TestKeySequenceUnknown(t *testing.T) {
	for _, name := range []string{"", "ctrl+", "alt+", "ctrl+shift+a", "F13", "meta+x", "ctrl+1"} {
		if _, err := KeySequence(name); err == nil {
			t.Fatalf("KeySequence(%q) should error", name)
		}
	}
}
