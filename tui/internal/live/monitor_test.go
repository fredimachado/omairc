//go:build live

package live

import (
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
)

// Mirrors `client/tests/live/monitor.rs`.

// sawMonitorReply reports whether a MONITOR numeric's trailing list names nick
// (entry may be nick!user@host; compared case-insensitively).
func sawMonitorReply(incoming []irc.Message, numeric, nick string) bool {
	for _, message := range incoming {
		if message.Command != numeric || len(message.Params) == 0 {
			continue
		}
		trailing := message.Params[len(message.Params)-1]
		for _, entry := range strings.Split(trailing, ",") {
			beforeBang := entry
			if bang := strings.IndexByte(entry, '!'); bang >= 0 {
				beforeBang = entry[:bang]
			}
			if strings.EqualFold(strings.TrimSpace(beforeBang), nick) {
				return true
			}
		}
	}
	return false
}

func statusLineIs(c *Client, text string) bool {
	for _, entry := range c.StatusLines() {
		if entry.Text() == text {
			return true
		}
	}
	return false
}

func TestMonitorDirectPresence(t *testing.T) {
	for _, daemon := range requireLive(t) {
		useTLS := daemon.UseTLS()
		client := Connect(t, daemon, UniqueNick(daemon.NickLength), useTLS, Options{})
		if !client.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, client.LastError())
		}
		if !client.WaitMotd() {
			t.Fatalf("%s motd", daemon.Name)
		}

		// Skip daemons that do not advertise MONITOR in ISUPPORT.
		features := client.Features()
		if !features.MonitorAdvertised() {
			// Guard against a silent skip on Ergo: the fixture must advertise it.
			if daemon.Name == "ergo" {
				t.Fatalf("%s must advertise MONITOR", daemon.Name)
			}
			continue
		}

		peerNick := UniqueNick(daemon.NickLength)
		peer := Connect(t, daemon, peerNick, useTLS, Options{})
		if !peer.WaitRegistered() {
			t.Fatalf("%s peer: %s", daemon.Name, peer.LastError())
		}

		channel := UniqueChannel()
		if !client.JoinChannel(channel) || !waitNamesEnd(client, channel) {
			t.Fatalf("%s: client join %s", daemon.Name, channel)
		}
		client.SelectChannel(channel)

		// OpenDirectMessage needs a selection to name the network.
		var opened bool
		client.Do(func(ctrl *controller.Controller) {
			opened = ctrl.OpenDirectMessage(peerNick)
		})
		if !opened {
			t.Fatalf("%s OpenDirectMessage", daemon.Name)
		}

		if !client.Submit("/monitor " + peerNick) {
			t.Fatalf("%s /monitor: %s", daemon.Name, client.LastError())
		}
		if !client.WaitUntil(func(c *Client) bool {
			return sawMonitorReply(c.Incoming(), "730", peerNick)
		}) {
			t.Fatalf("%s 730 for %s before QUIT", daemon.Name, peerNick)
		}

		peer.Do(func(ctrl *controller.Controller) {
			ctrl.Disconnect(peer.NetworkID)
		})

		offline := peerNick + " is offline"
		if !client.WaitUntil(func(c *Client) bool { return statusLineIs(c, offline) }) {
			t.Fatalf("%s status %s", daemon.Name, offline)
		}

		again := Connect(t, daemon, peerNick, useTLS, Options{})
		if !again.WaitRegistered() {
			t.Fatalf("%s peer again: %s", daemon.Name, again.LastError())
		}
		if again.Nick() != peerNick {
			t.Fatalf("%s peer nick after reconnect = %q, want %q", daemon.Name, again.Nick(), peerNick)
		}

		online := peerNick + " is online"
		if !client.WaitUntil(func(c *Client) bool {
			if !statusLineIs(c, online) {
				return false
			}
			var items []controller.InboxSnapshot
			c.Do(func(ctrl *controller.Controller) {
				items = ctrl.InboxItems()
			})
			for _, item := range items {
				if item.Kind == "monitorOnline" && item.Actor == peerNick {
					return true
				}
			}
			return false
		}) {
			t.Fatalf("%s status %s and monitorOnline inbox", daemon.Name, online)
		}

		if got := len(directRowsFor(client, peerNick)); got != 1 {
			t.Fatalf("%s MONITOR invents no extra DM: got %d rows", daemon.Name, got)
		}
	}
}
