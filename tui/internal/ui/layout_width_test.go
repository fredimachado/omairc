package ui

import "testing"

func TestColumnLayoutWidthSweep(t *testing.T) {
	for width := 50; width <= 160; width++ {
		m := resizeModel(t, seededModel(t), width, 30)
		if !m.ctrl.IsChannel() || !m.ctrl.ChannelJoined() {
			t.Fatalf("width %d: seeded model must be on a joined channel", width)
		}
		sidebar, transcript, members := m.columnLayout()
		if sidebar+transcript+members != width {
			t.Fatalf("width %d: columns %d+%d+%d = %d, want %d",
				width, sidebar, transcript, members, sidebar+transcript+members, width)
		}
		if width >= transcriptMinWidth && transcript < transcriptMinWidth {
			t.Fatalf("width %d: transcript %d, want at least %d",
				width, transcript, transcriptMinWidth)
		}
	}
}

func TestColumnLayoutAt118(t *testing.T) {
	m := seededModel(t)
	sidebar, transcript, members := m.columnLayout()
	if sidebar != 29 || transcript != 67 || members != 22 {
		t.Fatalf("118 layout = %d/%d/%d, want 29/67/22", sidebar, transcript, members)
	}
}

func TestColumnLayoutAt80(t *testing.T) {
	m := resizeModel(t, seededModel(t), 80, 30)
	sidebar, transcript, members := m.columnLayout()
	if sidebar != 20 {
		t.Fatalf("80 sidebar = %d, want 20", sidebar)
	}
	if transcript < transcriptMinWidth {
		t.Fatalf("80 transcript = %d, want at least %d", transcript, transcriptMinWidth)
	}
	if members >= membersWidth {
		t.Fatalf("80 members = %d, want narrower than %d or hidden", members, membersWidth)
	}
	// At 80 the remaining budget after transcript min and sidebar is 15.
	if members != 15 {
		t.Fatalf("80 members = %d, want 15 (shrunk below preferred)", members)
	}
	if transcript != 45 {
		t.Fatalf("80 transcript = %d, want 45", transcript)
	}
}

func TestMembersAutoHideDoesNotSetMembersHidden(t *testing.T) {
	m := resizeModel(t, seededModel(t), 60, 30)
	if m.membersHidden {
		t.Fatal("narrow resize must not set membersHidden")
	}
	if m.membersVisible() {
		t.Fatal("width 60 must auto-hide the member column")
	}
	m = resizeModel(t, m, 118, 30)
	if m.membersHidden {
		t.Fatal("growing the window must not set membersHidden")
	}
	if !m.membersVisible() {
		t.Fatal("width 118 must show the member column again after auto-hide")
	}
}

func TestMembersToggleStaysHiddenAfterGrow(t *testing.T) {
	m := seededModel(t)
	m.toggleMembers()
	if !m.membersHidden {
		t.Fatal("toggleMembers must set membersHidden")
	}
	m = resizeModel(t, m, 118, 30)
	if !m.membersHidden {
		t.Fatal("resize must not clear a user-hidden member column")
	}
	if m.membersVisible() {
		t.Fatal("user-hidden members must stay hidden at 118")
	}
}
