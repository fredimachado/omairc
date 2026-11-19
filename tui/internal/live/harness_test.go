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
			t.Fatalf("%s: expected one %s, count %d", daemon.Name, nickLine, nickHits)
		}

		// Both clients send CAP LS first, then NICK before CAP END.
		sent := client.Sent()
		if len(sent) == 0 || !strings.HasPrefix(sent[0], "CAP LS") {
			t.Fatalf("%s: first sent line must start with CAP LS", daemon.Name)
		}
		nickAt, capEndAt := -1, -1
		for index, line := range sent {
			if line == nickLine && nickAt < 0 {
				nickAt = index
			}
			if line == "CAP END" && capEndAt < 0 {
				capEndAt = index
			}
		}
		if nickAt < 0 || capEndAt < 0 || nickAt >= capEndAt {
			t.Fatalf("%s: NICK must precede CAP END (nick@%d end@%d)", daemon.Name, nickAt, capEndAt)
		}

		fenceToken := "fence-" + nick
		barePing := "PING :" + fenceToken
		if !client.Session().SendRaw(barePing) {
			t.Fatalf("%s: send bare %s", daemon.Name, barePing)
		}
		if !client.Fence(fenceToken) {
			t.Fatalf("%s: fence %s timed out", daemon.Name, fenceToken)
		}
		sent = client.Sent()
		bareHits := 0
		var wirePings []string
		wirePrefix := "PING :" + fenceToken + "-"
		for _, line := range sent {
			if line == barePing {
				bareHits++
			}
			if strings.HasPrefix(line, wirePrefix) {
				wirePings = append(wirePings, line)
			}
		}
		if bareHits != 1 {
			t.Fatalf("%s: expected one %s, count %d", daemon.Name, barePing, bareHits)
		}
		if len(wirePings) != 1 {
			t.Fatalf("%s: expected one %s*, count %d", daemon.Name, wirePrefix, len(wirePings))
		}
		wireToken := strings.TrimPrefix(wirePings[0], "PING :")
		foundPong := false
		for _, message := range client.Incoming() {
			if message.Command != "PONG" || len(message.Params) == 0 {
				continue
			}
			if message.Params[len(message.Params)-1] == wireToken {
				foundPong = true
				break
			}
		}
		if !foundPong {
			t.Fatalf("%s: missing PONG for wire token %s", daemon.Name, wireToken)
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
		for index := 0; index < n; index++ {
			if after[index] != before[index] {
				t.Fatalf("%s: pre-reconnect prefix mismatch at %d", daemon.Name, index)
			}
		}
		capIndexes := make([]int, 0, 2)
		for index, line := range after {
			if strings.HasPrefix(line, "CAP LS") {
				capIndexes = append(capIndexes, index)
			}
		}
		if len(capIndexes) < 2 || capIndexes[1] < n {
			t.Fatalf("%s: second CAP LS must be at index >= %d, got %v", daemon.Name, n, capIndexes)
		}

		secondNick := UniqueNick(daemon.NickLength)
		secondID := client.ConnectAdditional(daemon, secondNick, daemon.UseTLS(), Options{})
		if !client.WaitRegisteredNetwork(secondID) {
			t.Fatalf("%s: second network register: %s", daemon.Name, client.LastError())
		}
		secondNickLine := "NICK " + secondNick
		for _, line := range client.Sent() {
			if line == secondNickLine {
				t.Fatalf("%s: primary sent log must not contain %s", daemon.Name, secondNickLine)
			}
		}
	}
}
