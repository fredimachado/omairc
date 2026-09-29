package controller_test

// This file proves the Phase 8 members/presence/typing reads on the demo seed.
// It is an external test package: an in-package controller_test.go cannot
// import internal/demo, because demo imports internal/controller and Go
// rejects that as an import cycle in an internal test.

import (
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// seededDemo attaches the demo world on a frozen clock on a day far from the
// seeded 2026-09-12 transcript minutes. The clock is not advanced, so typing
// hints stay live, and c.now() never shares a HH:mm with the seeded anna DM row
// (10:12), so the typing footer never groups by accident.
func seededDemo(t *testing.T) (*controller.Controller, *demo.DemoServer) {
	t.Helper()
	c := controller.New()
	clock := session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC))
	c.SetClock(clock)
	d := demo.New()
	if !d.Attach(c, true) {
		t.Fatalf("demo Attach failed: %q", d.LastError())
	}
	return c, d
}

func conversationRow(t *testing.T, c *controller.Controller, networkID, target string) controller.ConversationSnapshot {
	t.Helper()
	for _, row := range c.Conversations() {
		if row.NetworkID == networkID && row.Conversation == target {
			return row
		}
	}
	t.Fatalf("conversation row %s/%s not found in %+v", networkID, target, c.Conversations())
	return controller.ConversationSnapshot{}
}

func memberRow(c *controller.Controller, nick string) (controller.MemberSnapshot, bool) {
	for _, row := range c.Members() {
		if row.Nick == nick {
			return row, true
		}
	}
	return controller.MemberSnapshot{}, false
}

// TestNickIsTypingSeededDemo covers the Qt nickIsTyping contract on the demo
// world: anna types in both #omarchy and the anna direct, the comparison is
// case-insensitive, and a non-typing or empty nick is false.
func TestNickIsTypingSeededDemo(t *testing.T) {
	c, _ := seededDemo(t)

	c.SelectConversation("omarchy", "#omarchy")
	if !c.NickIsTyping("anna") {
		t.Fatalf("NickIsTyping(anna) in #omarchy = false, typing=%v", c.TypingNicks())
	}
	if !c.NickIsTyping("ANNA") {
		t.Fatalf("NickIsTyping(ANNA) = false, want case-insensitive true")
	}
	if c.NickIsTyping("dax") {
		t.Fatalf("NickIsTyping(dax) = true, want false")
	}
	if c.NickIsTyping("") {
		t.Fatalf("NickIsTyping(\"\") = true, want false")
	}

	c.SelectConversation("omarchy", "anna")
	if !c.NickIsTyping("anna") {
		t.Fatalf("NickIsTyping(anna) in the anna DM = false, typing=%v", c.TypingNicks())
	}
	if !c.NickIsTyping("AnNa") {
		t.Fatalf("NickIsTyping(AnNa) in the anna DM = false, want case-insensitive true")
	}
}

// TestTranscriptTypingIndicatorSeededDemo covers the Qt typingRow footer on the
// demo world: the anna DM shows the footer ungrouped, a channel never shows it,
// Status never shows it, and no selection returns the zero values.
func TestTranscriptTypingIndicatorSeededDemo(t *testing.T) {
	c, _ := seededDemo(t)

	// The anna DM's only seeded row is a live peer chat at 10:12 on 2026-09-12;
	// the clock is 09:00 on 2026-09-26, so the footer does not group.
	c.SelectConversation("omarchy", "anna")
	nick, grouped, show := c.TranscriptTypingIndicator()
	if nick != "anna" {
		t.Fatalf("TranscriptTypingIndicator nick = %q, want anna", nick)
	}
	if !show {
		t.Fatalf("TranscriptTypingIndicator show = false in the anna DM")
	}
	if grouped {
		t.Fatalf("TranscriptTypingIndicator grouped = true, want false (minute differs)")
	}

	// A channel never shows the footer, even though anna is typing in #omarchy.
	c.SelectConversation("omarchy", "#omarchy")
	if _, _, show := c.TranscriptTypingIndicator(); show {
		t.Fatalf("TranscriptTypingIndicator show = true in #omarchy, want false")
	}

	// Status never shows it, even while the anna DM stays selected.
	c.SelectConversation("omarchy", "anna")
	c.OpenStatus("omarchy")
	if !c.ConsoleOpen() {
		t.Fatalf("ConsoleOpen = false after OpenStatus")
	}
	if _, _, show := c.TranscriptTypingIndicator(); show {
		t.Fatalf("TranscriptTypingIndicator show = true on Status, want false")
	}

	// No selection is safe and returns the zero values.
	c.ClearConversationSelection()
	if nick, grouped, show := c.TranscriptTypingIndicator(); nick != "" || grouped || show {
		t.Fatalf("TranscriptTypingIndicator with no selection = (%q, %v, %v), want zero", nick, grouped, show)
	}
}

