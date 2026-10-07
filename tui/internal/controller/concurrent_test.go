package controller

import (
	"fmt"
	"sync"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// TestSelectDuringInboundPresenceDoesNotRace reproduces the crash in
// omairc-tui.log: the shell walks conversations (SelectConversation ->
// MemberView -> presence Lookup) while a session goroutine folds an inbound
// line (MessageReceived -> Apply) that writes those same maps.
func TestSelectDuringInboundPresenceDoesNotRace(t *testing.T) {
	c, _ := newController(t)
	const network = "freenode"
	names := make([]irc.Name, 64)
	for i := range names {
		names[i] = irc.Name{Nick: fmt.Sprintf("user%d", i)}
	}
	c.Apply(irc.JoinEvent{NetworkID: network, Channel: "#room", Nick: "me"})
	c.Apply(irc.NamesEvent{NetworkID: network, Channel: "#room", Names: names, Complete: true})
	c.Apply(irc.JoinEvent{NetworkID: network, Channel: "#other", Nick: "me"})
	c.Apply(irc.NamesEvent{NetworkID: network, Channel: "#other", Names: names, Complete: true})
	c.SelectConversation(network, "#room")
	if len(c.Members()) == 0 {
		t.Fatal("seeded channel has no members")
	}

	var wg sync.WaitGroup
	wg.Add(2)
	go func() {
		defer wg.Done()
		for i := 0; i < 300; i++ {
			target := "#room"
			if i%2 == 0 {
				target = "#other"
			}
			// The shell holds Lock across the turn that walks conversations.
			c.Lock()
			c.SelectConversation(network, target)
			_ = c.Members()
			c.Unlock()
		}
	}()
	go func() {
		defer wg.Done()
		for i := 0; i < 300; i++ {
			nick := names[i%len(names)].Nick
			params := []string(nil)
			if i%2 == 0 {
				params = []string{"brb"}
			}
			c.MessageReceived(network, irc.Message{
				Prefix:  &irc.Prefix{Nick: nick, User: "u", Host: "h"},
				Command: "AWAY",
				Params:  params,
			})
		}
	}()
	wg.Wait()
	if len(c.Members()) == 0 {
		t.Fatal("member snapshot disappeared")
	}
}
