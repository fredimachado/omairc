package controller

// This file ports the Phase 9 session-inbox subset of
// tests/session/tst_controller.cpp: arrivals append a stamped row, selection
// and self-join consume matching rows, dismiss/activate walk the waiting list,
// an invite activates into a JOIN, a monitor-online edge appends, forgetting a
// network purges it, and PlainIrcText passes through to the formatter.

import (
	"testing"

	"github.com/fredimachado/omairc/tui/internal/session"
)

// inboxMentionSetup registers libera, joins #chan and selects it, leaving the
// mention target channel unselected so a chat line plants an inbox arrival.
func inboxMentionSetup(t *testing.T) (*Controller, *session.LoopbackTransport) {
	t.Helper()
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "message-tags")
	inject(t, transport, ":omairc!u@h JOIN :#chan\r\n")
	c.SelectConversation("libera", "#chan")
	return c, transport
}

func TestInboxMentionArrivalAppendsSnapshot(t *testing.T) {
	c, transport := inboxMentionSetup(t)

	changed := 0
	c.OnInboxChanged = func() { changed++ }

	inject(t, transport, ":omairc!u@h JOIN :#lab\r\n"+
		"@msgid=m1 :alice!u@h PRIVMSG #lab :omairc: ping\r\n")

	if got := c.InboxCount(); got != 1 {
		t.Fatalf("InboxCount = %d, want 1", got)
	}
	row := c.InboxItems()[0]
	if row.Kind != "mention" {
		t.Fatalf("Kind = %q, want mention", row.Kind)
	}
	if row.Target != "#lab" {
		t.Fatalf("Target = %q, want #lab", row.Target)
	}
	if row.Actor != "alice" {
		t.Fatalf("Actor = %q, want alice", row.Actor)
	}
	if row.MsgID != "m1" {
		t.Fatalf("MsgID = %q, want m1", row.MsgID)
	}
	if row.Label != "alice mentioned you in #lab" {
		t.Fatalf("Label = %q, want %q", row.Label, "alice mentioned you in #lab")
	}
	if changed == 0 {
		t.Fatalf("OnInboxChanged did not fire on arrival")
	}
}

func TestInboxConsumedOnSelect(t *testing.T) {
	c, transport := inboxMentionSetup(t)

	inject(t, transport, ":omairc!u@h JOIN :#lab\r\n"+
		"@msgid=m1 :alice!u@h PRIVMSG #lab :omairc: ping\r\n")
	if got := c.InboxCount(); got != 1 {
		t.Fatalf("InboxCount before select = %d, want 1", got)
	}

	c.SelectConversation("libera", "#lab")
	if got := c.InboxCount(); got != 0 {
		t.Fatalf("InboxCount after select = %d, want 0", got)
	}
}

func TestInboxDismissAndActivate(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "message-tags")
	// The first self-join is auto-selected; #two and #three stay unselected.
	inject(t, transport, ":omairc!u@h JOIN :#one\r\n")
	c.SelectConversation("libera", "#one")

	inject(t, transport, ":omairc!u@h JOIN :#two\r\n"+
		":omairc!u@h JOIN :#three\r\n"+
		"@msgid=m2 :alice!u@h PRIVMSG #two :omairc: ping two\r\n"+
		"@msgid=m3 :bob!u@h PRIVMSG #three :omairc: ping three\r\n")
	if got := c.InboxCount(); got != 2 {
		t.Fatalf("InboxCount = %d, want 2", got)
	}
	// Newest first: #three then #two.
	if newest := c.InboxItems()[0]; newest.Target != "#three" {
		t.Fatalf("newest Target = %q, want #three", newest.Target)
	}

	// Out-of-range operations are safe no-ops.
	c.DismissInboxItem(99)
	c.ActivateInboxItem(-1)
	if got := c.InboxCount(); got != 2 {
		t.Fatalf("out-of-range ops changed InboxCount = %d, want 2", got)
	}

	// Dismiss drops exactly the newest row.
	c.DismissInboxItem(0)
	items := c.InboxItems()
	if len(items) != 1 {
		t.Fatalf("InboxItems after dismiss = %+v, want 1 row", items)
	}
	if items[0].Target != "#two" {
		t.Fatalf("remaining Target = %q, want #two", items[0].Target)
	}

	// Activate reveals #two and consumes it.
	c.ActivateInboxItem(0)
	if c.SelectedTarget() != "#two" {
		t.Fatalf("SelectedTarget = %q, want #two", c.SelectedTarget())
	}
	if got := c.InboxCount(); got != 0 {
		t.Fatalf("InboxCount after activate = %d, want 0", got)
	}
}

