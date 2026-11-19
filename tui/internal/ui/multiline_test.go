package ui

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/demo"
)

// A multi-line message from another client (draft/multiline, #478) is one
// transcript row whose body breaks at the embedded newlines. The batch goes in as
// wire lines through the demo session, so the proof runs from the wire to the
// rendered row. The Rust twin is `client/src/ui/tests/multiline.rs`.

func injectMultilineBatch(d *demo.DemoServer, sender string, lines ...string) {
	wire := ":" + sender + "!u@h BATCH +ml draft/multiline #omarchy\r\n"
	for _, line := range lines {
		wire += "@batch=ml :" + sender + "!u@h PRIVMSG #omarchy :" + line + "\r\n"
	}
	wire += ":" + sender + "!u@h BATCH -ml\r\n"
	d.InjectOmarchy([]byte(wire))
}

func TestMultilineMessageIsOneRowThatBreaksAtTheEmbeddedNewlines(t *testing.T) {
	m, d := unreadDemoModel(t)
	m = resizeModel(t, m, 118, 40)
	before := len(m.ctrl.Messages())
	injectMultilineBatch(d, "mira", "alpha one", "beta two", "gamma three")
	m = updateMsg(t, m, NotifyMsg{})

	messages := m.ctrl.Messages()
	if len(messages) != before+1 {
		t.Fatalf("messages = %d, want one row for the batch", len(messages)-before)
	}
	last := messages[len(messages)-1]
	if last.Body != "alpha one\nbeta two\ngamma three" || last.Author != "mira" {
		t.Fatalf("row = %+v", last)
	}

	// The row is the last one. The first message of a new day also carries its
	// separator, so the three body lines are the last three of the row.
	area := m.transcriptArea()
	row := area.count() - 1
	end := area.line(row) + area.heights[row]
	if extra := area.heights[row] - 3; extra < 0 || extra > 1 {
		t.Fatalf("row height = %d, want 3 body lines (and at most a day separator)", area.heights[row])
	}
	rendered := plainLines(strings.Join(area.lines[end-3:end], "\n"))
	if !strings.Contains(rendered[0], "mira") || !strings.HasSuffix(strings.TrimRight(rendered[0], " "), "alpha one") {
		t.Fatalf("first line = %q", rendered[0])
	}
	// Every later line hangs under the body column, with no byline of its own.
	bodyColumn := strings.Index(rendered[0], "alpha one")
	for index, want := range []string{"beta two", "gamma three"} {
		line := rendered[index+1]
		if strings.Contains(line, "mira") {
			t.Fatalf("line %d repeats the byline: %q", index+2, line)
		}
		if got := strings.Index(line, want); got != bodyColumn {
			t.Fatalf("line %d starts at column %d, want the body column %d: %q", index+2, got, bodyColumn, line)
		}
	}
}

func TestMultilineMessageWithAMentionPadsEveryLine(t *testing.T) {
	m, d := unreadDemoModel(t)
	m = resizeModel(t, m, 118, 40)
	injectMultilineBatch(d, "mira", "first", "second", "ping fred")
	m = updateMsg(t, m, NotifyMsg{})

	area := m.transcriptArea()
	row := area.count() - 1
	end := area.line(row) + area.heights[row]
	lines := area.lines[end-3 : end]
	widths := map[int]bool{}
	for _, line := range lines {
		widths[lipgloss.Width(line)] = true
	}
	if len(widths) != 1 {
		t.Fatalf("the mention wash must pad every line to one width, got %v", widths)
	}
}

func TestMultilineMessageCopiesAndFindsWithItsBreaks(t *testing.T) {
	m, d := unreadDemoModel(t)
	injectMultilineBatch(d, "mira", "one", "two", "three")
	m = updateMsg(t, m, NotifyMsg{})

	rows := m.transcriptRowTexts()
	if got := rows[len(rows)-1]; got != "mira one\ntwo\nthree" {
		t.Fatalf("row text = %q, want the breaks kept", got)
	}
}
