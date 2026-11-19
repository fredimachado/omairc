//go:build live

package live

import (
	"strings"
	"testing"
)

// Mirrors `client/tests/live/harness.rs`.

func TestOutboundLogAndFence(t *testing.T) {
	for _, daemon := range requireLive(t) {
		nick := UniqueNick(daemon.NickLength)
		client := Connect(t, daemon, nick, daemon.UseTLS(), Options{ReconnectEnabled: true})
		if !client.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, client.LastError())
		}

		nickLine := "NICK " + nick
		nickHits := 0
		for _, line := range client.Sent() {
			if line == nickLine {
				nickHits++
			}
		}
		if nickHits != 1 {
			t.Fatalf("%s: expected one %s, got %#v", daemon.Name, nickLine, client.Sent())
		}

		fenceToken := "fence-" + nick
		if !client.Fence(fenceToken) {
			t.Fatalf("%s: fence %s timed out", daemon.Name, fenceToken)
		}
		pingLine := "PING :" + fenceToken
		foundPing := false
		for _, line := range client.Sent() {
			if line == pingLine {
				foundPing = true
				break
			}
		}
		if !foundPing {
			t.Fatalf("%s: missing %s in %#v", daemon.Name, pingLine, client.Sent())
		}

		n := len(client.Sent())
		before := append([]string(nil), client.Sent()...)
		if !client.ForceDisconnect() {
			t.Fatalf("%s: ForceDisconnect while registered", daemon.Name)
		}
		if !client.WaitReconnecting() {
			t.Fatalf("%s: WaitReconnecting", daemon.Name)
		}
		if !client.WaitRegisteredAgain() {
			t.Fatalf("%s: WaitRegisteredAgain: %s", daemon.Name, client.LastError())
		}

		after := client.Sent()
		if len(after) < n {
			t.Fatalf("%s: sent log shrank after reconnect (%d -> %d)", daemon.Name, n, len(after))
		}
		for i := 0; i < n; i++ {
			if after[i] != before[i] {
				t.Fatalf("%s: first %d sent lines changed after reconnect at %d: %q -> %q",
					daemon.Name, n, i, before[i], after[i])
			}
		}
		capLS := 0
		for _, line := range after {
			if strings.HasPrefix(line, "CAP LS") {
				capLS++
			}
		}
		if capLS != 2 {
			t.Fatalf("%s: expected two CAP LS lines after reconnect, got %d in %#v",
				daemon.Name, capLS, after)
		}
	}
}
