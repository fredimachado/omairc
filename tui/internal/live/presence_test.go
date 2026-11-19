//go:build live

package live

import (
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// Mirrors `client/tests/live/presence.rs` self_away_and_back.

func TestSelfAwayAndBack(t *testing.T) {
	for _, daemon := range requireLive(t) {
		client := Connect(t, daemon, UniqueNick(daemon.NickLength), daemon.UseTLS(), Options{})
		if !client.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, client.LastError())
		}
		channel := UniqueChannel()
		if !client.JoinChannel(channel) || !waitNamesEnd(client, channel) {
			t.Fatalf("%s: join %s", daemon.Name, channel)
		}
		client.SelectChannel(channel)

		own := client.Nick()
		if !client.WaitUntil(func(c *Client) bool {
			member, ok := c.Member(own)
			return ok && !member.Away
		}) {
			t.Fatalf("%s own row present", daemon.Name)
		}

		reason := "gone-" + channel
		if !client.Submit("/away " + reason) {
			t.Fatalf("%s: /away: %s", daemon.Name, client.LastError())
		}
		if !client.WaitUntil(func(c *Client) bool {
			var selfAway bool
			c.Do(func(ctrl *controller.Controller) { selfAway = ctrl.SelfAway() })
			member, ok := c.Member(own)
			return MessageHasCommand(c.Incoming(), "306") && selfAway && ok && member.Away
		}) {
			t.Fatalf("%s /away paints own row", daemon.Name)
		}

		if !client.Submit("/back") {
			t.Fatalf("%s: /back: %s", daemon.Name, client.LastError())
		}
		if !client.WaitUntil(func(c *Client) bool {
			var selfAway bool
			c.Do(func(ctrl *controller.Controller) { selfAway = ctrl.SelfAway() })
			member, ok := c.Member(own)
			return MessageHasCommand(c.Incoming(), "305") && !selfAway && ok && !member.Away
		}) {
			t.Fatalf("%s /back clears own row", daemon.Name)
		}
	}
}
