package irc

import (
	"fmt"
	"testing"
)

func directItem(actor string) InboxItem {
	return InboxItem{Kind: InboxDirect, Actor: actor}
}

// TestInboxCapAtFifty proves the newest-first list drops the oldest entry from
// the tail once it exceeds MaxInboxItems.
func TestInboxCapAtFifty(t *testing.T) {
	in := NewInbox()
	mapping := NewCaseMapping(KindRfc1459)
	for index := 0; index < MaxInboxItems+1; index++ {
		in.Append(directItem(fmt.Sprintf("actor%02d", index)), mapping)
	}
	if got := in.Count(); got != MaxInboxItems {
		t.Fatalf("Count() = %d, want %d", got, MaxInboxItems)
	}
	if got := in.At(0).Actor; got != "actor50" {
		t.Errorf("At(0).Actor = %q, want newest actor50", got)
	}
	if got := in.At(MaxInboxItems - 1).Actor; got != "actor01" {
		t.Errorf("At(49).Actor = %q, want oldest kept actor01", got)
	}
}

// TestInboxInviteDedup proves invite rows dedup by network and channel under
// the mapping, keeping the newer row at the front, while a different channel
// or network stays separate.
func TestInboxInviteDedup(t *testing.T) {
	in := NewInbox()
	mapping := NewCaseMapping(KindRfc1459)
	in.Append(InboxItem{Kind: InboxInvite, NetworkID: "net", Target: "#Chan", Actor: "a1"}, mapping)
	in.Append(InboxItem{Kind: InboxInvite, NetworkID: "net", Target: "#chan", Actor: "a2"}, mapping)
	if got := in.Count(); got != 1 {
		t.Fatalf("Count() = %d, want 1 after case-insensitive dedup", got)
	}
	if got := in.At(0).Actor; got != "a2" {
		t.Errorf("At(0).Actor = %q, want newer a2", got)
	}

	in.Append(InboxItem{Kind: InboxInvite, NetworkID: "net", Target: "#other", Actor: "a3"}, mapping)
	if got := in.Count(); got != 2 {
		t.Errorf("Count() = %d, want 2 for a different channel", got)
	}
	in.Append(InboxItem{Kind: InboxInvite, NetworkID: "elsewhere", Target: "#chan", Actor: "a4"}, mapping)
	if got := in.Count(); got != 3 {
		t.Errorf("Count() = %d, want 3 for a different network", got)
	}
}

// TestInboxMonitorDedup proves monitor-online rows dedup by network and nick
// under the mapping, while a different nick stays separate.
func TestInboxMonitorDedup(t *testing.T) {
	in := NewInbox()
	mapping := NewCaseMapping(KindRfc1459)
	in.Append(InboxItem{Kind: InboxMonitorOnline, NetworkID: "net", Actor: "Fred"}, mapping)
	in.Append(InboxItem{Kind: InboxMonitorOnline, NetworkID: "net", Actor: "fred"}, mapping)
	if got := in.Count(); got != 1 {
		t.Fatalf("Count() = %d, want 1 after case-insensitive dedup", got)
	}
	if got := in.At(0).Actor; got != "fred" {
		t.Errorf("At(0).Actor = %q, want newer fred", got)
	}

	in.Append(InboxItem{Kind: InboxMonitorOnline, NetworkID: "net", Actor: "other"}, mapping)
	if got := in.Count(); got != 2 {
		t.Errorf("Count() = %d, want 2 for a different nick", got)
	}
	in.Append(InboxItem{Kind: InboxMonitorOnline, NetworkID: "elsewhere", Actor: "fred"}, mapping)
	if got := in.Count(); got != 3 {
		t.Errorf("Count() = %d, want 3 for a different network", got)
	}
}

// TestInboxConsumeAt proves indexed removal drops exactly that row and that an
// out-of-range index is a no-op.
func TestInboxConsumeAt(t *testing.T) {
	in := NewInbox()
	mapping := NewCaseMapping(KindRfc1459)
	in.Append(directItem("a"), mapping)
	in.Append(directItem("b"), mapping)
	in.Append(directItem("c"), mapping)

	in.ConsumeAt(1)
	if got := in.Count(); got != 2 {
		t.Fatalf("Count() = %d, want 2", got)
	}
	if got := in.At(0).Actor; got != "c" {
		t.Errorf("At(0).Actor = %q, want c", got)
	}
	if got := in.At(1).Actor; got != "a" {
		t.Errorf("At(1).Actor = %q, want a", got)
	}

	in.ConsumeAt(-1)
	in.ConsumeAt(2)
	if got := in.Count(); got != 2 {
		t.Errorf("Count() = %d after out-of-range removals, want 2", got)
	}
}

