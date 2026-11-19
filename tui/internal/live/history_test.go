//go:build live

package live

import (
	"fmt"
	"strconv"
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
)

// Mirrors `client/tests/live/history.rs`.

func seedBody(channel string, index int) string {
	return fmt.Sprintf("%s-page-%03d", channel, index)
}

func seedIndices(c *Client, channel string) []int {
	prefix := channel + "-page-"
	var out []int
	for _, message := range c.Messages() {
		body := message.Body
		if !strings.HasPrefix(body, prefix) {
			continue
		}
		n, err := strconv.Atoi(strings.TrimPrefix(body, prefix))
		if err != nil {
			continue
		}
		out = append(out, n)
	}
	return out
}

func sentHistory(lines []string, verb, channel string) int {
	prefix := "CHATHISTORY " + verb + " " + channel + " "
	count := 0
	for _, line := range lines {
		if strings.HasPrefix(line, prefix) {
			count++
		}
	}
	return count
}

func expectedRange(from, to int) []int {
	out := make([]int, 0, to-from+1)
	for i := from; i <= to; i++ {
		out = append(out, i)
	}
	return out
}

func indicesEqual(a, b []int) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func historySentLines(lines []string) []string {
	var out []string
	for _, line := range lines {
		if strings.HasPrefix(line, "CHATHISTORY ") {
			out = append(out, line)
		}
	}
	return out
}

func TestChatHistoryBeforePages(t *testing.T) {
	// CHATHISTORY is Ergo-only on this fixture (Solanum/ngIRCd do not offer it).
	daemon := requireDaemon(t, "ergo")
	tls := daemon.UseTLS()
	channel := UniqueChannel()

	peer := Connect(t, daemon, UniqueNick(daemon.NickLength), tls, Options{})
	if !peer.WaitRegistered() {
		t.Fatalf("%s: peer register: %s", daemon.Name, peer.LastError())
	}
	if !peer.JoinChannel(channel) || !waitNamesEnd(peer, channel) {
		t.Fatalf("%s: peer join %s", daemon.Name, channel)
	}

	client := Connect(t, daemon, UniqueNick(daemon.NickLength), tls, Options{})
	if !client.WaitRegistered() {
		t.Fatalf("%s: client register: %s", daemon.Name, client.LastError())
	}
	if !client.WaitMotd() {
		t.Fatalf("%s: wait MOTD for CHATHISTORY= ISUPPORT", daemon.Name)
	}
	if !client.Capabilities().Contains(irc.CapabilityChatHistory) {
		if !client.WaitUntil(func(c *Client) bool {
			return c.Capabilities().Contains(irc.CapabilityChatHistory)
		}) {
			t.Fatalf("%s: negotiated ChatHistory", daemon.Name)
		}
	}

	limit := client.Session().HistoryLimit()
	total := limit + 30
	peerNetwork := peer.NetworkID
	var seedFailedAt int
	peer.Do(func(ctrl *controller.Controller) {
		for index := 1; index <= total; index++ {
			body := seedBody(channel, index)
			if !ctrl.SendToTarget(peerNetwork, channel, body) {
				seedFailedAt = index
				return
			}
		}
	})
	if seedFailedAt != 0 {
		t.Fatalf("%s: seed %d/%d", daemon.Name, seedFailedAt, total)
	}
	if !peer.Fence("seeded") {
		t.Fatalf("%s: fence after seeding %d lines", daemon.Name, total)
	}

	if !client.JoinChannel(channel) {
		t.Fatalf("%s: client join %s", daemon.Name, channel)
	}
	client.SelectChannel(channel)

	// Ergo keeps our own JOIN in the channel history and counts it in the
	// LATEST page. Without draft/event-playback it replays that item as a
	// HistServ PRIVMSG, so the newest limit items are the join and limit-1
	// seed lines.
	latestFrom := total - (limit - 1) + 1
	latestWant := expectedRange(latestFrom, total)
	if !client.WaitUntil(func(c *Client) bool {
		return indicesEqual(seedIndices(c, channel), latestWant)
	}) {
		t.Fatalf("%s: LATEST page want %v, got %v", daemon.Name, latestWant, seedIndices(client, channel))
	}
	if got := sentHistory(client.Sent(), "LATEST", channel); got != 1 {
		t.Fatalf("%s: one LATEST for %s; history lines %v", daemon.Name, channel, historySentLines(client.Sent()))
	}

	// One phase per target: both calls in one turn.
	var first, second bool
	client.Do(func(ctrl *controller.Controller) {
		first = ctrl.RequestOlderTranscriptHistory()
		second = ctrl.RequestOlderTranscriptHistory()
	})
	if !first {
		t.Fatalf("%s: first BEFORE must send", daemon.Name)
	}
	if second {
		t.Fatalf("%s: second same-turn BEFORE must refuse (one in flight)", daemon.Name)
	}

	allWant := expectedRange(1, total)
	if !client.WaitUntil(func(c *Client) bool {
		var inflight bool
		c.Do(func(ctrl *controller.Controller) {
			inflight = ctrl.OlderTranscriptHistoryInflight()
		})
		return !inflight && indicesEqual(seedIndices(c, channel), allWant)
	}) {
		var inflight bool
		client.Do(func(ctrl *controller.Controller) {
			inflight = ctrl.OlderTranscriptHistoryInflight()
		})
		t.Fatalf("%s: after first BEFORE want %v, got %v, inflight=%v",
			daemon.Name, allWant, seedIndices(client, channel), inflight)
	}
	if got := sentHistory(client.Sent(), "BEFORE", channel); got != 1 {
		t.Fatalf("%s: one BEFORE in flight per target; history lines %v",
			daemon.Name, historySentLines(client.Sent()))
	}

	// Scrolling past the start stops once history is exhausted.
	beforeCount := 1
	endedOnFalse := false
	for i := 0; i < 3; i++ {
		var sent bool
		client.Do(func(ctrl *controller.Controller) {
			sent = ctrl.RequestOlderTranscriptHistory()
		})
		if sent {
			if !client.WaitUntil(func(c *Client) bool {
				var inflight bool
				c.Do(func(ctrl *controller.Controller) {
					inflight = ctrl.OlderTranscriptHistoryInflight()
				})
				return !inflight
			}) {
				t.Fatalf("%s: wait BEFORE settle", daemon.Name)
			}
			if !client.Fence("page") {
				t.Fatalf("%s: fence after BEFORE page", daemon.Name)
			}
			beforeCount++
			if got := sentHistory(client.Sent(), "BEFORE", channel); got != beforeCount {
				t.Fatalf("%s: BEFORE count after page want %d got %d; history lines %v",
					daemon.Name, beforeCount, got, historySentLines(client.Sent()))
			}
			continue
		}
		if got := sentHistory(client.Sent(), "BEFORE", channel); got != beforeCount {
			t.Fatalf("%s: false call must write no BEFORE; history lines %v",
				daemon.Name, historySentLines(client.Sent()))
		}
		endedOnFalse = true
		break
	}
	if !endedOnFalse {
		t.Fatalf("%s: exhaustion loop must end on a false RequestOlderTranscriptHistory", daemon.Name)
	}
	if !indicesEqual(seedIndices(client, channel), allWant) {
		t.Fatalf("%s: transcript must stay complete with no duplicates; got %v",
			daemon.Name, seedIndices(client, channel))
	}
}
