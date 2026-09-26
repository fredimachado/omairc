package controller

import "testing"

// TestDisconnectStopsLiveSession pins the Disconnect / SessionIsLive seam: a
// live registered session stops when disconnected and stays registered for a
// later Start, while an unknown network reports false from both.
func TestDisconnectStopsLiveSession(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("net", "nick"))
	registerNetwork(t, transport, "nick")

	if !c.SessionIsLive("net") {
		t.Fatalf("SessionIsLive(net) = false before Disconnect")
	}
	if !c.Disconnect("net") {
		t.Fatalf("Disconnect(net) = false")
	}
	if c.SessionIsLive("net") {
		t.Fatalf("SessionIsLive(net) = true after Disconnect")
	}
	// The session stays registered so the model can start it again.
	if c.Session("net") == nil {
		t.Fatalf("Disconnect unregistered the session")
	}
	// An already-idle session reports false.
	if c.Disconnect("net") {
		t.Fatalf("second Disconnect(net) = true")
	}

	if c.Disconnect("missing") {
		t.Fatalf("Disconnect(missing) = true")
	}
	if c.SessionIsLive("missing") {
		t.Fatalf("SessionIsLive(missing) = true")
	}
}