// TestInboxConsumeConversation proves conversation removal matches mentions,
// highlights, directs, and kicks for one target on one network only.
func TestInboxConsumeConversation(t *testing.T) {
	in := NewInbox()
	mapping := NewCaseMapping(KindRfc1459)
	in.Append(InboxItem{Kind: InboxMention, NetworkID: "net", Target: "#chan", Actor: "a"}, mapping)
	in.Append(InboxItem{Kind: InboxHighlight, NetworkID: "net", Target: "#CHAN", Actor: "b"}, mapping)
	in.Append(InboxItem{Kind: InboxDirect, NetworkID: "net", Target: "bob", Actor: "bob"}, mapping)
	in.Append(InboxItem{Kind: InboxKick, NetworkID: "net", Target: "#chan", Actor: "op"}, mapping)
	in.Append(InboxItem{Kind: InboxInvite, NetworkID: "net", Target: "#chan", Actor: "op"}, mapping)
	in.Append(InboxItem{Kind: InboxMonitorOnline, NetworkID: "net", Actor: "bob"}, mapping)
	in.Append(InboxItem{Kind: InboxMention, NetworkID: "elsewhere", Target: "#chan", Actor: "c"}, mapping)

	in.ConsumeConversation("net", "#chan", mapping)

	if got := in.Count(); got != 4 {
		t.Fatalf("Count() = %d, want 4", got)
	}
	foundElsewhere := false
	for index := 0; index < in.Count(); index++ {
		item := in.At(index)
		if item.NetworkID == "net" && isConversationKind(item.Kind) &&
			mapping.Equals(item.Target, "#chan") {
			t.Errorf("%s row for net #chan survived ConsumeConversation", KindName(item.Kind))
		}
		if item.NetworkID == "elsewhere" && item.Kind == InboxMention {
			foundElsewhere = true
		}
	}
	if !foundElsewhere {
		t.Errorf("other-network mention did not survive")
	}
}

// TestInboxConsumeInvite proves invite removal is scoped to the matching
// network and channel.
func TestInboxConsumeInvite(t *testing.T) {
	in := NewInbox()
	mapping := NewCaseMapping(KindRfc1459)
	in.Append(InboxItem{Kind: InboxInvite, NetworkID: "net", Target: "#chan", Actor: "a"}, mapping)
	in.Append(InboxItem{Kind: InboxInvite, NetworkID: "elsewhere", Target: "#chan", Actor: "b"}, mapping)
	in.Append(InboxItem{Kind: InboxMonitorOnline, NetworkID: "net", Actor: "#chan"}, mapping)

	in.ConsumeInvite("net", "#CHAN", mapping)

	if got := in.Count(); got != 2 {
		t.Fatalf("Count() = %d, want 2", got)
	}
	if got := in.At(0).Kind; got != InboxMonitorOnline {
		t.Errorf("At(0).Kind = %s, want monitorOnline", KindName(got))
	}
	if got := in.At(1).NetworkID; got != "elsewhere" {
		t.Errorf("At(1).NetworkID = %q, want elsewhere", got)
	}
}

// TestInboxConsumeMonitor proves monitor removal is scoped to the matching
// network and nick.
func TestInboxConsumeMonitor(t *testing.T) {
	in := NewInbox()
	mapping := NewCaseMapping(KindRfc1459)
	in.Append(InboxItem{Kind: InboxMonitorOnline, NetworkID: "net", Actor: "bob"}, mapping)
	in.Append(InboxItem{Kind: InboxMonitorOnline, NetworkID: "net", Actor: "carol"}, mapping)
	in.Append(InboxItem{Kind: InboxMonitorOnline, NetworkID: "elsewhere", Actor: "bob"}, mapping)
	in.Append(InboxItem{Kind: InboxDirect, NetworkID: "net", Target: "bob"}, mapping)

	in.ConsumeMonitor("net", "BOB", mapping)

	if got := in.Count(); got != 3 {
		t.Fatalf("Count() = %d, want 3", got)
	}
	for index := 0; index < in.Count(); index++ {
		item := in.At(index)
		if item.Kind == InboxMonitorOnline && item.NetworkID == "net" && item.Actor == "bob" {
			t.Errorf("matching monitor row survived at %d", index)
		}
	}
}