// TestTranscriptTypingIndicatorGroupsInTheClockZone proves the footer's minute
// comparison is zone-correct. The reducer keeps a parsed server-time tag in UTC
// (translator.go's ircServerTimeOf) while the clock is local, so formatting both
// raw compared two different zones and never grouped outside UTC: a peer whose
// message arrived in the current minute still repeated its nick. Qt localizes
// both sides (displayTime calls toLocalTime; currentTranscriptMinute reads the
// local date), so the row and the placeholder must be compared in the clock's
// own zone.
func TestTranscriptTypingIndicatorGroupsInTheClockZone(t *testing.T) {
	// The anna DM's only seeded row is 2026-09-12T10:12:00Z, and the clock sits
	// 20 seconds later, so the row and the footer share one displayed minute in
	// every zone below.
	seeded := time.Date(2026, 9, 12, 10, 12, 0, 0, time.UTC)
	zones := []*time.Location{
		time.UTC,
		time.FixedZone("UTC+10", 10*60*60),
		time.FixedZone("UTC-05", -5*60*60),
	}
	for _, zone := range zones {
		c := controller.New()
		c.SetClock(session.NewFakeClock(seeded.Add(20 * time.Second).In(zone)))
		d := demo.New()
		if !d.Attach(c, true) {
			t.Fatalf("%s: demo Attach failed: %q", zone, d.LastError())
		}
		c.SelectConversation("omarchy", "anna")

		nick, grouped, show := c.TranscriptTypingIndicator()
		if nick != "anna" || !show {
			t.Fatalf("%s: indicator = (%q, show %v), want anna shown", zone, nick, show)
		}
		if !grouped {
			t.Fatalf("%s: a peer row in the current displayed minute must group", zone)
		}
	}
}

// TestTranscriptTypingIndicatorUngroupsThePreviousMinute keeps the minute gate
// itself intact in a non-UTC zone: the footer still takes its own header once
// the peer's row is a different displayed minute, matching Qt's 1s minute-roll
// timer. The hint is seeded fresh under each clock so it stays live and the
// assertion cannot pass vacuously on a pruned hint.
func TestTranscriptTypingIndicatorUngroupsThePreviousMinute(t *testing.T) {
	zone := time.FixedZone("UTC+10", 10*60*60)
	// The seeded row is 20:12 local; the clock is one displayed minute later.
	clock := time.Date(2026, 9, 12, 10, 13, 0, 0, time.UTC).In(zone)
	c := controller.New()
	c.SetClock(session.NewFakeClock(clock))
	d := demo.New()
	if !d.Attach(c, true) {
		t.Fatalf("demo Attach failed: %q", d.LastError())
	}
	c.SelectConversation("omarchy", "anna")

	_, grouped, show := c.TranscriptTypingIndicator()
	if !show {
		t.Fatal("the typing hint must stay live for the assertion to mean anything")
	}
	if grouped {
		t.Fatal("a peer row from the previous displayed minute must not group")
	}
}

// TestMemberBotFlagSeededDemo covers MemberSnapshot.Bot: the demo marks dax a
// bot via draft/metadata-2 and leaves anna a normal member.
func TestMemberBotFlagSeededDemo(t *testing.T) {
	c, _ := seededDemo(t)
	c.SelectConversation("omarchy", "#omarchy")

	dax, ok := memberRow(c, "dax")
	if !ok {
		t.Fatalf("dax missing from #omarchy members: %+v", c.Members())
	}
	if !dax.Bot {
		t.Fatalf("MemberSnapshot(dax).Bot = false, want true")
	}
	anna, ok := memberRow(c, "anna")
	if !ok {
		t.Fatalf("anna missing from #omarchy members: %+v", c.Members())
	}
	if anna.Bot {
		t.Fatalf("MemberSnapshot(anna).Bot = true, want false")
	}
}

// TestDirectPresenceGatedOnAwayNotify covers the DM sidebar presence gate. The
// demo negotiates away-notify, so the anna DM paints online from the shared
// #omarchy; once the network no longer advertises away-notify the dot clears,
// matching the gate that hides another member's away dot.
func TestDirectPresenceGatedOnAwayNotify(t *testing.T) {
	c, _ := seededDemo(t)
	c.SelectConversation("omarchy", "anna")

	if row := conversationRow(t, c, "omarchy", "anna"); row.Presence != "online" {
		t.Fatalf("anna DM presence with away-notify = %q, want online", row.Presence)
	}

	// Replace the negotiated set with one that lacks away-notify, keeping the
	// capabilities that do not gate presence so the change is surgical.
	replacement := irc.CapabilitySet(0)
	for _, capability := range []irc.Capability{
		irc.CapabilityBatch,
		irc.CapabilityMemberMetadata,
		irc.CapabilityMessageTags,
	} {
		replacement.Insert(capability)
	}
	c.CapabilitiesChanged("omarchy", replacement)
	// Republish on the controller's normal path so the cached snapshot reflects
	// the new capability set.
	c.Publish(irc.ViewNotify{Conversations: true})

	if row := conversationRow(t, c, "omarchy", "anna"); row.Presence != "" {
		t.Fatalf("anna DM presence without away-notify = %q, want empty", row.Presence)
	}
}
