//go:build live

package live

import (
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// Mirrors `client/tests/live/classic.rs`.

func TestClassicTraffic(t *testing.T) {
	for _, daemon := range requireLive(t) {
		useTLS := daemon.UseTLS()
		client := Connect(t, daemon, UniqueNick(daemon.NickLength), useTLS, Options{})
		if !client.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, client.LastError())
		}
		peer := Connect(t, daemon, UniqueNick(daemon.NickLength), useTLS, Options{})
		if !peer.WaitRegistered() {
			t.Fatalf("%s peer: %s", daemon.Name, peer.LastError())
		}

		channel := UniqueChannel()
		if !client.JoinChannel(channel) || !waitNamesEnd(client, channel) {
			t.Fatalf("%s: client join %s", daemon.Name, channel)
		}
		if !peer.JoinChannel(channel) || !waitNamesEnd(peer, channel) {
			t.Fatalf("%s: peer join %s", daemon.Name, channel)
		}
		client.SelectChannel(channel)
		peer.SelectChannel(channel)

		const (
			topic   = "live topic"
			privmsg = "hello live"
			notice  = "notice live"
			action  = "waves"
		)
		if !client.Session().SetTopic(channel, topic) {
			t.Fatalf("%s: set topic", daemon.Name)
		}
		var sent bool
		client.Do(func(ctrl *controller.Controller) {
			sent = ctrl.SendToTarget(client.NetworkID, channel, privmsg)
		})
		if !sent {
			t.Fatalf("%s: send privmsg: %s", daemon.Name, client.LastError())
		}
		if !client.Submit("/notice " + channel + " " + notice) {
			t.Fatalf("%s: /notice: %s", daemon.Name, client.LastError())
		}
		if !client.Submit("/me " + action) {
			t.Fatalf("%s: /me: %s", daemon.Name, client.LastError())
		}
		if !client.Submit("/part " + channel) {
			t.Fatalf("%s: /part: %s", daemon.Name, client.LastError())
		}

		ok := peer.WaitUntil(func(peer *Client) bool {
			peer.SelectChannel(channel)
			var sawPrivmsg, sawAction, sawNotice bool
			for _, message := range peer.Messages() {
				switch {
				case message.Body == privmsg:
					sawPrivmsg = true
				case message.Body == notice:
					sawNotice = true
				case strings.Contains(message.Body, action):
					sawAction = true
				}
			}
			if !sawNotice {
				sawNotice = statusTextContains(peer, notice)
			}
			return sawPrivmsg && sawAction && sawNotice && peer.Topic() == topic
		})
		if !ok {
			t.Fatalf("%s classic peer did not see traffic", daemon.Name)
		}
	}
}

func TestJoinMultipleChannels(t *testing.T) {
	for _, daemon := range requireLive(t) {
		client := Connect(t, daemon, UniqueNick(daemon.NickLength), daemon.UseTLS(), Options{})
		if !client.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, client.LastError())
		}
		first := UniqueChannel()
		second := UniqueChannel()
		client.Do(func(ctrl *controller.Controller) {
			ctrl.OpenStatus(client.NetworkID)
			ctrl.ConsoleSubmit("/join " + first + ", " + strings.TrimPrefix(second, "#"))
		})
		if !client.WaitUntil(func(c *Client) bool {
			return namesEndCount(c.Incoming(), first)+namesEndCount(c.Incoming(), second) >= 2
		}) {
			t.Fatalf("%s join ends", daemon.Name)
		}
		if !client.WaitUntil(func(c *Client) bool {
			return c.HasConversation(c.NetworkID, first) && c.HasConversation(c.NetworkID, second)
		}) {
			t.Fatalf("%s: sidebar lacks %s or %s", daemon.Name, first, second)
		}

		keyed := UniqueChannel()
		if !client.JoinChannel(keyed) || !waitNamesEnd(client, keyed) {
			t.Fatalf("%s: join %s", daemon.Name, keyed)
		}
		client.SelectChannel(keyed)
		if !client.Submit("/mode " + keyed + " +k s3cret") {
			t.Fatalf("%s: /mode: %s", daemon.Name, client.LastError())
		}
		if !client.WaitUntil(func(c *Client) bool { return sawChannelModeKey(c.Incoming(), keyed) }) {
			t.Fatalf("%s mode +k", daemon.Name)
		}
		if !client.Submit("/part " + keyed) {
			t.Fatalf("%s: /part: %s", daemon.Name, client.LastError())
		}
		if !client.WaitUntil(func(c *Client) bool { return sawPart(c.Incoming(), keyed) }) {
			t.Fatalf("%s part", daemon.Name)
		}
		if !client.Submit("/join " + keyed + " s3cret") {
			t.Fatalf("%s: /join with key: %s", daemon.Name, client.LastError())
		}
		if !client.WaitUntil(func(c *Client) bool { return namesEndCount(c.Incoming(), keyed) >= 2 }) {
			t.Fatalf("%s rejoin", daemon.Name)
		}
	}
}
