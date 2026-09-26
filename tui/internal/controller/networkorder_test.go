package controller

import (
	"slices"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
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

func TestNetworkCollapsedRoundTrip(t *testing.T) {
	c := New()
	addNetwork(t, c, "alpha")

	if c.IsNetworkCollapsed("alpha") {
		t.Fatal("IsNetworkCollapsed(alpha) = true before any toggle")
	}
	c.SetNetworkCollapsed("alpha", true)
	if !c.IsNetworkCollapsed("alpha") {
		t.Fatal("IsNetworkCollapsed(alpha) = false after collapsing")
	}
	c.SetNetworkCollapsed("alpha", false)
	if c.IsNetworkCollapsed("alpha") {
		t.Fatal("IsNetworkCollapsed(alpha) = true after expanding")
	}

	c.SetNetworkCollapsed("", true)
	if c.IsNetworkCollapsed("") {
		t.Fatal("IsNetworkCollapsed(\"\") = true, want false")
	}
}

func TestSetAllNetworksCollapsed(t *testing.T) {
	c := New()
	for _, id := range []string{"alpha", "omarchy", "zulu"} {
		addNetwork(t, c, id)
	}

	c.SetAllNetworksCollapsed(true)
	for _, id := range []string{"alpha", "omarchy", "zulu"} {
		if !c.IsNetworkCollapsed(id) {
			t.Fatalf("IsNetworkCollapsed(%q) = false after collapse-all", id)
		}
	}

	c.SetAllNetworksCollapsed(false)
	for _, id := range []string{"alpha", "omarchy", "zulu"} {
		if c.IsNetworkCollapsed(id) {
			t.Fatalf("IsNetworkCollapsed(%q) = true after expand-all", id)
		}
	}

	c.SetAllNetworksCollapsed(true)
	addNetwork(t, c, "bravo")
	if c.IsNetworkCollapsed("bravo") {
		t.Fatal("network added after collapse-all is collapsed, want expanded")
	}
}

func TestMoveNetwork(t *testing.T) {
	c := New()
	for _, id := range []string{"alpha", "omarchy", "zulu"} {
		addNetwork(t, c, id)
	}
	c.SetNetworkOrder([]string{"alpha", "omarchy", "zulu"})

	if !c.MoveNetwork("omarchy", -1) {
		t.Fatal("MoveNetwork(omarchy, -1) = false, want true")
	}
	if got, want := c.NetworkOrder(), []string{"omarchy", "alpha", "zulu"}; !slices.Equal(got, want) {
		t.Fatalf("NetworkOrder() = %v, want %v", got, want)
	}

	if !c.MoveNetwork("omarchy", 1) {
		t.Fatal("MoveNetwork(omarchy, 1) = false, want true")
	}
	if got, want := c.NetworkOrder(), []string{"alpha", "omarchy", "zulu"}; !slices.Equal(got, want) {
		t.Fatalf("NetworkOrder() = %v, want %v", got, want)
	}

	for _, move := range []struct {
		id    string
		delta int
	}{
		{"alpha", -1},
		{"zulu", 1},
		{"nope", 1},
		{"alpha", 0},
	} {
		if c.MoveNetwork(move.id, move.delta) {
			t.Fatalf("MoveNetwork(%q, %d) = true, want false", move.id, move.delta)
		}
	}
	if got, want := c.NetworkOrder(), []string{"alpha", "omarchy", "zulu"}; !slices.Equal(got, want) {
		t.Fatalf("NetworkOrder() = %v after rejected moves, want %v", got, want)
	}
}

func TestCollapseDoesNotMutateConversationsOrder(t *testing.T) {
	c := New()
	for _, id := range []string{"alpha", "omarchy", "zulu"} {
		addNetwork(t, c, id)
	}
	// Attach a little sidebar state so the snapshot is not trivially empty.
	key := c.reducer.ConversationKey("alpha", "#room")
	c.reducer.EnsureConversation(key, "#room", irc.CauseChannelState)
	c.Publish(irc.ViewNotify{Conversations: true})

	conversationsBefore := c.Conversations()
	orderBefore := c.NetworkOrder()

	c.SetNetworkCollapsed("alpha", true)
	c.SetAllNetworksCollapsed(true)
	c.SetAllNetworksCollapsed(false)

	if got := c.Conversations(); !slices.Equal(got, conversationsBefore) {
		t.Fatalf("Conversations() = %v after collapse toggles, want %v", got, conversationsBefore)
	}
	if got := c.NetworkOrder(); !slices.Equal(got, orderBefore) {
		t.Fatalf("NetworkOrder() = %v after collapse toggles, want %v", got, orderBefore)
	}
}
