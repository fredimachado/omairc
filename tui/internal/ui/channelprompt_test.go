package ui

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/demo"
)

// TestChannelNameAsksBeforeOpening covers a channel name in a message and in
// the topic. An open buffer switches. A missing buffer asks, and Enter is
// what joins it.
func TestChannelNameAsksBeforeOpening(t *testing.T) {
	m, d := phase9DemoModel(t)
	if got := m.ctrl.ChannelNameAt("see &local now", strings.Index("see &local now", "&")); got != "" {
		t.Fatalf("demo CHANTYPES=# matched &local as %q", got)
	}
	if got := m.ctrl.ChannelNameAt("see #desktop now", strings.Index("see #desktop now", "#")); got != "#desktop" {
		t.Fatalf("channelNameAt = %q, want #desktop", got)
	}

	d.InjectOmarchy([]byte(":anna!u@h TOPIC #omarchy :talk in #help\r\n"))
	d.InjectOmarchy([]byte(":dax!u@h PRIVMSG #omarchy :see #Desktop and #brand-new\r\n"))
	if got := m.ctrl.Topic(); !strings.Contains(got, "#help") {
		t.Fatalf("topic = %q, want it to mention #help", got)
	}

	m = press(t, m, ctrlShiftKey('o'))
	if !m.linkVisible() {
		t.Fatal("Ctrl+Shift+O must open the link sheet")
	}
	help := channelMatch(t, m, "#help")
	if help.row != -1 {
		t.Fatalf("topic channel row = %d, want -1", help.row)
	}

	framesBefore := len(d.OmarchyTransport().WrittenFrames())
	m.link.selected = help.index
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.channelPrompt.open {
		t.Fatal("an open topic buffer must switch without asking")
	}
	if got := m.ctrl.SelectedTarget(); got != "#help" {
		t.Fatalf("selected = %q, want #help", got)
	}
	if joinedSince(d, framesBefore, "JOIN #help") {
		t.Fatal("switching to #help sent JOIN")
	}

	m.ctrl.SelectConversation(m.ctrl.FocusedNetworkID(), "#omarchy")
	m = press(t, m, ctrlShiftKey('o'))
	desktop := channelMatch(t, m, "#Desktop")
	fresh := channelMatch(t, m, "#brand-new")
	framesBefore = len(d.OmarchyTransport().WrittenFrames())
	m.link.selected = fresh.index
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.linkVisible() {
		t.Fatal("Enter must close the link sheet")
	}
	if !m.channelPrompt.open || m.channelPrompt.name != "#brand-new" {
		t.Fatalf("prompt = %+v, want an ask for #brand-new", m.channelPrompt)
	}
	if got := m.ctrl.SelectedTarget(); got != "#omarchy" {
		t.Fatalf("selected = %q, want to stay on #omarchy before confirm", got)
	}
	if joinedSince(d, framesBefore, "JOIN #brand-new") {
		t.Fatal("opening the prompt joined #brand-new")
	}
	if !strings.Contains(m.View().Content, "Open #brand-new?") {
		t.Fatalf("prompt view missing the question:\n%s", m.View().Content)
	}

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.channelPrompt.open {
		t.Fatal("Escape must cancel the prompt")
	}
	if joinedSince(d, framesBefore, "JOIN #brand-new") {
		t.Fatal("Escape joined #brand-new")
	}
	if got := m.ctrl.SelectedTarget(); got != "#omarchy" {
		t.Fatalf("selected after cancel = %q, want #omarchy", got)
	}

	m = press(t, m, ctrlShiftKey('o'))
	desktop = channelMatch(t, m, "#Desktop")
	m.link.selected = desktop.index
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.channelPrompt.open {
		t.Fatal("an open buffer must switch without asking")
	}
	if got := m.ctrl.SelectedTarget(); got != "#desktop" {
		t.Fatalf("selected = %q, want #desktop", got)
	}
	if joinedSince(d, framesBefore, "JOIN #desktop") || joinedSince(d, framesBefore, "JOIN #Desktop") {
		t.Fatal("switching to an open channel sent JOIN")
	}

	m.ctrl.SelectConversation(m.ctrl.FocusedNetworkID(), "#omarchy")
	m = press(t, m, ctrlShiftKey('o'))
	fresh = channelMatch(t, m, "#brand-new")
	m.link.selected = fresh.index
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if !m.channelPrompt.open {
		t.Fatal("a missing channel must ask again")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.channelPrompt.open {
		t.Fatal("Enter must confirm the prompt")
	}
	if !joinedSince(d, framesBefore, "JOIN #brand-new") || !wroteJoinCommand(d, framesBefore, "#brand-new") {
		t.Fatalf("ConsoleSubmit(%q) did not write %q", "/join #brand-new", "JOIN #brand-new\r\n")
	}
	if got := m.ctrl.SelectedTarget(); got != "#brand-new" {
		t.Fatalf("selected after confirm = %q, want #brand-new", got)
	}
}

type channelHit struct {
	index int
	row   int
}

func channelMatch(t *testing.T, m *Model, name string) channelHit {
	t.Helper()
	matches := m.linkMatches()
	for index, match := range matches {
		if match.kind == linkKindChannel && match.value == name {
			return channelHit{index: index, row: match.row}
		}
	}
	t.Fatalf("link sheet missing channel %q: %+v", name, matches)
	return channelHit{}
}

func wroteJoinCommand(d *demo.DemoServer, before int, channel string) bool {
	want := "JOIN " + channel + "\r\n"
	frames := d.OmarchyTransport().WrittenFrames()
	if before > len(frames) {
		before = 0
	}
	for _, frame := range frames[before:] {
		if string(frame) == want {
			return true
		}
	}
	return false
}

func joinedSince(d *demo.DemoServer, before int, text string) bool {
	frames := d.OmarchyTransport().WrittenFrames()
	if before > len(frames) {
		before = 0
	}
	for _, frame := range frames[before:] {
		if strings.Contains(string(frame), text) {
			return true
		}
	}
	return false
}
