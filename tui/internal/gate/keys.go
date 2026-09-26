package gate

import (
	"fmt"
	"strings"
)

// keyNames maps the chord vocabulary control-omairc-tui accepts to the bytes a
// terminal sends over the PTY.
//
// The shared recipes under .cursor/skills/verify-omairc/features/*.md are
// written for the Qt driver, which hands names to xdotool; this table (plus the
// generic ctrl+<letter> and alt+<letter> families in KeySequence) is the TUI
// driver's own vocabulary, not a verbatim mirror of every name a recipe may
// use. A name outside it is rejected with a descriptive error instead of
// silently sending nothing. Both the capitalised arrow/Home/End/Page spellings
// and their lowercase aliases are accepted, because the skill uses both.
var keyNames = map[string]string{
	"ctrl+q":     "\x11",
	"ctrl+c":     "\x03",
	"ctrl+l":     "\x0c",
	"ctrl+slash": "\x1f",
	"enter":      "\r",
	"Return":     "\r",
	"return":     "\r",
	"tab":        "\t",
	"Tab":        "\t",
	"space":      " ",
	"escape":     "\x1b",
	"Escape":     "\x1b",
	"backspace":  "\x7f",
	"shift+Tab":  "\x1b[Z",
	"Shift+Tab":  "\x1b[Z",
	"Up":         "\x1b[A",
	"up":         "\x1b[A",
	"Down":       "\x1b[B",
	"down":       "\x1b[B",
	"Right":      "\x1b[C",
	"right":      "\x1b[C",
	"Left":       "\x1b[D",
	"left":       "\x1b[D",
	"Page_Up":    "\x1b[5~",
	"page_up":    "\x1b[5~",
	"Page_Down":  "\x1b[6~",
	"page_down":  "\x1b[6~",
	"Home":       "\x1b[H",
	"home":       "\x1b[H",
	"End":        "\x1b[F",
	"end":        "\x1b[F",
	"alt+Down":   "\x1b[1;3B",
	"alt+Up":     "\x1b[1;3A",
	"alt+Right":  "\x1b[1;3C",
	"alt+Left":   "\x1b[1;3D",
}

// KeySequence encodes one chord name as terminal bytes.
//
// Beyond the static table it accepts a generic ctrl+<letter> (the control
// byte letter-'a'+1) and alt+<letter> (ESC followed by the letter, case
// preserved). Unknown names return a descriptive error so a recipe typo
// surfaces at the chord, not as a silent no-op.
func KeySequence(name string) ([]byte, error) {
	if b, ok := keyNames[name]; ok {
		return []byte(b), nil
	}
	if rest, ok := strings.CutPrefix(name, "ctrl+"); ok {
		if len(rest) == 1 {
			ch := rest[0]
			switch {
			case ch >= 'a' && ch <= 'z':
				return []byte{ch - 'a' + 1}, nil
			case ch >= 'A' && ch <= 'Z':
				return []byte{ch - 'A' + 1}, nil
			}
		}
	}
	if rest, ok := strings.CutPrefix(name, "alt+"); ok {
		if len(rest) == 1 {
			ch := rest[0]
			if ch >= 'a' && ch <= 'z' || ch >= 'A' && ch <= 'Z' {
				return []byte{0x1b, ch}, nil
			}
		}
	}
	return nil, fmt.Errorf("unknown key %q", name)
}
