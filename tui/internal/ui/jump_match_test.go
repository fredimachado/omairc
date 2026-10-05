package ui

import (
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
	// The OFTC topic also contains "#omarchy", so that row ranks above the
	// name-only omarchy channel. Both rows still carry the network.
	if len(entries) != 2 ||
		entries[0].target != "#omarchy" || entries[0].networkID != "oftc" ||
		entries[1].target != "#omarchy" || entries[1].networkID != "omarchy" ||
		entries[0].label == "#omarchy" || entries[1].label == "#omarchy" {
		t.Fatalf("#omarchy entries = %+v, want oftc then omarchy, each with its network", entries)
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
