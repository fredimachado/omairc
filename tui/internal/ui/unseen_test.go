package ui

// This file covers the jump-to-first-new affordance: the "↓ new" header marker
// that arms when rows arrive while the reader is scrolled up, and the Alt+U
// chord that lands on them. It mirrors TranscriptList's firstUnseenIndex,
// jumpArmed, and jumpToUnseen.

import (
	"fmt"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// unseenDemoModel seeds the demo on a frozen clock, pads the selected #omarchy
// transcript past a viewport, and leaves it following the end.
func unseenDemoModel(t *testing.T) (*Model, *demo.DemoServer) {
	t.Helper()
	ctrl := controller.New()
	ctrl.SetClock(session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)))
	d := demo.New()
	if !d.Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := New(ctrl, nil)
	m = resizeModel(t, m, 118, 30)
	// The selected conversation is focused, so these pad lines accrue neither
	// unread nor a mark; they only make the transcript taller than the viewport
	// so Page Up can detach.
	for i := 0; i < 40; i++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=pad-%d :nora!u@h PRIVMSG #omarchy :pad line %d\r\n", i, i)))
	}
	m = updateMsg(t, m, NotifyMsg{})
	return m, d
}

// TestDetachedArrivalArmsMarker proves a row that arrives while the reader is
// scrolled up arms the marker, and Alt+U lands on it.
func TestDetachedArrivalArmsMarker(t *testing.T) {
	m, d := unseenDemoModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	before := m.transcriptRowTotal()

	d.InjectOmarchy([]byte("@msgid=arrival-1 :nora!u@h PRIVMSG #omarchy :new while reading\r\n"))
	m = updateMsg(t, m, NotifyMsg{})

	if !m.jumpArmed() {
		t.Fatal("an arrival while scrolled up must arm the jump")
	}
	if m.firstUnseenRow != before {
		t.Fatalf("firstUnseenRow = %d, want %d", m.firstUnseenRow, before)
	}
	if content := m.View().Content; !strings.Contains(content, "↓ new") {
		t.Fatalf("the header must show the jump marker:\n%s", content)
	}

	m = press(t, m, altKey('u'))
	if m.firstUnseenRow != -1 {
		t.Fatalf("Alt+U must consume the marker, firstUnseenRow = %d", m.firstUnseenRow)
	}
	if m.jumpArmed() {
		t.Fatal("the jump must be unarmed after landing on the first new row")
	}
}

// TestJumpWithMarkGoesToEnd proves Alt+U falls back to the newest row when the
// only reason to jump is the "New messages" mark.
func TestJumpWithMarkGoesToEnd(t *testing.T) {
	m, d := unreadDemoModel(t)
	m = plantOmarchyUnread(t, m, d)
	m = press(t, m, altKey(tea.KeyUp))
	if m.transcriptFollowEnd {
		t.Fatal("precondition: the conversation must have landed on the mark")
	}
	if !m.jumpArmed() {
		t.Fatal("a New messages mark while detached must arm the jump")
	}

	m = press(t, m, altKey('u'))
	if !m.transcriptFollowEnd {
		t.Fatal("Alt+U with only a mark must land on the newest row")
	}
}

// TestJumpNoOpWhenFollowingEnd proves the chord does nothing when the reader is
// already at the newest row.
func TestJumpNoOpWhenFollowingEnd(t *testing.T) {
	m, _ := unseenDemoModel(t)
	if !m.transcriptFollowEnd {
		t.Fatal("precondition: the pad model starts following the end")
	}
	if m.jumpArmed() {
		t.Fatal("nothing is waiting below a reader who follows the end")
	}
	m = press(t, m, altKey('u'))
	if !m.transcriptFollowEnd || m.firstUnseenRow != -1 {
		t.Fatal("Alt+U must be a no-op while following the end")
	}
}

// TestArrivalWhileFollowingEndDoesNotArm proves the marker stays cleared for a
// reader at the bottom: the new row is already visible.
func TestArrivalWhileFollowingEndDoesNotArm(t *testing.T) {
	m, d := unseenDemoModel(t)
	d.InjectOmarchy([]byte("@msgid=arrival-2 :nora!u@h PRIVMSG #omarchy :at the bottom\r\n"))
	m = updateMsg(t, m, NotifyMsg{})
	if m.firstUnseenRow != -1 {
		t.Fatalf("firstUnseenRow = %d, want -1 while following the end", m.firstUnseenRow)
	}
	if m.jumpArmed() {
		t.Fatal("following the end must not arm the jump")
	}
}

// TestShortcutSheetListsJumpToNew proves the chord is discoverable in the
// shortcuts sheet.
func TestShortcutSheetListsJumpToNew(t *testing.T) {
	m := resizeModel(t, seededModel(t), 140, 70)
	m = press(t, m, ctrlKey('/'))
	content := m.View().Content
	if !strings.Contains(content, "Alt+U") || !strings.Contains(content, "first new message") {
		t.Fatalf("shortcuts sheet missing the Alt+U row:\n%s", content)
	}
}
