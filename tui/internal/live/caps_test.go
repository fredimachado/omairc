//go:build live

package live

import (
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
)

// Mirrors `client/tests/live/caps.rs`.

func TestCapabilityFlags(t *testing.T) {
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
		var away, typing, memberStatus bool
		client.Do(func(ctrl *controller.Controller) {
			away = ctrl.HasAwayPresence()
			typing = ctrl.HasTyping()
			memberStatus = ctrl.HasMemberStatus()
		})
		if away != daemon.AwayNotify {
			t.Errorf("%s away presence = %v, want %v", daemon.Name, away, daemon.AwayNotify)
		}
		if typing != daemon.MessageTags {
			t.Errorf("%s typing = %v, want %v", daemon.Name, typing, daemon.MessageTags)
		}
		if memberStatus != daemon.Metadata {
			t.Errorf("%s member status = %v, want %v", daemon.Name, memberStatus, daemon.Metadata)
		}
		caps := client.Capabilities()
		if !daemon.WantedCapsAdvertised {
			for _, capability := range []irc.Capability{
				irc.CapabilityAwayNotify,
				irc.CapabilityMessageTags,
				irc.CapabilityMemberMetadata,
				irc.CapabilityInviteNotify,
				irc.CapabilitySetName,
				irc.CapabilityUserhostInNames,
			} {
				if caps.Contains(capability) {
					t.Errorf("%s negotiated %v", daemon.Name, capability)
				}
			}
		}
		// Which daemons advertise the three identity caps, as observed on the
		// fixtures: Ergo all of them, Solanum only userhost-in-names, and
		// ngIRCd none (the negative block above).
		var inviteNotify, setName, userhostInNames bool
		switch daemon.Name {
		case "ergo":
			inviteNotify, setName, userhostInNames = true, true, true
		case "solanum":
			userhostInNames = true
		}
		if got := caps.Contains(irc.CapabilityInviteNotify); got != inviteNotify {
			t.Errorf("%s invite-notify = %v, want %v", daemon.Name, got, inviteNotify)
		}
		if got := caps.Contains(irc.CapabilitySetName); got != setName {
			t.Errorf("%s setname = %v, want %v", daemon.Name, got, setName)
		}
		if got := caps.Contains(irc.CapabilityUserhostInNames); got != userhostInNames {
			t.Errorf("%s userhost-in-names = %v, want %v", daemon.Name, got, userhostInNames)
		}
	}
}

func TestIsupport(t *testing.T) {
	for _, daemon := range requireLive(t) {
		client := Connect(t, daemon, UniqueNick(daemon.NickLength), daemon.UseTLS(), Options{})
		if !client.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, client.LastError())
		}
		if !client.WaitUntil(func(c *Client) bool {
			features := c.Features()
			_, ok := features.NickLength()
			return ok
		}) {
			t.Fatalf("%s nicklen", daemon.Name)
		}
		features := client.Features()
		if got := features.CaseMapping().Kind(); got != daemon.Mapping {
			t.Errorf("%s case mapping = %v, want %v", daemon.Name, got, daemon.Mapping)
		}
		if features.PrefixModes() == "" {
			t.Errorf("%s: empty PREFIX modes", daemon.Name)
		}
		if features.PrefixSymbols() == "" {
			t.Errorf("%s: empty PREFIX symbols", daemon.Name)
		}
		if !features.IsChannel("#live") {
			t.Errorf("%s: #live is not a channel", daemon.Name)
		}
		if length, _ := features.NickLength(); length <= 0 {
			t.Errorf("%s: NICKLEN %d", daemon.Name, length)
		}
	}
}