func TestInboxInviteAppendActivateJoins(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc")
	inject(t, transport, ":omairc!u@h JOIN :#chan\r\n")
	c.SelectConversation("libera", "#chan")

	inject(t, transport, ":alice!u@h INVITE omairc :#invited\r\n")
	if got := c.InboxCount(); got != 1 {
		t.Fatalf("InboxCount = %d, want 1", got)
	}
	row := c.InboxItems()[0]
	if row.Kind != "invite" {
		t.Fatalf("Kind = %q, want invite", row.Kind)
	}
	if row.Actor != "alice" {
		t.Fatalf("Actor = %q, want alice", row.Actor)
	}
	if row.Target != "#invited" {
		t.Fatalf("Target = %q, want #invited", row.Target)
	}

	c.ActivateInboxItem(0)
	if !writtenFramesContain(transport, "JOIN #invited") {
		t.Fatalf("no JOIN #invited frame: %v", transport.WrittenFrames())
	}
	if !hasConversation(c, "libera", "#invited") {
		t.Fatalf("#invited not revealed: %v", conversationTargets(c))
	}
	if c.SelectedTarget() != "#invited" {
		t.Fatalf("SelectedTarget = %q, want #invited", c.SelectedTarget())
	}
	if got := c.InboxCount(); got != 0 {
		t.Fatalf("InboxCount after activate = %d, want 0", got)
	}
}

func TestInboxInviteConsumedOnSelfJoin(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc")
	inject(t, transport, ":omairc!u@h JOIN :#chan\r\n")
	c.SelectConversation("libera", "#chan")

	inject(t, transport, ":alice!u@h INVITE omairc :#invited\r\n")
	if got := c.InboxCount(); got != 1 {
		t.Fatalf("InboxCount = %d, want 1", got)
	}

	inject(t, transport, ":omairc!u@h JOIN :#invited\r\n")
	if got := c.InboxCount(); got != 0 {
		t.Fatalf("InboxCount after self-join = %d, want 0", got)
	}
}

func TestInboxMonitorOnlineAppendsAndConsumes(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "draft/monitor")
	inject(t, transport, ":server 005 omairc CHANTYPES=# MONITOR=10 :are supported by this server\r\n")
	inject(t, transport, ":omairc!u@h JOIN :#chan\r\n")
	c.SelectConversation("libera", "#chan")

	if !c.SendMessage("/monitor dax") {
		t.Fatalf("/monitor dax = false")
	}
	if !writtenFramesContain(transport, "MONITOR + dax") {
		t.Fatalf("no MONITOR + dax frame: %v", transport.WrittenFrames())
	}

	// Unknown -> Offline records but plants no row; Offline -> Online plants.
	inject(t, transport, ":server 731 omairc :dax!user@host\r\n")
	if got := c.InboxCount(); got != 0 {
		t.Fatalf("InboxCount after 731 = %d, want 0", got)
	}
	inject(t, transport, ":server 730 omairc :dax!user@host\r\n")
	if got := c.InboxCount(); got != 1 {
		t.Fatalf("InboxCount after 730 = %d, want 1", got)
	}
	row := c.InboxItems()[0]
	if row.Kind != "monitorOnline" {
		t.Fatalf("Kind = %q, want monitorOnline", row.Kind)
	}
	if row.Actor != "dax" {
		t.Fatalf("Actor = %q, want dax", row.Actor)
	}

	c.RevealConversation("libera", "dax")
	if got := c.InboxCount(); got != 0 {
		t.Fatalf("InboxCount after reveal = %d, want 0", got)
	}
	if c.SelectedTarget() != "dax" {
		t.Fatalf("SelectedTarget = %q, want dax", c.SelectedTarget())
	}
}

func TestInboxPurgesNetworkOnForget(t *testing.T) {
	c, transport := inboxMentionSetup(t)

	inject(t, transport, ":omairc!u@h JOIN :#lab\r\n"+
		"@msgid=m1 :alice!u@h PRIVMSG #lab :omairc: ping\r\n")
	if got := c.InboxCount(); got != 1 {
		t.Fatalf("InboxCount before forget = %d, want 1", got)
	}

	changed := 0
	c.OnInboxChanged = func() { changed++ }
	c.ForgetNetworkState("libera")

	if got := c.InboxCount(); got != 0 {
		t.Fatalf("InboxCount after forget = %d, want 0", got)
	}
	if changed == 0 {
		t.Fatalf("OnInboxChanged did not fire on forget")
	}
}

func TestPlainIrcTextPassthrough(t *testing.T) {
	c, _ := newController(t)
	if got := c.PlainIrcText("hey \x02fred"); got != "hey fred" {
		t.Fatalf("PlainIrcText = %q, want %q", got, "hey fred")
	}
}
