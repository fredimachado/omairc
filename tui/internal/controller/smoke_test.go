package controller

import (
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

func TestSmokeEndToEnd(t *testing.T) {
	c := New()
	clock := session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC))
	c.SetClock(clock)

	transport := session.NewLoopbackTransport()
	config := session.DefaultSessionConfig("libera", "Libera", "irc.example", "omairc")
	config.TLSEnabled = true
	config.ReconnectEnabled = false
	config.Username = "omairc"
	config.Realname = "Omairc User"

	if _, err := c.AddSession(config, transport, clock); err != nil {
		t.Fatalf("AddSession: %v", err)
	}
	if !c.Start("libera") {
		t.Fatalf("Start returned false")
	}
	transport.CompleteConnect()
	transport.InjectBytes([]byte(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n"))

	if got := c.ConnectionStatusFor("libera"); got != "Connected" {
		t.Fatalf("ConnectionStatusFor = %q, want Connected", got)
	}
	if got := c.Session("libera"); got == nil || got.State() != session.StateRegistered {
		t.Fatalf("session state = %v", got)
	}

	transport.InjectBytes([]byte(":omairc!u@h JOIN :#omarchy\r\n" +
		":server 353 omairc = #omarchy :@omairc Alice\r\n" +
		":server 366 omairc #omarchy :End of NAMES\r\n"))

	if got := c.SelectedTarget(); got != "#omarchy" {
		t.Fatalf("SelectedTarget = %q, want #omarchy", got)
	}
	if got := c.ConnectionStatus(); got != "Connected" {
		t.Fatalf("ConnectionStatus = %q, want Connected after selection", got)
	}
	if got := c.CurrentNick(); got != "omairc" {
		t.Fatalf("CurrentNick = %q", got)
	}
	if !c.IsChannel() {
		t.Fatalf("IsChannel = false")
	}
	if got := c.PeopleCount(); got != 2 {
		t.Fatalf("PeopleCount = %d, want 2", got)
	}
	conversations := c.Conversations()
	if len(conversations) != 1 || conversations[0].Conversation != "#omarchy" {
		t.Fatalf("Conversations = %+v", conversations)
	}
	members := c.Members()
	if len(members) != 2 || members[0].Nick != "omairc" || members[0].Label != "@omairc" {
		t.Fatalf("Members = %+v", members)
	}
	if members[1].Nick != "Alice" || members[1].Label != "Alice" {
		t.Fatalf("member[1] = %+v", members[1])
	}

	rowsAfterJoin := len(c.Messages())
	transport.InjectBytes([]byte(":Alice!u@h PRIVMSG #omarchy :hello\r\n"))
	messages := c.Messages()
	if len(messages) != rowsAfterJoin+1 {
		t.Fatalf("messages after privmsg = %d, want %d", len(messages), rowsAfterJoin+1)
	}
	last := messages[len(messages)-1]
	if last.Body != "hello" || last.Author != "Alice" || last.Kind != "message" || last.Origin != "live" {
		t.Fatalf("last message = %+v", last)
	}

	// A plain say echoes locally and writes the frame.
	if !c.SendMessage("own line") {
		t.Fatalf("SendMessage failed")
	}
	messages = c.Messages()
	if messages[len(messages)-1].Body != "own line" || messages[len(messages)-1].Author != "omairc" {
		t.Fatalf("own echo = %+v", messages[len(messages)-1])
	}
	frames := transport.WrittenFrames()
	if len(frames) == 0 || string(frames[len(frames)-1]) != "PRIVMSG #omarchy :own line\r\n" {
		t.Fatalf("frames = %q", frames)
	}

	// A background chat bumps the conversation epoch and unread.
	transport.InjectBytes([]byte(":omairc!u@h JOIN :#lab\r\n:server 353 omairc = #lab :omairc\r\n:server 366 omairc #lab :End of NAMES\r\n"))
	epoch := c.ConversationEpoch()
	transport.InjectBytes([]byte(":zed!u@h PRIVMSG #lab :ping\r\n"))
	if c.ConversationEpoch() <= epoch {
		t.Fatalf("conversation epoch did not bump")
	}
	if got := c.UnreadCountFor("libera"); got != 1 {
		t.Fatalf("UnreadCountFor = %d, want 1", got)
	}
	if c.MentionFor("libera") {
		t.Fatalf("unexpected mention")
	}

	// A direct message addressed to the assigned nick routes.
	transport.InjectBytes([]byte(":Alice!u@h PRIVMSG omairc :hi there\r\n"))
	foundDirect := false
	for _, row := range c.Conversations() {
		if row.Conversation == "Alice" && row.Direct {
			foundDirect = true
		}
	}
	if !foundDirect {
		t.Fatalf("direct Alice row missing: %+v", c.Conversations())
	}

	// The Status console recorded server lines.
	if len(c.Console().Lines("libera")) == 0 {
		t.Fatalf("console is empty")
	}
}

func TestSmokeNickRetargetAndService(t *testing.T) {
	c := New()
	clock := session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC))
	c.SetClock(clock)
	transport := session.NewLoopbackTransport()
	config := session.DefaultSessionConfig("libera", "Libera", "irc.example", "omairc")
	config.TLSEnabled = true
	config.ReconnectEnabled = false
	if _, err := c.AddSession(config, transport, clock); err != nil {
		t.Fatalf("AddSession: %v", err)
	}
	c.Start("libera")
	transport.CompleteConnect()
	transport.InjectBytes([]byte(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n" +
		":server 353 omairc = #omarchy :omairc Alice\r\n" +
		":server 366 omairc #omarchy :End of NAMES\r\n" +
		":Alice!u@h PRIVMSG omairc :hi\r\n"))

	c.SelectConversation("libera", "Alice")
	if got := c.SelectedTarget(); got != "Alice" {
		t.Fatalf("SelectedTarget = %q", got)
	}
	transport.InjectBytes([]byte(":Alice!u@h NICK :Alicia\r\n"))
	if got := c.SelectedTarget(); got != "Alicia" {
		t.Fatalf("after NICK SelectedTarget = %q, want Alicia", got)
	}
	foundOld, foundNew := false, false
	for _, row := range c.Conversations() {
		if row.Conversation == "Alice" {
			foundOld = true
		}
		if row.Conversation == "Alicia" {
			foundNew = true
		}
	}
	if foundOld || !foundNew {
		t.Fatalf("rows old=%v new=%v: %+v", foundOld, foundNew, c.Conversations())
	}
	messages := c.Messages()
	if len(messages) == 0 || messages[len(messages)-1].Body != "Alice is now Alicia" {
		t.Fatalf("last message = %+v", messages)
	}

	// A service sender never invents a direct.
	transport.InjectBytes([]byte(":NickServ!NickServ@services PRIVMSG omairc :This nickname is registered.\r\n"))
	for _, row := range c.Conversations() {
		if row.Conversation == "NickServ" || row.Conversation == "nickserv" {
			t.Fatalf("NickServ conversation invented: %+v", c.Conversations())
		}
	}
}

