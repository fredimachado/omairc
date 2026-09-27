package controller

import (
	"slices"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/session"
)

// addNetwork registers an unstarted session so NetworkIDs/NetworkOrder see it.
func addNetwork(t *testing.T, c *Controller, id string) {
	t.Helper()
	config := session.DefaultSessionConfig(id, "", "irc.example", id)
	if _, err := c.AddSession(config, session.NewLoopbackTransport(), nil); err != nil {
		t.Fatalf("AddSession(%q): %v", id, err)
	}
}

func TestNetworkOrderRecordedOrderWins(t *testing.T) {
	c := New()
	for _, id := range []string{"alpha", "omarchy", "zulu"} {
		addNetwork(t, c, id)
	}
	// "gone" has no session, so it is dropped; "zulu" is not in the recorded
	// order, so it is appended in NetworkIDs() order.
	c.SetNetworkOrder([]string{"omarchy", "gone", "alpha"})

	got := c.NetworkOrder()
	want := []string{"omarchy", "alpha", "zulu"}
	if !slices.Equal(got, want) {
		t.Fatalf("NetworkOrder() = %v, want %v", got, want)
	}
}

func TestNetworkOrderEmptyRecordedOrderIsSorted(t *testing.T) {
	c := New()
	for _, id := range []string{"zulu", "alpha", "omarchy"} {
		addNetwork(t, c, id)
	}

	got := c.NetworkOrder()
	want := []string{"alpha", "omarchy", "zulu"}
	if !slices.Equal(got, want) {
		t.Fatalf("NetworkOrder() = %v, want %v", got, want)
	}
}

func TestNetworkOrderDeduplicates(t *testing.T) {
	c := New()
	for _, id := range []string{"alpha", "omarchy"} {
		addNetwork(t, c, id)
	}
	c.SetNetworkOrder([]string{"omarchy", "omarchy", "alpha", "omarchy"})

	got := c.NetworkOrder()
	want := []string{"omarchy", "alpha"}
	if !slices.Equal(got, want) {
		t.Fatalf("NetworkOrder() = %v, want %v", got, want)
	}
}
