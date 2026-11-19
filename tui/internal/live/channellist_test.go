//go:build live

package live

import (
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// Mirrors `client/tests/live/channellist.rs`.

func isListLine(line string) bool {
	return line == "LIST" || strings.HasPrefix(line, "LIST ")
}

func listLines(sent []string) []string {
	var out []string
	for _, line := range sent {
		if isListLine(line) {
			out = append(out, line)
		}
	}
	return out
}

func TestChannelListCompletesInOrder(t *testing.T) {
	for _, daemon := range requireLive(t) {
		client := Connect(t, daemon, UniqueNick(daemon.NickLength), daemon.UseTLS(), Options{})
		if !client.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, client.LastError())
		}

		first := UniqueChannel()
		second := UniqueChannel()
		if !client.JoinChannel(first) || !waitNamesEnd(client, first) {
			t.Fatalf("%s: join %s", daemon.Name, first)
		}
		if !client.JoinChannel(second) || !waitNamesEnd(client, second) {
			t.Fatalf("%s: join %s", daemon.Name, second)
		}
		client.SelectChannel(first)

		topic := "listed " + first
		if !client.Submit("/topic " + topic) {
			t.Fatalf("%s /topic: %s", daemon.Name, client.LastError())
		}
		if !client.WaitUntil(func(c *Client) bool { return c.Topic() == topic }) {
			t.Fatalf("%s topic", daemon.Name)
		}

		// Leg 1: one exact-mask /list completes with that channel and its topic.
		if !client.Submit("/list " + first) {
			t.Fatalf("%s /list %s: %s", daemon.Name, first, client.LastError())
		}
		if !client.WaitUntil(func(c *Client) bool {
			var view controller.ChannelListSnapshot
			c.Do(func(ctrl *controller.Controller) { view = ctrl.ChannelListSnapshot() })
			return view.Complete && !view.Loading && view.Mask == first
		}) {
			t.Fatalf("%s leg1 complete mask=%s", daemon.Name, first)
		}
		var view controller.ChannelListSnapshot
		client.Do(func(ctrl *controller.Controller) { view = ctrl.ChannelListSnapshot() })
		if view.Error != "" {
			t.Fatalf("%s leg1 error=%q", daemon.Name, view.Error)
		}
		if len(view.Rows) != 1 {
			t.Fatalf("%s leg1 rows=%v", daemon.Name, view.Rows)
		}
		if !strings.EqualFold(view.Rows[0].Channel, first) {
			t.Fatalf("%s leg1 name=%q want %s", daemon.Name, view.Rows[0].Channel, first)
		}
		if view.Rows[0].Users != 1 {
			t.Fatalf("%s leg1 users=%d", daemon.Name, view.Rows[0].Users)
		}
		if !strings.Contains(view.Rows[0].Topic, topic) {
			t.Fatalf("%s leg1 topic=%q want contains %s", daemon.Name, view.Rows[0].Topic, topic)
		}
		if !client.Fence("list-leg1") {
			t.Fatalf("%s leg1 fence", daemon.Name)
		}

		// Leg 2: a second /list queued behind the first keeps its own end.
		// One lock turn so no 323 can land between the two submits; assert the
		// queued mask has not written LIST yet before the turn ends.
		sentAt := len(client.Sent())
		wantSecond := "LIST " + second
		wantFirst := "LIST " + first
		var sentSecond, sentFirst bool
		var mid []string
		client.Do(func(ctrl *controller.Controller) {
			sentSecond = ctrl.SendMessage("/list " + second)
			sentFirst = ctrl.SendMessage("/list " + first)
			transport := client.transports[client.NetworkID]
			if transport != nil {
				mid = listLines(transport.sent()[sentAt:])
			}
		})
		if !sentSecond || !sentFirst {
			t.Fatalf("%s leg2 submits second=%v first=%v err=%s", daemon.Name, sentSecond, sentFirst, client.LastError())
		}
		if len(mid) != 1 || mid[0] != wantSecond {
			t.Fatalf("%s leg2 mid LIST before turn end %v want [%s]", daemon.Name, mid, wantSecond)
		}
		if !client.WaitUntil(func(c *Client) bool {
			var view controller.ChannelListSnapshot
			c.Do(func(ctrl *controller.Controller) { view = ctrl.ChannelListSnapshot() })
			return view.Complete && !view.Loading && view.Mask == first
		}) {
			t.Fatalf("%s leg2 complete mask=%s", daemon.Name, first)
		}
		client.Do(func(ctrl *controller.Controller) { view = ctrl.ChannelListSnapshot() })
		if view.Error != "" {
			t.Fatalf("%s leg2 error=%q", daemon.Name, view.Error)
		}
		if len(view.Rows) != 1 {
			t.Fatalf("%s leg2 rows=%v", daemon.Name, view.Rows)
		}
		if !strings.EqualFold(view.Rows[0].Channel, first) {
			t.Fatalf("%s leg2 name=%q want %s", daemon.Name, view.Rows[0].Channel, first)
		}
		if view.Rows[0].Users != 1 {
			t.Fatalf("%s leg2 users=%d", daemon.Name, view.Rows[0].Users)
		}
		if !client.Fence("list-leg2") {
			t.Fatalf("%s leg2 fence", daemon.Name)
		}

		lists := listLines(client.Sent()[sentAt:])
		if len(lists) != 2 || lists[0] != wantSecond || lists[1] != wantFirst {
			t.Fatalf("%s leg2 LIST order %v want [%s %s]", daemon.Name, lists, wantSecond, wantFirst)
		}
	}
}
