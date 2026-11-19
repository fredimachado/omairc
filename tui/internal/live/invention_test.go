//go:build live

package live

import (
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
)

// Mirrors `client/tests/live/invention.rs`: EnsureConversation causes over
// real wire. Composer /msg is QuietSend (no row, selection kept, even after
// the echo-message echo); a NickServ reply stays on Status; an inbound human
// PRIVMSG is InboundOther and invents the direct message.

func TestConversationInventionMatrix(t *testing.T) {
	daemon := requireDaemon(t, "ergo")
	useTLS := daemon.UseTLS()
	peerNick := UniqueNick(daemon.NickLength)
	targetNick := UniqueNick(daemon.NickLength)
	const (
		dmBody   = "hello"
		peerBody = "hi"
	)

	client := Connect(t, daemon, UniqueNick(daemon.NickLength), useTLS, Options{})
	if !client.WaitRegistered() {
		t.Fatalf("register: %s", client.LastError())
	}
	if !client.WaitUntil(func(c *Client) bool {
		return c.Capabilities().Contains(irc.CapabilityEchoMessage)
	}) {
		t.Skip("ergo did not negotiate echo-message")
	}
	network := client.NetworkID
	client.Do(func(ctrl *controller.Controller) { ctrl.OpenStatus(network) })

	channel := UniqueChannel()
	if !client.JoinChannel(channel) || !waitNamesEnd(client, channel) {
		t.Fatalf("join %s", channel)
	}
	client.SelectChannel(channel)
	if got := client.SelectedTarget(); got != channel {
		t.Fatalf("selected %q, want %q", got, channel)
	}

	msgTarget := Connect(t, daemon, targetNick, useTLS, Options{})
	if !msgTarget.WaitRegistered() {
		t.Fatalf("target register: %s", msgTarget.LastError())
	}
	if !client.Submit("/msg " + targetNick + " " + dmBody) {
		t.Fatalf("/msg: %s", client.LastError())
	}
	if client.HasConversation(network, targetNick) {
		t.Fatal("/msg invented a direct message before the echo")
	}
	if got := client.SelectedTarget(); got != channel {
		t.Fatalf("/msg moved selection to %q", got)
	}

	selfNick := client.Nick()
	if !client.WaitUntil(func(c *Client) bool {
		for _, message := range c.Incoming() {
			if message.Command == "PRIVMSG" &&
				message.Prefix != nil && strings.EqualFold(message.Prefix.Nick, selfNick) &&
				paramIs(message, 0, targetNick) &&
				len(message.Params) > 0 && message.Params[len(message.Params)-1] == dmBody {
				return true
			}
		}
		return false
	}) {
		t.Fatal("no echo-message echo of /msg")
	}
	if client.HasConversation(network, targetNick) {
		t.Fatal("the /msg echo invented a direct message")
	}
	if got := client.SelectedTarget(); got != channel {
		t.Fatalf("the /msg echo moved selection to %q", got)
	}
	if channelHasBody(client, dmBody) {
		t.Fatal("the /msg body landed in the channel transcript")
	}

	if !client.Session().SendPrivmsg("NickServ", "HELP") {
		t.Fatal("PRIVMSG NickServ")
	}
	if !client.WaitUntil(func(c *Client) bool {
		for _, message := range c.Incoming() {
			if message.Command == "NOTICE" && message.Prefix != nil &&
				strings.EqualFold(message.Prefix.Nick, "NickServ") {
				return true
			}
		}
		return false
	}) {
		t.Fatal("no NOTICE from NickServ")
	}
	if client.HasConversation(network, "NickServ") {
		t.Fatal("a NickServ reply invented a direct message")
	}
	sawStatus := statusTextContains(client, "NickServ")
	if !sawStatus {
		for _, entry := range client.StatusLines() {
			if strings.EqualFold(entry.Label(), "NOTICE") {
				sawStatus = true
				break
			}
		}
	}
	if !sawStatus {
		t.Fatal("the NickServ reply is not on Status")
	}

	peer := Connect(t, daemon, peerNick, useTLS, Options{})
	if !peer.WaitRegistered() {
		t.Fatalf("peer register: %s", peer.LastError())
	}
	var sent bool
	peer.Do(func(ctrl *controller.Controller) {
		sent = ctrl.SendToTarget(peer.NetworkID, selfNick, peerBody)
	})
	if !sent {
		t.Fatalf("peer send: %s", peer.LastError())
	}
	if !client.WaitUntil(func(c *Client) bool { return c.HasConversation(network, peerNick) }) {
		t.Fatal("an inbound human PRIVMSG did not invent the direct message")
	}
	if got := client.SelectedTarget(); got != channel {
		t.Fatalf("the inbound direct message moved selection to %q", got)
	}
}

