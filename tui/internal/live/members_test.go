//go:build live

package live

import (
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
)

// Mirrors `client/tests/live/members.rs` peer_nick_change.

func TestPeerNickChange(t *testing.T) {
	for _, daemon := range requireLive(t) {
		if daemon.NickLength < 5 {
			continue
		}
		useTLS := daemon.UseTLS()
		base := UniqueNick(daemon.NickLength - 1)
		oldNick, newNick := base+"[", base+"{"
		channel := UniqueChannel()

		client := Connect(t, daemon, UniqueNick(daemon.NickLength), useTLS, Options{})
		if !client.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, client.LastError())
		}
		if !client.JoinChannel(channel) || !waitNamesEnd(client, channel) {
			t.Fatalf("%s: client join %s", daemon.Name, channel)
		}
		client.SelectChannel(channel)

		peer := Connect(t, daemon, oldNick, useTLS, Options{})
		if !peer.WaitRegistered() {
			t.Fatalf("%s peer: %s", daemon.Name, peer.LastError())
		}
		if !peer.JoinChannel(channel) || !waitNamesEnd(peer, channel) {
			t.Fatalf("%s: peer join %s", daemon.Name, channel)
		}
		if !client.WaitUntil(func(c *Client) bool {
			_, ok := c.Member(oldNick)
			return ok
		}) {
			t.Fatalf("%s peer visible as %s", daemon.Name, oldNick)
		}

		features := client.Features()
		folded := features.CaseMapping().Equals(oldNick, newNick)
		if daemon.Mapping == irc.KindRfc1459 {
			if !folded {
				t.Fatalf("%s rfc1459 must fold %s and %s", daemon.Name, oldNick, newNick)
			}
		} else if folded {
			t.Fatalf("%s ascii must keep %s and %s distinct", daemon.Name, oldNick, newNick)
		}

		var opened bool
		client.Do(func(ctrl *controller.Controller) {
			opened = ctrl.OpenDirectMessage(oldNick)
		})
		if !opened {
			t.Fatalf("%s OpenDirectMessage", daemon.Name)
		}

		before := "before-" + channel
		clientNick := client.Nick()
		var sent bool
		peer.Do(func(ctrl *controller.Controller) {
			sent = ctrl.SendToTarget(peer.NetworkID, clientNick, before)
		})
		if !sent {
			t.Fatalf("%s: peer DM: %s", daemon.Name, peer.LastError())
		}
		if !client.WaitUntil(func(c *Client) bool { return channelHasBody(c, before) }) {
			t.Fatalf("%s DM before-marker", daemon.Name)
		}

		rowBefore := directRowsFor(client, oldNick)
		if len(rowBefore) != 1 {
			t.Fatalf("%s expected one DM before rename, got %d", daemon.Name, len(rowBefore))
		}
		idBefore := rowBefore[0].ConversationID

		if !peer.Session().SendRaw("NICK " + newNick) {
			t.Fatalf("%s: NICK: %s", daemon.Name, peer.LastError())
		}

		if !client.WaitUntil(func(c *Client) bool {
			rows := directRowsFor(c, newNick)
			return len(rows) == 1 && rows[0].Conversation == newNick
		}) {
			t.Fatalf("%s DM retargeted to %s", daemon.Name, newNick)
		}
		newRows := directRowsFor(client, newNick)
		if len(newRows) != 1 {
			t.Fatalf("%s one DM for new nick", daemon.Name)
		}
		if newRows[0].Conversation != newNick {
			t.Fatalf("%s DM spelling = %q, want %q", daemon.Name, newRows[0].Conversation, newNick)
		}
		oldRows := directRowsFor(client, oldNick)
		if folded {
			if len(oldRows) != 1 {
				t.Fatalf("%s folded DM still one row", daemon.Name)
			}
			if oldRows[0].Conversation != newNick {
				t.Fatalf("%s folded DM spelling = %q, want %q", daemon.Name, oldRows[0].Conversation, newNick)
			}
			if oldRows[0].ConversationID != newRows[0].ConversationID {
				t.Fatalf("%s same conversation key", daemon.Name)
			}
			if newRows[0].ConversationID != idBefore {
				t.Fatalf("%s rfc1459 must keep conversation id across rename", daemon.Name)
			}
		} else {
			if len(oldRows) != 0 {
				t.Fatalf("%s ascii must not keep a DM under %s", daemon.Name, oldNick)
			}
			if newRows[0].ConversationID == idBefore {
				t.Fatalf("%s ascii must move DM to a new conversation id", daemon.Name)
			}
		}

		client.SelectChannel(channel)
		member, ok := client.Member(newNick)
		if !ok || member.Nick != newNick {
			t.Fatalf("%s member display %s", daemon.Name, newNick)
		}
		if _, ok := client.Member(oldNick); ok {
			t.Fatalf("%s old spelling gone from members", daemon.Name)
		}

		client.SelectChannel(newNick)
		if !channelHasBody(client, before) {
			t.Fatalf("%s DM history moved with rename", daemon.Name)
		}
	}
}
