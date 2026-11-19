//go:build live

package live

import (
	"maps"
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
)

// Mirrors `client/tests/live/invites.rs`.

// observerDirectIDs returns conversation ids of the observer's direct messages
// on its primary network.
func observerDirectIDs(c *Client) map[string]struct{} {
	ids := make(map[string]struct{})
	c.Do(func(ctrl *controller.Controller) {
		for _, row := range ctrl.Conversations() {
			if row.Direct && row.NetworkID == c.NetworkID {
				ids[row.ConversationID] = struct{}{}
			}
		}
	})
	return ids
}

func TestInviteNotifyLine(t *testing.T) {
	for _, daemon := range requireLive(t) {
		useTLS := daemon.UseTLS()
		observer := Connect(t, daemon, UniqueNick(daemon.NickLength), useTLS, Options{})
		if !observer.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, observer.LastError())
		}

		// Skip daemons that did not negotiate invite-notify (Solanum and ngIRCd
		// do not advertise it on these fixtures; capability_flags pins that).
		if !observer.Capabilities().Contains(irc.CapabilityInviteNotify) {
			continue
		}

		channel := UniqueChannel()
		if !observer.JoinChannel(channel) || !waitNamesEnd(observer, channel) {
			t.Fatalf("%s: observer join %s", daemon.Name, channel)
		}
		observer.SelectChannel(channel)

		inviter := Connect(t, daemon, UniqueNick(daemon.NickLength), useTLS, Options{})
		if !inviter.WaitRegistered() {
			t.Fatalf("%s inviter: %s", daemon.Name, inviter.LastError())
		}
		if !inviter.JoinChannel(channel) || !waitNamesEnd(inviter, channel) {
			t.Fatalf("%s: inviter join %s", daemon.Name, channel)
		}
		inviter.SelectChannel(channel)

		inviteeNick := UniqueNick(daemon.NickLength)
		invitee := Connect(t, daemon, inviteeNick, useTLS, Options{})
		if !invitee.WaitRegistered() {
			t.Fatalf("%s invitee: %s", daemon.Name, invitee.LastError())
		}

		inviterNick := inviter.Nick()
		if !observer.WaitUntil(func(c *Client) bool {
			_, ok := c.Member(inviterNick)
			return ok
		}) {
			t.Fatalf("%s member %s", daemon.Name, inviterNick)
		}

		directsBefore := observerDirectIDs(observer)

		if !inviter.Submit("/invite " + inviteeNick + " " + channel) {
			t.Fatalf("%s /invite: %s", daemon.Name, inviter.LastError())
		}

		line := inviterNick + " invited " + inviteeNick
		if !observer.WaitUntil(func(c *Client) bool {
			for _, message := range c.Messages() {
				if message.Body == line && message.Kind == "event" {
					return true
				}
			}
			return false
		}) {
			t.Fatalf("%s event line %s", daemon.Name, line)
		}

		// Positive control: the INVITE reached the invitee.
		if !invitee.WaitUntil(func(c *Client) bool {
			invite, ok := c.Session().PendingInvite()
			return ok && strings.EqualFold(invite.Channel, channel)
		}) {
			t.Fatalf("%s invitee pending_invite for %s", daemon.Name, channel)
		}

		if !observer.Fence("invite") {
			t.Fatalf("%s fence", daemon.Name)
		}

		matching := 0
		for _, message := range observer.Messages() {
			if message.Body == line {
				matching++
			}
		}
		if matching != 1 {
			t.Fatalf("%s exactly one invite line, got %d", daemon.Name, matching)
		}

		for _, entry := range observer.StatusLines() {
			text := entry.Text()
			if strings.Contains(text, "invited") || strings.Contains(text, inviteeNick) {
				t.Fatalf("%s Status must not mention invite (%s)", daemon.Name, text)
			}
		}

		var inbox []controller.InboxSnapshot
		observer.Do(func(ctrl *controller.Controller) {
			inbox = append(inbox, ctrl.InboxItems()...)
		})
		for _, item := range inbox {
			if item.Kind == "invite" {
				t.Fatalf("%s no Invite inbox row, got %+v", daemon.Name, item)
			}
		}

		if _, ok := observer.Session().PendingInvite(); ok {
			t.Fatalf("%s observer pending_invite is None (no /join link)", daemon.Name)
		}

		if !maps.Equal(observerDirectIDs(observer), directsBefore) {
			t.Fatalf("%s invite invents no DM", daemon.Name)
		}
	}
}
