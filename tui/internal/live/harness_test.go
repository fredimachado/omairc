//go:build live

package live

import (
	"fmt"
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
			t.Fatalf("%s: expected one primary NICK, count %d", daemon.Name, nickHits)
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
		if capEndAt < 0 {
			t.Fatalf("%s: missing CAP END before NICK order check", daemon.Name)
		}
		if nickAt < 0 || nickAt >= capEndAt {
			t.Fatalf("%s: NICK must precede CAP END (nick@%d end@%d)", daemon.Name, nickAt, capEndAt)
		}

		fenceToken := "fence-" + nick
		nextSerial := FenceSerial()
		decoys := []string{
			"PING :" + fenceToken,
			fmt.Sprintf("PING :%s-%d", fenceToken, nextSerial),
		}
		for _, decoy := range decoys {
			if !client.Session().SendRaw(decoy) {
				t.Fatalf("%s: send decoy PING", daemon.Name)
			}
		}
		if !client.Fence(fenceToken) {
			t.Fatalf("%s: fence timed out", daemon.Name)
		}
		sent = client.Sent()
		wirePrefix := "PING :" + fenceToken + "-"
		var fencePings []string
		for _, line := range sent {
			if !strings.HasPrefix(line, wirePrefix) {
				continue
			}
			isDecoy := false
			for _, decoy := range decoys {
				if line == decoy {
					isDecoy = true
					break
				}
			}
			if !isDecoy {
				fencePings = append(fencePings, line)
			}
		}
		if len(fencePings) != 1 {
			t.Fatalf("%s: expected one fence PING distinct from decoys, count %d",
				daemon.Name, len(fencePings))
		}
		for _, decoy := range decoys {
			found := false
			for _, line := range sent {
				if line == decoy {
					found = true
					break
				}
			}
			if !found {
				t.Fatalf("%s: missing decoy PING in sent log", daemon.Name)
			}
			if fencePings[0] == decoy {
				t.Fatalf("%s: fence PING matched a decoy", daemon.Name)
			}
		}
		wireToken := strings.TrimPrefix(fencePings[0], "PING :")
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
			t.Fatalf("%s: missing PONG for fence wire token", daemon.Name)
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
		if len(capIndexes) != 2 {
			t.Fatalf("%s: expected exactly two CAP LS lines, count %d", daemon.Name, len(capIndexes))
		}
		if capIndexes[1] < n {
			t.Fatalf("%s: second CAP LS must be at index >= %d, got %d", daemon.Name, n, capIndexes[1])
		}

		secondNick := UniqueNick(daemon.NickLength)
		secondID := client.ConnectAdditional(daemon, secondNick, daemon.UseTLS(), Options{})
		if !client.WaitRegisteredNetwork(secondID) {
			t.Fatalf("%s: second network register: %s", daemon.Name, client.LastError())
		}
		secondNickLine := "NICK " + secondNick
		primarySecond := 0
		for _, line := range client.Sent() {
			if line == secondNickLine {
				primarySecond++
			}
		}
		if primarySecond != 0 {
			t.Fatalf("%s: primary sent log must not contain second NICK, count %d",
				daemon.Name, primarySecond)
		}
		secondHits := 0
		for _, line := range client.SentFor(secondID) {
			if line == secondNickLine {
				secondHits++
			}
		}
		if secondHits != 1 {
			t.Fatalf("%s: second network must record one NICK, count %d", daemon.Name, secondHits)
		}
	}
}
