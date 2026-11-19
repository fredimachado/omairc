//go:build live

package live

import (
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// Mirrors `client/tests/live/channel.rs`.

func TestPeerChannelChanges(t *testing.T) {
	for _, daemon := range requireLive(t) {
		useTLS := daemon.UseTLS()
		channel := UniqueChannel()
		topic := "changed " + channel
		reason := "out-" + channel

		op := Connect(t, daemon, UniqueNick(daemon.NickLength), useTLS, Options{})
		if !op.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, op.LastError())
		}
		if !op.JoinChannel(channel) || !waitNamesEnd(op, channel) {
			t.Fatalf("%s: op join %s", daemon.Name, channel)
		}
		op.SelectChannel(channel)

		client := Connect(t, daemon, UniqueNick(daemon.NickLength), useTLS, Options{})
		if !client.WaitRegistered() {
			t.Fatalf("%s client: %s", daemon.Name, client.LastError())
		}
		if !client.JoinChannel(channel) || !waitNamesEnd(client, channel) {
			t.Fatalf("%s: client join %s", daemon.Name, channel)
		}
		client.SelectChannel(channel)

		leaver := Connect(t, daemon, UniqueNick(daemon.NickLength), useTLS, Options{})
		if !leaver.WaitRegistered() {
			t.Fatalf("%s leaver: %s", daemon.Name, leaver.LastError())
		}
		if !leaver.JoinChannel(channel) || !waitNamesEnd(leaver, channel) {
			t.Fatalf("%s: leaver join %s", daemon.Name, channel)
		}

		opNick := op.Nick()
		clientNick := client.Nick()
		leaverNick := leaver.Nick()
		if !client.WaitUntil(func(c *Client) bool {
			_, haveOp := c.Member(opNick)
			_, haveLeaver := c.Member(leaverNick)
			return haveOp && haveLeaver
		}) {
			t.Fatalf("%s members visible", daemon.Name)
		}
		var people, memberCount int
		client.Do(func(ctrl *controller.Controller) {
			people = ctrl.PeopleCount()
			memberCount = len(ctrl.Members())
		})
		if people != 3 || memberCount != 3 {
			t.Fatalf("%s people_count=%d members=%d, want 3", daemon.Name, people, memberCount)
		}

		if !op.Submit("/topic " + topic) {
			t.Fatalf("%s /topic: %s", daemon.Name, op.LastError())
		}
		if !client.WaitUntil(func(c *Client) bool { return c.Topic() == topic }) {
			t.Fatalf("%s topic", daemon.Name)
		}

		var disconnected bool
		leaver.Do(func(ctrl *controller.Controller) {
			disconnected = ctrl.Disconnect(leaver.NetworkID)
		})
		if !disconnected {
			t.Fatalf("%s leaver disconnect", daemon.Name)
		}
		if !client.WaitUntil(func(c *Client) bool {
			_, ok := c.Member(leaverNick)
			return !ok
		}) {
			t.Fatalf("%s leaver gone", daemon.Name)
		}
		var joinedBeforeKick bool
		client.Do(func(ctrl *controller.Controller) {
			people = ctrl.PeopleCount()
			memberCount = len(ctrl.Members())
			joinedBeforeKick = ctrl.ChannelJoined()
		})
		if people != 2 || memberCount != 2 {
			t.Fatalf("%s after quit people=%d members=%d, want 2", daemon.Name, people, memberCount)
		}
		if !joinedBeforeKick {
			t.Fatalf("%s client joined before kick", daemon.Name)
		}

		if !op.Submit("/kick " + clientNick + " " + reason) {
			t.Fatalf("%s /kick: %s", daemon.Name, op.LastError())
		}
		if !client.WaitUntil(func(c *Client) bool {
			var joined bool
			c.Do(func(ctrl *controller.Controller) { joined = ctrl.ChannelJoined() })
			return !joined
		}) {
			t.Fatalf("%s kicked", daemon.Name)
		}
		if !client.HasConversation(client.NetworkID, channel) {
			t.Fatalf("%s kicked channel keeps its row", daemon.Name)
		}
		var inboxKick bool
		var kickLine bool
		kickedBody := clientNick + " was kicked"
		client.Do(func(ctrl *controller.Controller) {
			people = ctrl.PeopleCount()
			memberCount = len(ctrl.Members())
			for _, item := range ctrl.InboxItems() {
				if item.Kind == "kick" &&
					strings.EqualFold(item.Target, channel) &&
					strings.EqualFold(item.Actor, opNick) &&
					item.Preview == reason {
					inboxKick = true
				}
			}
			for _, message := range ctrl.Messages() {
				if message.Body == kickedBody {
					kickLine = true
				}
			}
		})
		if people != 0 || memberCount != 0 {
			t.Fatalf("%s after kick people=%d members=%d, want 0", daemon.Name, people, memberCount)
		}
		if !inboxKick {
			t.Fatalf("%s kick inbox", daemon.Name)
		}
		if !kickLine {
			t.Fatalf("%s kick event line", daemon.Name)
		}
	}
}
