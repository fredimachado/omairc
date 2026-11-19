//go:build live

package live

import (
	"strings"
	"testing"
)

// Mirrors `client/tests/live/register.rs`.

func TestRegisterPlain(t *testing.T) {
	for _, daemon := range requireLive(t) {
		if daemon.PlainPort == 0 {
			continue
		}
		client := Connect(t, daemon, UniqueNick(daemon.NickLength), false, Options{})
		if !client.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, client.LastError())
		}
	}
}

func TestRegisterTLS(t *testing.T) {
	for _, daemon := range requireLive(t) {
		if daemon.TLSPort == 0 {
			continue
		}
		client := Connect(t, daemon, UniqueNick(daemon.NickLength), true, Options{})
		if !client.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, client.LastError())
		}
	}
}

func TestNickInUse(t *testing.T) {
	for _, daemon := range requireLive(t) {
		nick := UniqueNick(max(daemon.NickLength-1, 1))
		useTLS := daemon.UseTLS()
		first := Connect(t, daemon, nick, useTLS, Options{})
		if !first.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, first.LastError())
		}
		second := Connect(t, daemon, nick, useTLS, Options{})
		if !second.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, second.LastError())
		}
		assigned := second.Nick()
		if !strings.EqualFold(assigned, nick+"_") && !strings.EqualFold(assigned, nick+"2") {
			t.Fatalf("%s assigned %s", daemon.Name, assigned)
		}
		if !second.HasServerLabel("433") {
			t.Fatalf("%s: no 433 on Status", daemon.Name)
		}
	}
}

func TestNickLength(t *testing.T) {
	for _, daemon := range requireLive(t) {
		const tooLong = "abcdefghij"
		client := Connect(t, daemon, tooLong, daemon.UseTLS(), Options{})
		if daemon.NickLength < len(tooLong) {
			if !client.WaitFailed() {
				t.Fatalf("%s: %s", daemon.Name, client.LastError())
			}
		} else if !client.WaitRegistered() {
			t.Fatalf("%s: %s", daemon.Name, client.LastError())
		}
	}
}
