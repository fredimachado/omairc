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

// TestAltUReattachesWhenTheTargetIsTheEnd is the short-arrival repro: a few
// rows land while the reader is scrolled up, Alt+U puts the newest row on the
// bottom line, and the view follows the end. A later arrival stays followed
// and does not bring ↓ new back.
func TestAltUReattachesWhenTheTargetIsTheEnd(t *testing.T) {
	m, d := unseenDemoModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	for i := 0; i < 3; i++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=few-%d :nora!u@h PRIVMSG #omarchy :arrive %d\r\n", i, i)))
	}
	m = updateMsg(t, m, NotifyMsg{})
	if !m.jumpArmed() {
		t.Fatal("arrivals while scrolled up must arm the jump")
	}
	if !strings.Contains(m.View().Content, "↓ new") {
		t.Fatal("the header must show ↓ new before Alt+U")
	}
	if m.transcriptPlaceLabel() == "" {
		t.Fatal("a detached reader must show the place counter")
	}

	m = press(t, m, altKey('u'))
	if !m.transcriptFollowEnd {
		t.Fatal("Alt+U onto a target at the end must follow the end")
	}
	if m.transcriptScroll != 0 {
		t.Fatalf("scroll = %d, want 0 after reattach", m.transcriptScroll)
	}
	if m.firstUnseenRow != -1 {
		t.Fatalf("firstUnseenRow = %d, want cleared", m.firstUnseenRow)
	}
	if label := m.transcriptPlaceLabel(); label != "" {
		t.Fatalf("place after reattach = %q, want empty", label)
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if m.jumpArmed() || strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must clear once the view follows the end:\n%s", plain)
	}
	if !strings.Contains(plain, "arrive 2") {
		t.Fatal("the newest arrival must be on screen after Alt+U")
	}

	d.InjectOmarchy([]byte("@msgid=few-later :nora!u@h PRIVMSG #omarchy :after reattach\r\n"))
	m = updateMsg(t, m, NotifyMsg{})
	if !m.transcriptFollowEnd || m.firstUnseenRow != -1 || m.jumpArmed() {
		t.Fatalf("later arrival follow=%v unseen=%d armed=%v, want followed and clear",
			m.transcriptFollowEnd, m.firstUnseenRow, m.jumpArmed())
	}
	if label := m.transcriptPlaceLabel(); label != "" {
		t.Fatalf("place after the later arrival = %q, want empty", label)
	}
	plain = ansiPattern.ReplaceAllString(m.View().Content, "")
	if strings.Contains(plain, "↓ new") {
		t.Fatalf("a later arrival must not bring ↓ new back:\n%s", plain)
	}
	if !strings.Contains(plain, "after reattach") {
		t.Fatal("the later arrival must be visible while following the end")
	}
}

// TestAltUKeepsMarkerWhileUnseenRowsRemainBelow is the 180x30 repro: two Page
// Ups, then 35 arrivals. Alt+U lands on the first unseen row with more of
// that run still below the view. ↓ new must stay, each further Alt+U must
// move toward the end, and the view follows the end once a landing reaches it.
func TestAltUKeepsMarkerWhileUnseenRowsRemainBelow(t *testing.T) {
	m, d := unseenDemoModel(t)
	m = resizeModel(t, m, 180, 30)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("two Page Ups must leave follow-the-end")
	}
	before := m.transcriptRowTotal()
	const arrivals = 35
	for i := 0; i < arrivals; i++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=tail-%d :nora!u@h PRIVMSG #omarchy :tail %d\r\n", i, i)))
	}
	m = updateMsg(t, m, NotifyMsg{})
	if m.firstUnseenRow != before {
		t.Fatalf("firstUnseenRow = %d, want %d", m.firstUnseenRow, before)
	}
	if !m.jumpArmed() {
		t.Fatal("arrivals while scrolled up must arm the jump")
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(plain, "↓ new") {
		t.Fatal("the header must show ↓ new before Alt+U")
	}

	m = press(t, m, altKey('u'))
	if m.transcriptFollowEnd {
		t.Fatal("Alt+U partway through the unseen run must stay detached")
	}
	if m.firstUnseenRow != -1 {
		t.Fatalf("firstUnseenRow = %d, want the original marker consumed", m.firstUnseenRow)
	}
	if got := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body; got != "tail 0" {
		t.Fatalf("top body = %q, want the first unseen row", got)
	}
	wantPlace := fmt.Sprintf("%d of %d", before+1, before+arrivals)
	if got := m.transcriptPlaceLabel(); got != wantPlace {
		t.Fatalf("place = %q, want %q", got, wantPlace)
	}
	last := fmt.Sprintf("tail %d", arrivals-1)
	plain = ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must stay while unseen rows remain below:\n%s", plain)
	}
	if strings.Contains(plain, last) {
		t.Fatalf("the newest row %q must stay below the fold:\n%s", last, plain)
	}

	top := m.firstVisibleMessageRow()
	for pressN := 0; pressN < 8; pressN++ {
		m = press(t, m, altKey('u'))
		if m.transcriptFollowEnd {
			break
		}
		next := m.firstVisibleMessageRow()
		if next <= top {
			t.Fatalf("Alt+U %d left the top at row %d, want it to advance past %d", pressN+2, next, top)
		}
		plain = ansiPattern.ReplaceAllString(m.View().Content, "")
		if !strings.Contains(plain, "↓ new") {
			t.Fatalf("↓ new cleared before the end, place %s:\n%s", m.transcriptPlaceLabel(), plain)
		}
		if strings.Contains(plain, last) {
			t.Fatalf("Alt+U %d showed %q before reattaching:\n%s", pressN+2, last, plain)
		}
		top = next
	}
	if !m.transcriptFollowEnd || m.transcriptScroll != 0 {
		t.Fatalf("follow=%v scroll=%d, want followed at the end", m.transcriptFollowEnd, m.transcriptScroll)
	}
	if label := m.transcriptPlaceLabel(); label != "" {
		t.Fatalf("place after reattach = %q, want empty", label)
	}
	plain = ansiPattern.ReplaceAllString(m.View().Content, "")
	if m.jumpArmed() || strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must clear once the view follows the end:\n%s", plain)
	}
	if !strings.Contains(plain, last) {
		t.Fatal("the newest arrival must be on screen after the view reattaches")
	}
}

