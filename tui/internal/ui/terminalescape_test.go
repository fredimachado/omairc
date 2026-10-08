package ui

import (
	"strings"
	"testing"
)

// TestTranscriptDropsRemoteTerminalEscapes pins that a PRIVMSG cannot steer the
// terminal. Bubble Tea passes SGR and OSC 8 inside view strings through to the
// tty, so before the session sanitized frames a remote user could recolor the
// transcript or plant a hyperlink whose label hides an unvetted URL.
func TestTranscriptDropsRemoteTerminalEscapes(t *testing.T) {
	m, d := unseenDemoModel(t)
	d.InjectOmarchy([]byte("@msgid=escape-1 :nora!u@h PRIVMSG #omarchy :" +
		"PWN \x1b[41mred\x1b[0m \x1b]8;;https://evil.example\x07click\x1b]8;;\x07 \x1b]52;c;aGk=\x07 \u009b2J end\r\n"))
	m = updateMsg(t, m, NotifyMsg{})

	content := m.View().Content
	for _, forbidden := range []string{"\x1b[41m", "\x1b]8;;", "\x1b]52;", "\u009b", "\a"} {
		if strings.Contains(content, forbidden) {
			t.Fatalf("rendered frame carries %q", forbidden)
		}
	}
	plain := ansiPattern.ReplaceAllString(content, "")
	for _, wanted := range []string{"PWN [41mred[0m ]8;;https://evil.exampleclick]8;;", "]52;c;aGk= 2J end"} {
		if !strings.Contains(plain, wanted) {
			t.Fatalf("frame is missing %q as inert text:\n%s", wanted, plain)
		}
	}
}
