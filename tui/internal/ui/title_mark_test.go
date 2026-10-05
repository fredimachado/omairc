package ui

import (
	"testing"

	tea "charm.land/bubbletea/v2"
)

func TestAttentionTitleCollapsesFormatting(t *testing.T) {
	if got := attentionTitle("alice", "hey fred", "#ricing"); got != "alice: hey fred · #ricing - Omairc" {
		t.Fatalf("channel = %q", got)
	}
	if got := attentionTitle("dax", "hello there", "dax"); got != "dax: hello there - Omairc" {
		t.Fatalf("direct = %q", got)
	}
	if got := attentionTitle("alice", "hey fred", "#omarchy · irc.example · fred"); got != "alice: hey fred · #omarchy · irc.example · fred - Omairc" {
		t.Fatalf("duplicate = %q", got)
	}
	if got := collapseTitleSpace("hey \x07fred\nthere"); got != "hey fred there" {
		t.Fatalf("collapse = %q", got)
	}
}

func TestUnfocusedMentionMarksWindowTitle(t *testing.T) {
	m, d := phase9DemoModel(t)
	var arrived []MentionArrivalMsg
	m.ctrl.OnMentionArrived = func(author, body, networkID, target, msgid string) {
		arrived = append(arrived, MentionArrivalMsg{
			Author: author, Body: body, NetworkID: networkID, Target: target, MsgID: msgid,
		})
	}
	deliver := func() {
		t.Helper()
		for _, msg := range arrived {
			m = updateMsg(t, m, msg)
		}
		arrived = nil
	}

	base := m.View().WindowTitle
	d.InjectOmarchy([]byte(":alice!u@h PRIVMSG #ricing :hey \x02fred\r\n"))
	deliver()
	if got := m.View().WindowTitle; got != base {
		t.Fatalf("focused title = %q, want %q", got, base)
	}

	m = updateMsg(t, m, tea.BlurMsg{})
	// The open conversation still marks the title. #omarchy is on two networks.
	d.InjectOmarchy([]byte(":alice!u@h PRIVMSG #omarchy :hey \x02fred\r\n"))
	deliver()
	const openMarked = "alice: hey fred · #omarchy · irc.example · fred - Omairc"
	if got := m.View().WindowTitle; got != openMarked {
		t.Fatalf("open conversation while unfocused = %q, want %q", got, openMarked)
	}

	d.InjectOmarchy([]byte(":alice!u@h PRIVMSG #ricing :hey \x02fred\x07\r\n"))
	deliver()
	const ricing = "alice: hey fred · #ricing - Omairc"
	if got := m.View().WindowTitle; got != ricing {
		t.Fatalf("unfocused title = %q, want %q", got, ricing)
	}

	m.switchSelection(func() { m.ctrl.OpenStatus("omarchy") })
	if got := m.View().WindowTitle; got != ricing {
		t.Fatalf("Status must keep the mark, title = %q", got)
	}
	m.switchSelection(func() { m.ctrl.SelectConversation("omarchy", "#desktop") })
	if got := m.View().WindowTitle; got != ricing {
		t.Fatalf("title after leaving the mention = %q, want %q", got, ricing)
	}
	m.switchSelection(func() { m.ctrl.SelectConversation("omarchy", "#ricing") })
	if got := m.View().WindowTitle; got != "#ricing - Omairc" {
		t.Fatalf("opened title = %q, want the conversation", got)
	}

	d.InjectOmarchy([]byte(":dax!u@h PRIVMSG fred :hello\r\n"))
	deliver()
	if got := m.View().WindowTitle; got != "dax: hello - Omairc" {
		t.Fatalf("direct title = %q, want dax: hello - Omairc", got)
	}

	d.InjectOmarchy([]byte(":alice!u@h PRIVMSG #omarchy :hey \x1ffred\r\n"))
	deliver()
	const duplicated = "alice: hey fred · #omarchy · irc.example · fred - Omairc"
	if got := m.View().WindowTitle; got != duplicated {
		t.Fatalf("duplicate title = %q, want %q", got, duplicated)
	}

	m = updateMsg(t, m, tea.FocusMsg{})
	if got := m.View().WindowTitle; got != "#ricing - Omairc" {
		t.Fatalf("focus clears the mark, title = %q", got)
	}

	m = updateMsg(t, m, tea.BlurMsg{})
	quiet := m.View().WindowTitle
	m = updateMsg(t, m, MonitorArrivalMsg{NetworkID: "omarchy", Author: "mira", Body: "is online"})
	if got := m.View().WindowTitle; got != quiet {
		t.Fatalf("monitor title = %q, want %q", got, quiet)
	}

	d.InjectOmarchy([]byte(":alice!u@h INVITE fred :#lab\r\n"))
	if len(arrived) != 0 {
		t.Fatalf("invite produced %d mention arrivals", len(arrived))
	}
	if got := m.View().WindowTitle; got != quiet {
		t.Fatalf("invite title = %q, want %q", got, quiet)
	}

	m.ctrl.SetMuted("omarchy", "#desktop", true)
	d.InjectOmarchy([]byte(":alice!u@h PRIVMSG #desktop :hey \x02fred\r\n"))
	if len(arrived) != 0 {
		t.Fatal("muted conversation must not arrive as a mention")
	}
	if got := m.View().WindowTitle; got != quiet {
		t.Fatalf("muted title = %q, want %q", got, quiet)
	}
}