// TestAltUAdvancesOnePageWhenTheUnseenTailOutlastsTwoViewports proves a second
// Alt+U moves on through a long run instead of jumping to the end. The tail
// is more than two viewports, so the press after the first landing still has
// unseen rows below and must stay detached with ↓ new.
func TestAltUAdvancesOnePageWhenTheUnseenTailOutlastsTwoViewports(t *testing.T) {
	m, d := unseenDemoModel(t)
	m = resizeModel(t, m, 180, 30)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	before := m.transcriptRowTotal()
	extra := m.transcriptRowsHeight()*2 + 8
	if extra < 40 {
		extra = 40
	}
	for i := 0; i < extra; i++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=page-%d :nora!u@h PRIVMSG #omarchy :page %d\r\n", i, i)))
	}
	m = updateMsg(t, m, NotifyMsg{})
	if m.firstUnseenRow != before {
		t.Fatalf("firstUnseenRow = %d, want %d", m.firstUnseenRow, before)
	}

	m = press(t, m, altKey('u'))
	if m.transcriptFollowEnd {
		t.Fatal("the first Alt+U must stay detached")
	}
	if got := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body; got != "page 0" {
		t.Fatalf("top body = %q, want the first unseen row", got)
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must stay after the first landing:\n%s", plain)
	}
	top := m.firstVisibleMessageRow()

	m = press(t, m, altKey('u'))
	if m.transcriptFollowEnd {
		t.Fatal("the second Alt+U must stay detached while a viewport of unseen rows remains")
	}
	if m.transcriptScroll == 0 {
		t.Fatal("the second Alt+U must not clamp onto the end")
	}
	next := m.firstVisibleMessageRow()
	if next <= top {
		t.Fatalf("second landing top = %d, want it past %d", next, top)
	}
	last := fmt.Sprintf("page %d", extra-1)
	plain = ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must stay after the second landing:\n%s", plain)
	}
	if strings.Contains(plain, last) {
		t.Fatalf("the newest row %q must stay below the fold:\n%s", last, plain)
	}

	for pressN := 0; pressN < 8; pressN++ {
		m = press(t, m, altKey('u'))
		if m.transcriptFollowEnd {
			break
		}
		advanced := m.firstVisibleMessageRow()
		if advanced <= next {
			t.Fatalf("Alt+U left the top at row %d, want it past %d", advanced, next)
		}
		next = advanced
	}
	if !m.transcriptFollowEnd || m.transcriptScroll != 0 {
		t.Fatalf("follow=%v scroll=%d, want followed at the end", m.transcriptFollowEnd, m.transcriptScroll)
	}
	plain = ansiPattern.ReplaceAllString(m.View().Content, "")
	if m.jumpArmed() || strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must clear once the view follows the end:\n%s", plain)
	}
	if !strings.Contains(plain, last) {
		t.Fatal("the newest arrival must be on screen after the view reattaches")
	}
}

// TestAltUStaysDetachedWhenUnseenRowsFillTheViewport proves a long unseen tail
// still pins to the first new row and keeps the place counter.
func TestAltUStaysDetachedWhenUnseenRowsFillTheViewport(t *testing.T) {
	m, d := unseenDemoModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	before := m.transcriptRowTotal()
	extra := m.transcriptRowsHeight() + 8
	if extra < 12 {
		extra = 12
	}
	for i := 0; i < extra; i++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=many-%d :nora!u@h PRIVMSG #omarchy :buried %d\r\n", i, i)))
	}
	m = updateMsg(t, m, NotifyMsg{})
	if m.firstUnseenRow != before {
		t.Fatalf("firstUnseenRow = %d, want %d", m.firstUnseenRow, before)
	}

	m = press(t, m, altKey('u'))
	if m.transcriptFollowEnd {
		t.Fatal("Alt+U with more unseen rows than the viewport must stay detached")
	}
	if m.transcriptScroll == 0 {
		t.Fatal("the newest row must stay below the viewport")
	}
	if m.firstUnseenRow != -1 {
		t.Fatalf("firstUnseenRow = %d, want the marker consumed", m.firstUnseenRow)
	}
	label := m.transcriptPlaceLabel()
	if label == "" {
		t.Fatal("a detached pin must keep the place counter")
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(plain, "buried 0") {
		t.Fatalf("the first unseen row must be on screen:\n%s", plain)
	}
	last := fmt.Sprintf("buried %d", extra-1)
	if strings.Contains(plain, last) {
		t.Fatalf("the newest unseen row %q must stay below the fold:\n%s", last, plain)
	}
	if !strings.Contains(plain, label) {
		t.Fatalf("composer edge missing the place counter %q:\n%s", label, plain)
	}
}

// TestJumpWithMarkGoesToEnd proves Alt+U falls back to the newest row when the
// only reason to jump is the "New messages" mark.
func TestJumpWithMarkGoesToEnd(t *testing.T) {
	m, d := unreadDemoModel(t)
	fillDesktopUnread(d)
	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#desktop")
	})
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