func TestSmokeEchoMessageSuppression(t *testing.T) {
	c := New()
	clock := session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC))
	c.SetClock(clock)
	transport := session.NewLoopbackTransport()
	config := session.DefaultSessionConfig("libera", "Libera", "irc.example", "omairc")
	config.TLSEnabled = true
	config.ReconnectEnabled = false
	c.AddSession(config, transport, clock)
	c.Start("libera")
	transport.CompleteConnect()
	transport.InjectBytes([]byte(":server CAP omairc LS :echo-message\r\n" +
		":server CAP omairc ACK :echo-message\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":omairc!u@h JOIN :#omarchy\r\n" +
		":server 353 omairc = #omarchy :omairc\r\n" +
		":server 366 omairc #omarchy :End of NAMES\r\n"))
	if !c.Session("libera").Capabilities().Contains(irc.CapabilityEchoMessage) {
		t.Fatalf("echo-message not acked")
	}
	rows := len(c.Messages())
	if !c.SendMessage("hello") {
		t.Fatalf("SendMessage failed")
	}
	if len(c.Messages()) != rows {
		t.Fatalf("echo-message should suppress local echo: %d -> %d", rows, len(c.Messages()))
	}
	transport.InjectBytes([]byte(":omairc!u@h PRIVMSG #omarchy :hello\r\n"))
	if len(c.Messages()) != rows+1 {
		t.Fatalf("server echo not admitted: %d", len(c.Messages()))
	}
}
