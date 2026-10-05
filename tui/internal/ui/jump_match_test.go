package ui

import (
	"fmt"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

func TestJumpMatchesTopicAboveNameAndKeepsNetwork(t *testing.T) {
	m := seededModel(t)
	m.openJump()

	m.jump.input.SetValue("builders")
	entries := m.jumpEntries()
	if len(entries) != 1 || entries[0].target != "#omarchy" || entries[0].networkID != "omarchy" {
		t.Fatalf("builders entries = %+v, want the omarchy #omarchy topic", entries)
	}
	if entries[0].label == "#omarchy" || entries[0].label == "" {
		t.Fatalf("duplicate channel label = %q, want the network name on the row", entries[0].label)
	}

	m.jump.input.SetValue("#omarchy")
	entries = m.jumpEntries()
	// The OFTC topic also contains "#omarchy". That detail hit must not
	// reorder the two name matches, so the sidebar-first row stays first.
	if len(entries) != 2 ||
		entries[0].target != "#omarchy" || entries[0].networkID != "omarchy" ||
		entries[1].target != "#omarchy" || entries[1].networkID != "oftc" ||
		entries[0].label == "#omarchy" || entries[1].label == "#omarchy" {
		t.Fatalf("#omarchy entries = %+v, want omarchy then oftc, each with its network", entries)
	}

	m.jump.input.SetValue("build")
	entries = m.jumpEntries()
	// "build" is inside #omarchy's "builders" topic, so that row ties with
	// #lab. #build wins on the name, then sidebar order.
	if len(entries) != 3 ||
		entries[0].target != "#build" || entries[0].networkID != "oftc" ||
		entries[1].target != "#omarchy" || entries[1].networkID != "omarchy" ||
		entries[2].target != "#lab" || entries[2].networkID != "oftc" {
		t.Fatalf("build entries = %+v, want #build, then #omarchy, then #lab", entries)
	}

	realname := "Anna Vale"
	m.ctrl.Apply(irc.AwayEvent{
		NetworkID:    "omarchy",
		Nick:         "anna",
		Realname:     &realname,
		RealnameOnly: true,
	})
	placeholder := "unknown"
	m.ctrl.Apply(irc.AwayEvent{
		NetworkID:    "omarchy",
		Nick:         "dax",
		Realname:     &placeholder,
		RealnameOnly: true,
	})

	m.jump.input.SetValue("vale")
	entries = m.jumpEntries()
	if len(entries) != 1 || entries[0].target != "anna" || entries[0].networkID != "omarchy" {
		t.Fatalf("vale entries = %+v, want anna", entries)
	}

	m.jump.input.SetValue("unknown")
	if entries = m.jumpEntries(); len(entries) != 0 {
		t.Fatalf("placeholder real name matched: %+v", entries)
	}
}

func TestJumpCapsAfterRank(t *testing.T) {
	m := seededModel(t)
	m.openJump()
	before := m.jumpEntries()
	if len(before) == 0 {
		t.Fatal("jump entries are empty before the extra channels")
	}
	first := before[0]
	m.closeJump()

	nick := m.ctrl.CurrentNick()
	if nick == "" {
		t.Fatal("demo nick is empty")
	}
	// Joined channels sort by name, so a joined #c00 becomes the first
	// sidebar row. Parting after the self join keeps the channel and parks
	// it after the channels that were already joined. #zzcap is then past
	// the 20-row cap until a name match pulls it forward.
	for index := 0; index < 20; index++ {
		channel := fmt.Sprintf("#c%02d", index)
		m.ctrl.Apply(irc.JoinEvent{NetworkID: "omarchy", Channel: channel, Nick: nick})
		m.ctrl.Apply(irc.TopicEvent{NetworkID: "omarchy", Channel: channel, Topic: "zzcap notes"})
		m.ctrl.Apply(irc.PartEvent{NetworkID: "omarchy", Channel: channel, Nick: nick})
	}
	m.ctrl.Apply(irc.JoinEvent{NetworkID: "omarchy", Channel: "#zzcap", Nick: nick})
	m.ctrl.Apply(irc.PartEvent{NetworkID: "omarchy", Channel: "#zzcap", Nick: nick})

	m.openJump()
	entries := m.jumpEntries()
	if len(entries) != 20 || entries[0] != first {
		t.Fatalf("empty jump = %+v, want 20 rows starting at %+v", entries, first)
	}

	m.jump.input.SetValue("zzcap")
	entries = m.jumpEntries()
	if len(entries) != 20 || entries[0].target != "#zzcap" || entries[0].networkID != "omarchy" {
		t.Fatalf("zzcap entries = %+v, want 20 rows with #zzcap first", entries)
	}
	for _, entry := range entries {
		if entry.target == "#c19" {
			t.Fatalf("#c19 survived the cap: %+v", entries)
		}
	}
}