// TestInboxPurgeNetwork proves every row for one network is dropped and other
// networks stay.
func TestInboxPurgeNetwork(t *testing.T) {
	in := NewInbox()
	mapping := NewCaseMapping(KindRfc1459)
	in.Append(InboxItem{Kind: InboxMention, NetworkID: "net", Target: "#chan"}, mapping)
	in.Append(InboxItem{Kind: InboxInvite, NetworkID: "net", Target: "#other"}, mapping)
	in.Append(InboxItem{Kind: InboxDirect, NetworkID: "elsewhere", Target: "bob"}, mapping)
	in.Append(InboxItem{Kind: InboxMonitorOnline, NetworkID: "elsewhere", Actor: "carol"}, mapping)

	in.PurgeNetwork("net")

	if got := in.Count(); got != 2 {
		t.Fatalf("Count() = %d, want 2", got)
	}
	for index := 0; index < in.Count(); index++ {
		if got := in.At(index).NetworkID; got != "elsewhere" {
			t.Errorf("At(%d).NetworkID = %q, want elsewhere", index, got)
		}
	}
}

// TestInboxAtAndItemsCovering proves out-of-range access never panics and that
// Items returns a copy detached from the store.
func TestInboxAtAndItemsCovering(t *testing.T) {
	in := NewInbox()
	if got := in.At(0); got != (InboxItem{}) {
		t.Errorf("At(0) on empty = %+v, want zero", got)
	}
	if got := in.At(-1); got != (InboxItem{}) {
		t.Errorf("At(-1) = %+v, want zero", got)
	}

	mapping := NewCaseMapping(KindRfc1459)
	in.Append(directItem("a"), mapping)
	if got := in.At(5); got != (InboxItem{}) {
		t.Errorf("At(5) out of range = %+v, want zero", got)
	}
	if got := in.At(-3); got != (InboxItem{}) {
		t.Errorf("At(-3) = %+v, want zero", got)
	}

	items := in.Items()
	if len(items) != 1 {
		t.Fatalf("Items() length = %d, want 1", len(items))
	}
	items[0].Actor = "mutated"
	items = append(items, directItem("extra"))
	if got := in.Count(); got != 1 {
		t.Errorf("Count() = %d after mutating the copy, want 1", got)
	}
	if got := in.At(0).Actor; got != "a" {
		t.Errorf("At(0).Actor = %q after mutating the copy, want a", got)
	}
}

// TestKindName covers every kind and an unknown value.
func TestKindName(t *testing.T) {
	cases := []struct {
		kind InboxKind
		want string
	}{
		{InboxMention, "mention"},
		{InboxHighlight, "highlight"},
		{InboxDirect, "direct"},
		{InboxInvite, "invite"},
		{InboxMonitorOnline, "monitorOnline"},
		{InboxKick, "kick"},
		{InboxKind(99), ""},
	}
	for _, testCase := range cases {
		if got := KindName(testCase.kind); got != testCase.want {
			t.Errorf("KindName(%d) = %q, want %q", testCase.kind, got, testCase.want)
		}
	}
}

// TestInboxLabel covers every kind and an unknown value.
func TestInboxLabel(t *testing.T) {
	cases := []struct {
		item InboxItem
		want string
	}{
		{InboxItem{Kind: InboxMention, Actor: "alice", Target: "#chan"}, "alice mentioned you in #chan"},
		{InboxItem{Kind: InboxHighlight, Target: "#chan"}, "Highlight in #chan"},
		{InboxItem{Kind: InboxDirect, Actor: "alice"}, "Message from alice"},
		{InboxItem{Kind: InboxInvite, Actor: "alice", Target: "#chan"}, "alice invited you to #chan"},
		{InboxItem{Kind: InboxMonitorOnline, Actor: "alice"}, "alice is online"},
		{InboxItem{Kind: InboxKick, Target: "#chan"}, "You were kicked from #chan"},
		{InboxItem{Kind: InboxKind(99)}, ""},
	}
	for _, testCase := range cases {
		if got := testCase.item.Label(); got != testCase.want {
			t.Errorf("Label(%+v) = %q, want %q", testCase.item, got, testCase.want)
		}
	}
}
