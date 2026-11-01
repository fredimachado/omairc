package controller

import (
	"testing"
	"time"
)

func TestSuccessfulReconnectClearsStaleDisconnectError(t *testing.T) {
	c, clock := newController(t)
	config := baseConfig("lab", "tester")
	config.ReconnectEnabled = true
	transport := addAndStart(t, c, clock, config)
	registerNetwork(t, transport, "tester")

	statusLines := len(c.Console().Lines("lab"))

	transport.RemoteClose()
	if c.LastErrorFor("lab") != "Connection closed by the server" {
		t.Fatalf("LastErrorFor = %q, want %q", c.LastErrorFor("lab"), "Connection closed by the server")
	}
	if c.ConnectionStatusFor("lab") != "Reconnecting" {
		t.Fatalf("ConnectionStatusFor = %q, want Reconnecting", c.ConnectionStatusFor("lab"))
	}
	if after := len(c.Console().Lines("lab")); after < statusLines {
		t.Fatalf("disconnect dropped Status lines: before=%d after=%d", statusLines, after)
	}

	clock.Advance(time.Second)
	transport.CompleteConnect()
	registerNetwork(t, transport, "tester")

	if c.ConnectionStatusFor("lab") != "Connected" {
		t.Fatalf("ConnectionStatusFor = %q, want Connected", c.ConnectionStatusFor("lab"))
	}
	if c.LastErrorFor("lab") != "" {
		t.Fatalf("LastErrorFor = %q, want empty after registration", c.LastErrorFor("lab"))
	}
	if len(c.Console().Lines("lab")) < statusLines {
		t.Fatalf("registration dropped Status buffer lines")
	}
}

func TestInvalidAutojoinAfterRegistrationStillShowsLastError(t *testing.T) {
	c, clock := newController(t)
	config := baseConfig("lab", "tester")
	config.AutojoinChannels = []string{"#bad\x00chan"}
	transport := addAndStart(t, c, clock, config)
	registerNetwork(t, transport, "tester")

	if c.LastErrorFor("lab") != "Invalid autojoin channel" {
		t.Fatalf("LastErrorFor = %q, want Invalid autojoin channel", c.LastErrorFor("lab"))
	}
	if c.ConnectionStatusFor("lab") != "Connected" {
		t.Fatalf("ConnectionStatusFor = %q, want Connected", c.ConnectionStatusFor("lab"))
	}
}