func TestFoldedNickCollision(t *testing.T) {
	for _, daemon := range requireLive(t) {
		if daemon.NickLength < 5 {
			continue
		}
		useTLS := daemon.UseTLS()
		base := UniqueNick(daemon.NickLength - 1)
		nickA := base + "["
		nickB := base + "{"
		a := Connect(t, daemon, nickA, useTLS, Options{})
		if !a.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, a.LastError())
		}
		features := a.Features()
		folded := features.CaseMapping().Equals(a.Nick(), nickB)
		b := Connect(t, daemon, nickB, useTLS, Options{})
		if !folded {
			if !b.WaitRegistered() {
				t.Fatalf("%s: %s", daemon.Name, b.LastError())
			}
			continue
		}
		if b.WaitRegistered() {
			// The daemon advertised folding but accepted both nicks.
			continue
		}
		if !b.WaitFailed() {
			t.Fatalf("%s: %s", daemon.Name, b.LastError())
		}
	}
}

func TestStatusIsolationAndSameNickDirects(t *testing.T) {
	daemons := requireLive(t)
	if len(daemons) < 2 {
		t.Skip("dual-network proof needs two daemons in the live profile")
	}
	left, right := daemons[0], daemons[1]
	nickLength := min(left.NickLength, right.NickLength)
	clientNick := UniqueNick(nickLength)
	peerNick := UniqueNick(nickLength)
	leftMark := "inbound-left-" + peerNick
	rightMark := "inbound-right-" + peerNick

	const leftNet, rightNet = "live-left", "live-right"
	client := Connect(t, left, clientNick, left.UseTLS(), Options{NetworkID: leftNet})
	client.ConnectAdditional(right, clientNick, right.UseTLS(), Options{NetworkID: rightNet})
	if !client.WaitRegistered() {
		t.Fatalf("left: %s", client.LastError())
	}
	if !client.WaitRegisteredNetwork(rightNet) {
		t.Fatal("right network did not register")
	}
	leftSelf := client.Nick()
	rightSelf := client.SessionOf(rightNet).Nick()

	peerLeft := Connect(t, left, peerNick, left.UseTLS(), Options{})
	peerRight := Connect(t, right, peerNick, right.UseTLS(), Options{})
	if !peerLeft.WaitRegistered() || !peerRight.WaitRegistered() {
		t.Fatal("peers did not register")
	}
	send := func(peer *Client, target, body string) {
		var sent bool
		peer.Do(func(ctrl *controller.Controller) {
			sent = ctrl.SendToTarget(peer.NetworkID, target, body)
		})
		if !sent {
			t.Fatalf("send %q: %s", body, peer.LastError())
		}
	}
	send(peerLeft, leftSelf, leftMark)
	send(peerRight, rightSelf, rightMark)

	if !client.WaitUntil(func(c *Client) bool {
		return c.HasConversation(leftNet, peerNick) && c.HasConversation(rightNet, peerNick)
	}) {
		t.Fatal("inbound direct messages did not land on both networks")
	}

	statusCarries := func(networkID, mark string) bool {
		for _, entry := range client.StatusLinesOf(networkID) {
			if strings.Contains(entry.Text(), mark) {
				return true
			}
		}
		return false
	}
	// Status on one network must not surface the other network's inbound
	// direct-message body.
	client.Do(func(ctrl *controller.Controller) { ctrl.OpenStatus(leftNet) })
	if statusCarries(leftNet, rightMark) {
		t.Fatal("left Status must not carry the right-network DM body")
	}
	client.Do(func(ctrl *controller.Controller) { ctrl.OpenStatus(rightNet) })
	if statusCarries(rightNet, leftMark) {
		t.Fatal("right Status must not carry the left-network DM body")
	}

	// Same peer nick on both networks: each transcript stays network-scoped.
	selectDirect := func(networkID string) {
		client.Do(func(ctrl *controller.Controller) { ctrl.SelectConversation(networkID, peerNick) })
	}
	selectDirect(leftNet)
	if !client.WaitUntil(func(c *Client) bool { return channelHasBody(c, leftMark) }) {
		t.Fatal("left DM does not show the left-network mark")
	}
	if channelHasBody(client, rightMark) {
		t.Fatal("left DM must not show the right-network mark")
	}
	selectDirect(rightNet)
	if !client.WaitUntil(func(c *Client) bool { return channelHasBody(c, rightMark) }) {
		t.Fatal("right DM does not show the right-network mark")
	}
	if channelHasBody(client, leftMark) {
		t.Fatal("right DM must not show the left-network mark")
	}
	selectDirect(leftNet)
	if !channelHasBody(client, leftMark) || channelHasBody(client, rightMark) {
		t.Fatal("reselecting the left DM must show only the left-network mark")
	}
}
