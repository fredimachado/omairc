package controller

import (
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

func directListed(c *Controller, target string) bool {
	features := c.reducer.ServerFeatures("libera")
	mapping := features.CaseMapping()
	for _, listed := range c.openDirects.Listed("libera", mapping) {
		if mapping.Equals(listed, target) {
			return true
		}
	}
	return false
}

func closedHas(c *Controller, target string) bool {
	features := c.reducer.ServerFeatures("libera")
	mapping := features.CaseMapping()
	return c.closed.Contains("libera", target, mapping)
}

func applyNotice(c *Controller, nick, body string) {
	c.Apply(irc.NoticeEvent{
		Conversation: c.reducer.ConversationKey("libera", nick),
		Author:       nick,
		Body:         body,
		Timestamp:    time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC),
		Target:       nick,
	})
}

func frameCount(transport *session.LoopbackTransport, needle string) int {
	count := 0
	for _, frame := range transport.WrittenFrames() {
		if strings.Contains(string(frame), needle) {
			count++
		}
	}
	return count
}

func TestReopenedDirectIsRememberedAcrossRestart(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	c, clock := newController(t)
	c.SetEphemeral(false)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc")
	inject(t, transport, ":lena!u@h PRIVMSG omairc :hi\r\n"+
		":rio!u@h PRIVMSG omairc :\x01ACTION waves\x01\r\n"+
		":kai!u@h PRIVMSG omairc :once\r\n")
	applyNotice(c, "nia", "psst")

	for _, nick := range []string{"lena", "nia", "rio", "kai"} {
		if !hasConversation(c, "libera", nick) {
			t.Fatalf("%s was not opened", nick)
		}
		if directListed(c, nick) {
			t.Fatalf("first line persisted %s", nick)
		}
	}
	for _, nick := range []string{"lena", "nia", "rio"} {
		c.SelectConversation("libera", nick)
		if !c.CloseDirectMessage() {
			t.Fatalf("CloseDirectMessage(%s) = false", nick)
		}
		if hasConversation(c, "libera", nick) {
			t.Fatalf("%s stayed after close", nick)
		}
		if !closedHas(c, nick) {
			t.Fatalf("close did not remember %s", nick)
		}
	}

	inject(t, transport, ":lena!u@h PRIVMSG omairc :again\r\n"+
		":rio!u@h PRIVMSG omairc :\x01ACTION waves again\x01\r\n")
	applyNotice(c, "nia", "again")
	for _, nick := range []string{"lena", "nia", "rio"} {
		if !hasConversation(c, "libera", nick) {
			t.Fatalf("live line did not reopen %s", nick)
		}
		if closedHas(c, nick) {
			t.Fatalf("reopen left %s closed", nick)
		}
		if !directListed(c, nick) {
			t.Fatalf("reopen did not remember %s", nick)
		}
	}
	if directListed(c, "kai") {
		t.Fatal("a first line that was never closed was remembered")
	}

	again, againClock := newController(t)
	again.SetEphemeral(false)
	againTransport := addAndStart(t, again, againClock, baseConfig("libera", "omairc"))
	registerNetwork(t, againTransport, "omairc")
	for _, nick := range []string{"lena", "nia", "rio", "kai"} {
		if hasConversation(again, "libera", nick) {
			t.Fatalf("%s existed before MOTD", nick)
		}
	}
	inject(t, againTransport, ":server 376 omairc :End of MOTD\r\n")
	for _, nick := range []string{"lena", "nia", "rio"} {
		if !hasConversation(again, "libera", nick) {
			t.Fatalf("restart dropped %s", nick)
		}
	}
	if hasConversation(again, "libera", "kai") {
		t.Fatal("restart restored a query that was never closed or answered")
	}
}

func TestUnsolicitedSelfJoinKeepsClosedChannel(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	c, clock := newController(t)
	c.SetEphemeral(false)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc")
	inject(t, transport, ":omairc!u@h JOIN :#omarchy\r\n"+
		":omairc!u@h JOIN :#lab\r\n"+
		":omairc!u@h JOIN :#desk\r\n")
	for _, channel := range []string{"#lab", "#desk"} {
		c.SelectConversation("libera", channel)
		if !c.SendMessage("/part") {
			t.Fatalf("/part %s = false", channel)
		}
		if !c.SendMessage("/close") {
			t.Fatalf("/close %s = false", channel)
		}
		if hasConversation(c, "libera", channel) {
			t.Fatalf("%s stayed after close", channel)
		}
		if !closedHas(c, channel) {
			t.Fatalf("close did not remember %s", channel)
		}
	}
	if frameCount(transport, "PART #lab") != 1 {
		t.Fatalf("PART #lab count = %d, want 1", frameCount(transport, "PART #lab"))
	}

	inject(t, transport, ":omairc!u@h JOIN :#lab\r\n"+
		":server 353 omairc = #lab :omairc\r\n"+
		":server 366 omairc #lab :End of NAMES\r\n"+
		":lena!u@h PRIVMSG #lab :still\r\n")
	if hasConversation(c, "libera", "#lab") {
		t.Fatal("an unsolicited join brought #lab back")
	}
	if !closedHas(c, "#lab") {
		t.Fatal("the unsolicited join forgot the close")
	}

	c.SelectConversation("libera", "#omarchy")
	if !c.SendMessage("/join #desk,#gamma") {
		t.Fatal("SendMessage(/join #desk,#gamma) = false")
	}
	if !hasConversation(c, "libera", "#gamma") {
		t.Fatal("the last join target was not opened")
	}
	if closedHas(c, "#desk") {
		t.Fatal("an explicit join left #desk closed")
	}
	if !closedHas(c, "#lab") {
		t.Fatal("#lab close was cleared with #desk")
	}
	inject(t, transport, ":omairc!u@h JOIN :#desk\r\n"+
		":server 353 omairc = #desk :omairc\r\n"+
		":server 366 omairc #desk :End of NAMES\r\n")
	if !hasConversation(c, "libera", "#desk") {
		t.Fatal("an explicit join did not open #desk")
	}
	if closedHas(c, "#desk") {
		t.Fatal("an explicit join left #desk closed after the echo")
	}
	if !closedHas(c, "#lab") {
		t.Fatal("#lab close was cleared with #desk")
	}

	again, againClock := newController(t)
	again.SetEphemeral(false)
	againTransport := addAndStart(t, again, againClock, baseConfig("libera", "omairc"))
	registerNetwork(t, againTransport, "omairc")
	inject(t, againTransport, ":omairc!u@h JOIN :#lab\r\n"+
		":server 353 omairc = #lab :omairc\r\n"+
		":server 366 omairc #lab :End of NAMES\r\n"+
		":lena!u@h PRIVMSG #lab :still\r\n")
	if hasConversation(again, "libera", "#lab") {
		t.Fatal("restart join brought #lab back")
	}
	if !closedHas(again, "#lab") {
		t.Fatal("restart join forgot the close")
	}
	if writtenFramesContain(againTransport, "PART #lab") {
		t.Fatal("restart join wrote PART")
	}
	again.OpenStatus("libera")
	again.ConsoleSubmit("/join #lab")
	if !hasConversation(again, "libera", "#lab") {
		t.Fatal("an explicit join did not open #lab")
	}
	if closedHas(again, "#lab") {
		t.Fatal("an explicit join left #lab closed")
	}
}
