package controller

import (
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

func TestControllerOlderHistoryMutedAndUnreadNeutral(t *testing.T) {
	c := New()
	transport := session.NewLoopbackTransport()
	clock := session.NewFakeClock(time.Unix(0, 0))
	config := session.SessionConfig{
		NetworkID: "libera",
		Host:      "irc.example",
		Port:      6697,
		TLSEnabled: true,
		Nick:      "omairc",
		Username:  "omairc",
		Realname:  "Omairc User",
	}
	s, err := c.AddSession(config, transport, clock)
	if err != nil {
		t.Fatal(err)
	}
	s.Start()
	transport.CompleteConnect()
	transport.InjectBytes([]byte(
		":server CAP omairc LS :batch chathistory\r\n" +
			":server CAP omairc ACK :batch chathistory\r\n" +
			":server 001 omairc :Welcome\r\n" +
			":server 376 omairc :End of MOTD\r\n" +
			":omairc!u@h JOIN :#omarchy\r\n" +
			":irc.host BATCH +hx chathistory #omarchy\r\n" +
			"@batch=hx;time=2011-10-19T16:40:51.620Z;msgid=live :alice!u@h PRIVMSG #omarchy :live\r\n" +
			":irc.host BATCH -hx\r\n"))
	c.SelectConversation("libera", "#omarchy")
	c.Publish(irc.ViewNotify{Messages: true, Conversations: true})
	beforeUnread := c.UnreadCountFor("libera")
	if !c.RequestOlderTranscriptHistory() {
		t.Fatal("RequestOlderTranscriptHistory must succeed")
	}
	transport.InjectBytes([]byte(
		":irc.host BATCH +older chathistory #omarchy\r\n" +
			"@batch=older;time=2011-10-19T16:40:50.000Z;msgid=page :bob!u@h PRIVMSG #omarchy :backlog\r\n" +
			":irc.host BATCH -older\r\n"))
	c.Publish(irc.ViewNotify{Messages: true})
	found := false
	for _, message := range c.Messages() {
		if message.Body == "backlog" {
			found = true
			if message.Origin != "replay" {
				t.Fatalf("backlog origin = %q, want replay", message.Origin)
			}
		}
	}
	if !found {
		t.Fatalf("messages = %+v, want backlog row", c.Messages())
	}
	if c.UnreadCountFor("libera") != beforeUnread {
		t.Fatalf("unread = %d, want unchanged %d", c.UnreadCountFor("libera"), beforeUnread)
	}
}

func TestControllerDuplicateMsgidNotInserted(t *testing.T) {
	c := New()
	transport := session.NewLoopbackTransport()
	clock := session.NewFakeClock(time.Unix(0, 0))
	config := session.SessionConfig{
		NetworkID: "libera",
		Host:      "irc.example",
		Port:      6697,
		TLSEnabled: true,
		Nick:      "omairc",
		Username:  "omairc",
		Realname:  "Omairc User",
	}
	s, err := c.AddSession(config, transport, clock)
	if err != nil {
		t.Fatal(err)
	}
	s.Start()
	transport.CompleteConnect()
	transport.InjectBytes([]byte(
		":server CAP omairc LS :batch chathistory\r\n" +
			":server CAP omairc ACK :batch chathistory\r\n" +
			":server 001 omairc :Welcome\r\n" +
			":server 376 omairc :End of MOTD\r\n" +
			":omairc!u@h JOIN :#omarchy\r\n" +
			":irc.host BATCH +hx chathistory #omarchy\r\n" +
			"@batch=hx;msgid=same :alice!u@h PRIVMSG #omarchy :once\r\n" +
			":irc.host BATCH -hx\r\n"))
	c.SelectConversation("libera", "#omarchy")
	c.Publish(irc.ViewNotify{Messages: true})
	transport.InjectBytes([]byte(
		":irc.host BATCH +dup chathistory #omarchy\r\n" +
			"@batch=dup;msgid=same :Alice!u@h PRIVMSG #omarchy :once\r\n" +
			":irc.host BATCH -dup\r\n"))
	c.Publish(irc.ViewNotify{Messages: true})
	count := 0
	for _, message := range c.Messages() {
		if message.Body == "once" {
			count++
		}
	}
	if count != 1 {
		t.Fatalf("once count = %d, want 1", count)
	}
}

func TestControllerRequestOlderTranscriptHistoryUsesOldestRow(t *testing.T) {
	c := New()
	transport := session.NewLoopbackTransport()
	clock := session.NewFakeClock(time.Unix(0, 0))
	config := session.SessionConfig{
		NetworkID: "libera",
		Host:      "irc.example",
		Port:      6697,
		TLSEnabled: true,
		Nick:      "omairc",
		Username:  "omairc",
		Realname:  "Omairc User",
	}
	s, err := c.AddSession(config, transport, clock)
	if err != nil {
		t.Fatal(err)
	}
	s.Start()
	transport.CompleteConnect()
	transport.InjectBytes([]byte(
		":server CAP omairc LS :batch chathistory\r\n" +
			":server CAP omairc ACK :batch chathistory\r\n" +
			":server 001 omairc :Welcome\r\n" +
			":server 376 omairc :End of MOTD\r\n" +
			":omairc!u@h JOIN :#omarchy\r\n" +
			":irc.host BATCH +hx chathistory #omarchy\r\n" +
			"@batch=hx;msgid=anchor :alice!u@h PRIVMSG #omarchy :first\r\n" +
			":irc.host BATCH -hx\r\n"))
	c.SelectConversation("libera", "#omarchy")
	c.Publish(irc.ViewNotify{Messages: true})
	if !c.RequestOlderTranscriptHistory() {
		t.Fatal("RequestOlderTranscriptHistory must succeed")
	}
	frames := transport.WrittenFrames()
	last := string(frames[len(frames)-1])
	want := "CHATHISTORY BEFORE #omarchy msgid=anchor 100\r\n"
	if last != want {
		t.Fatalf("last frame = %q, want %q", last, want)
	}
	if !c.SendMessage("/part") {
		t.Fatalf("part: %s", c.LastError())
	}
	afterPart := len(transport.WrittenFrames())
	if c.RequestOlderTranscriptHistory() {
		t.Fatal("a channel you have left must not page history")
	}
	if len(transport.WrittenFrames()) != afterPart {
		t.Fatal("left channel wrote a history frame")
	}
}

func TestRequestOlderTranscriptHistoryArmsTailCapBeforeSend(t *testing.T) {
	c := New()
	transport := session.NewLoopbackTransport()
	clock := session.NewFakeClock(time.Unix(0, 0))
	config := session.SessionConfig{
		NetworkID:  "libera",
		Host:       "irc.example",
		Port:       6697,
		TLSEnabled: true,
		Nick:       "omairc",
		Username:   "omairc",
		Realname:   "Omairc User",
	}
	s, err := c.AddSession(config, transport, clock)
	if err != nil {
		t.Fatal(err)
	}
	s.Start()
	transport.CompleteConnect()
	transport.InjectBytes([]byte(
		":server CAP omairc LS :batch chathistory\r\n" +
			":server CAP omairc ACK :batch chathistory\r\n" +
			":server 001 omairc :Welcome\r\n" +
			":server 376 omairc :End of MOTD\r\n" +
			":omairc!u@h JOIN :#omarchy\r\n" +
			":irc.host BATCH +hx chathistory #omarchy\r\n" +
			"@batch=hx;msgid=anchor :alice!u@h PRIVMSG #omarchy :first\r\n" +
			":irc.host BATCH -hx\r\n"))
	c.SelectConversation("libera", "#omarchy")
	c.Publish(irc.ViewNotify{Messages: true})
	key := c.Reducer().ConversationKey("libera", "#omarchy")
	if !c.RequestOlderTranscriptHistory() {
		t.Fatal("RequestOlderTranscriptHistory must succeed")
	}
	conversation := c.Reducer().Find(key)
	if conversation == nil || !conversation.HistoryPageCapTail {
		t.Fatal("tail cap must be armed when the BEFORE request is sent")
	}
}

func TestRequestOlderTranscriptHistoryKeepsTailCapWhenBeforeInflight(t *testing.T) {
	c := New()
	transport := session.NewLoopbackTransport()
	clock := session.NewFakeClock(time.Unix(0, 0))
	config := session.SessionConfig{
		NetworkID:  "libera",
		Host:       "irc.example",
		Port:       6697,
		TLSEnabled: true,
		Nick:       "omairc",
		Username:   "omairc",
		Realname:   "Omairc User",
	}
	s, err := c.AddSession(config, transport, clock)
	if err != nil {
		t.Fatal(err)
	}
	s.Start()
	transport.CompleteConnect()
	transport.InjectBytes([]byte(
		":server CAP omairc LS :batch chathistory\r\n" +
			":server CAP omairc ACK :batch chathistory\r\n" +
			":server 001 omairc :Welcome\r\n" +
			":server 376 omairc :End of MOTD\r\n" +
			":omairc!u@h JOIN :#omarchy\r\n" +
			":irc.host BATCH +hx chathistory #omarchy\r\n" +
			"@batch=hx;msgid=anchor :alice!u@h PRIVMSG #omarchy :first\r\n" +
			":irc.host BATCH -hx\r\n"))
	c.SelectConversation("libera", "#omarchy")
	c.Publish(irc.ViewNotify{Messages: true})
	key := c.Reducer().ConversationKey("libera", "#omarchy")
	if !c.RequestOlderTranscriptHistory() {
		t.Fatal("first BEFORE must succeed")
	}
	if c.RequestOlderTranscriptHistory() {
		t.Fatal("second BEFORE while in flight must fail")
	}
	conversation := c.Reducer().Find(key)
	if conversation == nil || !conversation.HistoryPageCapTail {
		t.Fatal("tail cap must stay armed while a BEFORE page is in flight")
	}
}

func TestRequestOlderTranscriptHistoryClearsTailCapWhenRefused(t *testing.T) {
	c := New()
	transport := session.NewLoopbackTransport()
	clock := session.NewFakeClock(time.Unix(0, 0))
	config := session.SessionConfig{
		NetworkID:  "libera",
		Host:       "irc.example",
		Port:       6697,
		TLSEnabled: true,
		Nick:       "omairc",
		Username:   "omairc",
		Realname:   "Omairc User",
	}
	s, err := c.AddSession(config, transport, clock)
	if err != nil {
		t.Fatal(err)
	}
	s.Start()
	transport.CompleteConnect()
	transport.InjectBytes([]byte(
		":server CAP omairc LS :batch chathistory\r\n" +
			":server CAP omairc ACK :batch chathistory\r\n" +
			":server 001 omairc :Welcome\r\n" +
			":server 376 omairc :End of MOTD\r\n" +
			":omairc!u@h JOIN :#omarchy\r\n" +
			":irc.host BATCH +hx chathistory #omarchy\r\n" +
			"@batch=hx;msgid=anchor :alice!u@h PRIVMSG #omarchy :first\r\n" +
			":irc.host BATCH -hx\r\n"))
	c.SelectConversation("libera", "#omarchy")
	c.Publish(irc.ViewNotify{Messages: true})
	key := c.Reducer().ConversationKey("libera", "#omarchy")
	if !c.RequestOlderTranscriptHistory() {
		t.Fatal("first BEFORE must succeed")
	}
	transport.InjectBytes([]byte(":irc.host BATCH +empty chathistory #omarchy\r\n" +
		":irc.host BATCH -empty\r\n"))
	c.Publish(irc.ViewNotify{Messages: true})
	if c.RequestOlderTranscriptHistory() {
		t.Fatal("BEFORE after exhaustion must fail")
	}
	conversation := c.Reducer().Find(key)
	if conversation == nil || conversation.HistoryPageCapTail {
		t.Fatal("tail cap must clear when BEFORE is refused and nothing is in flight")
	}
}
